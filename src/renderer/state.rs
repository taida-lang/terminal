//! Renderer state types and pack <-> Vec marshalling (Phase 8 / TMB-020).
//!
//! This module owns the **internal** mutable representation of the
//! renderer's `ScreenBuffer` and the FFI marshalling primitives that
//! convert between the Taida-facing pack shape (immutable, value-based)
//! and a row-major `Vec<Cell>` buffer that can be mutated in place.
//!
//! ## Why this exists (TMB-020)
//!
//! The pure-Taida renderer in `taida/renderer.td` implemented
//! `BufferPut` / `BufferWrite` / `BufferFillRect` / `BufferDiff` /
//! `RenderFull` on top of `Take` / `Drop` / `Append` / `Concat`, each
//! of which is `O(N)`. Per-cell list-replace therefore degrades to
//! `O(N²)` and `BufferWrite` of width-W text on a `cols × rows = N`
//! buffer to `O(W·N)`. Hachikuma observed `composePane` 40×20 = 6081 ms
//! against an R-2 budget of 50 ms (121× over budget). `RenderFull` of
//! a 120×40 buffer reached the multi-second range.
//!
//! Phase 8 fixes this by moving the **hot-path computations** (put /
//! write / fill_rect / diff / render_full / render_ops / render_frame)
//! into Rust where the buffer is a single `Vec<Cell>` with `O(1)` cell
//! mutation and pre-sized string accumulation.
//!
//! ## Layer boundary (`TM_DESIGN.md`)
//!
//! - **Layer A (Rust)**: this module + `ops.rs` + `diff.rs`. Pure
//!   computation; no syscalls, no termios, no signals. The renderer
//!   addon entries do **not** touch stdin/stdout/stderr — they only
//!   transform values.
//! - **Layer B (ANSI facade)**: `taida/ansi.td` / `taida/style.td`
//!   continue to own ANSI escape composition. `diff.rs::render_ops`
//!   embeds the same ANSI literals so the addon can return a single
//!   `Str` without cross-module callbacks.
//! - **Layer C (renderer/widget)**: `taida/renderer.td` becomes a
//!   thin dispatch thunk that calls the Rust entries.
//!
//! ## Pack shape (frozen)
//!
//! The Taida-facing pack shapes are part of the package's public
//! surface and were locked in Phase 4 / Phase 7. The marshalling here
//! must produce **byte-identical** packs to what the pure-Taida facade
//! returned in `@a.4`, otherwise downstream code that destructures
//! `buf.cells.get(i).text` etc. breaks.
//!
//! ### `Cell` (7 fields)
//!
//! ```text
//! @(text <= " ", fg <= "", bg <= "",
//!   bold <= false, dim <= false, underline <= false, italic <= false)
//! ```
//!
//! ### `ScreenBuffer` (6 fields)
//!
//! ```text
//! @(cols <= 0, rows <= 0, cells <= @[Cell],
//!   cursor_col <= 1, cursor_row <= 1, cursor_visible <= true)
//! ```
//!
//! `cells` is a row-major flat list: index `(row - 1) * cols + (col - 1)`.
//!
//! ### Style options (BufferWrite / DiffOp.style) — 6 fields
//!
//! ```text
//! @(fg <= "", bg <= "", bold <= false, dim <= false,
//!   underline <= false, italic <= false)
//! ```
//!
//! ### `DiffOp` (5 fields)
//!
//! ```text
//! @(kind <= DiffOpKind.Write, col <= 1, row <= 1, text <= "",
//!   style <= @(fg <= "", bg <= "", bold <= false, dim <= false,
//!              underline <= false, italic <= false))
//! ```
//!
//! ### BufferDiff result (2 fields)
//!
//! ```text
//! @(ops <= @[], requires_full <= false)
//! ```
//!
//! ### RenderFrame result (2 fields)
//!
//! ```text
//! @(text <= "", next <= ScreenBuffer)
//! ```
//!
//! ## Error handling
//!
//! Every parser path uses `Result<T, RendererError>` so callers can
//! return a deterministic addon error. Out-of-bounds checks live
//! inside `ops.rs` because they need column/row context that is not
//! visible at parse time. Pack/list/string-shape errors map to
//! `RendererInvalidArg` (code `6001`).
//!
//! ## Performance discipline
//!
//! - `Vec<Cell>` is allocated **once** per buffer, never grown after
//!   `BufferNew` / `BufferResize`.
//! - Mutation entries (`buffer_put_impl`, `buffer_write_impl`, ...)
//!   pre-build the result `Vec<Cell>` by cloning the input once and
//!   mutating in place — no per-cell `O(N)` list-replace.
//! - Strings returned from `render_full` / `render_ops` use
//!   `String::with_capacity(...)` based on a conservative upper bound
//!   so the hot path avoids reallocation under typical TUI loads.

use core::ffi::c_char;

use taida_addon::bridge::{BorrowedPack, BorrowedValue, HostValueBuilder};

// ── Error codes ───────────────────────────────────────────────────

/// Renderer error band: `6xxx`. Does not collide with any of the
/// existing error bands (1xxx ReadKey, 2xxx TerminalSize/IsTerminal,
/// 3xxx RawMode, 4xxx ReadEvent, 5xxx Write).
pub mod err {
    /// An incoming pack / list / value did not have the expected
    /// shape (missing field, wrong tag, etc.). Maps to Taida-side
    /// error name `RendererInvalidArg`.
    pub const RENDERER_INVALID_ARG: u32 = 6001;
    /// An out-of-bounds put/write/fill_rect coordinate. Maps to
    /// Taida-side error name `RendererOutOfBounds`.
    pub const RENDERER_OUT_OF_BOUNDS: u32 = 6002;
    /// `BufferNew` / `BufferResize` got `cols < 1` or `rows < 1`.
    /// Maps to Taida-side error name `RendererInvalidSize`.
    pub const RENDERER_INVALID_SIZE: u32 = 6003;
    /// Host failed to allocate a return value. Maps to Taida-side
    /// error name `RendererBuildValue`.
    pub const RENDERER_BUILD_VALUE: u32 = 6004;
    /// A panic escaped the addon body. Maps to Taida-side error name
    /// `RendererPanic`.
    pub const RENDERER_PANIC: u32 = 6005;
}

// ── Internal types ────────────────────────────────────────────────

/// Style attributes for a cell. Mirrors the style sub-pack shape used
/// by `BufferWrite`, `DiffOp.style`, and the `Cell` pack itself.
///
/// All six fields are always present in the marshalling layer — the
/// pack contract requires a default value rather than `null`. This
/// avoids the silent-fallback trap from `TM_DESIGN.md` non-negotiable
/// #4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: String,
    pub bg: String,
    pub bold: bool,
    pub dim: bool,
    pub underline: bool,
    pub italic: bool,
}

impl CellStyle {
    /// Empty style: no color, no decoration. Equivalent to the
    /// Taida-side `@(fg <= "", bg <= "", bold <= false, dim <= false,
    /// underline <= false, italic <= false)` literal.
    pub fn empty() -> Self {
        Self {
            fg: String::new(),
            bg: String::new(),
            bold: false,
            dim: false,
            underline: false,
            italic: false,
        }
    }

    /// True if every field is at its default. Used by `render_full`
    /// to skip emitting `Stylize` wrappers for plain ASCII cells.
    pub fn is_empty(&self) -> bool {
        self.fg.is_empty()
            && self.bg.is_empty()
            && !self.bold
            && !self.dim
            && !self.underline
            && !self.italic
    }
}

/// One cell of the screen buffer.
///
/// `text` is the grapheme rendered at this cell. By contract it is
/// either a single grapheme (width 1) or `" "` for the placeholder
/// half of a wide cell. Empty `text` is normalized to `" "` on
/// rendering — see `ops::buffer_put_impl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub style: CellStyle,
}

impl Cell {
    /// The default cell: a single space with no style. This matches
    /// the Taida-side `Cell()` constructor literal.
    ///
    /// Used by tests and reserved for callers that want to seed a
    /// `Vec<Cell>` without re-doing the marshalling.
    #[allow(dead_code)]
    pub fn default_space() -> Self {
        Self {
            text: " ".to_string(),
            style: CellStyle::empty(),
        }
    }
}

/// Internal mutable representation of a `ScreenBuffer`.
///
/// Held only for the duration of a single addon call. Each entry
/// reads the input pack into a `BufferState`, performs the mutation,
/// then re-serializes a fresh pack for the host. The marshalling
/// cost is `O(N)` per call (one full clone + one full serialize),
/// which is the same upper bound as the pure-Taida code did
/// inadvertently — but with a constant factor that is now linear
/// in `N` instead of quadratic.
///
/// ## `row_hashes` (TMB-021 dirty-region tracking)
///
/// `row_hashes` is an opt-in cache of one [FNV-1a 64-bit hash] per row
/// over the row's `cells` slice. When present **on both sides** of
/// `diff_buffers`, the differ walks the per-row hashes first and only
/// visits cells in rows whose hashes disagree. This collapses
/// `render_frame_identical 120×40` from `~825 µs` (per-cell compare
/// over all 4800 cells) to `< 5 µs` (40 `u64` compares).
///
/// `parse_buffer` always populates `row_hashes` — the cost of hashing
/// is `O(N)` and amortises into the `O(N)` clone + serialise that
/// every addon entry already pays. Callers that build a `BufferState`
/// by hand (tests, benches, ad-hoc internal helpers) may leave
/// `row_hashes = None`; in that case `diff_buffers` falls back to the
/// slow per-cell walk transparently.
///
/// `BufferState::compute_row_hashes` is exposed so test helpers can
/// opt in explicitly.
///
/// [FNV-1a 64-bit hash]: https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function
#[derive(Clone, Debug)]
pub struct BufferState {
    pub cols: i64,
    pub rows: i64,
    pub cells: Vec<Cell>,
    pub cursor_col: i64,
    pub cursor_row: i64,
    pub cursor_visible: bool,
    /// Optional per-row content fingerprint cache. See type docs.
    /// `None` is the unconditionally-correct default; `Some(v)` must
    /// have `v.len() == rows` and each entry must equal the FNV-1a
    /// hash of the corresponding row computed by [`row_fingerprint`].
    pub row_hashes: Option<Vec<u64>>,
}

/// FNV-1a 64-bit offset basis. Public for test-time invariants.
pub const FNV1A_OFFSET_64: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime. Public for test-time invariants.
pub const FNV1A_PRIME_64: u64 = 0x0000_0100_0000_01b3;

#[inline(always)]
fn fnv1a_update(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV1A_PRIME_64);
    }
    h
}

#[inline(always)]
fn fnv1a_update_u8(h: u64, b: u8) -> u64 {
    let h = h ^ (b as u64);
    h.wrapping_mul(FNV1A_PRIME_64)
}

/// Compute a 64-bit fingerprint over a row of cells. Mixes every
/// byte of every cell's `text`, every byte of `style.fg` and `bg`,
/// and the four boolean attributes packed into a discriminator byte.
///
/// The hash is **not** cryptographic; it is a content fingerprint
/// used to skip equal rows during `diff_buffers`. A 64-bit FNV-1a
/// has a collision rate of `~2^-64`, so a missed diff would require
/// the user to encounter a hash collision **between two different
/// frames at the same row position** — well below the visual noise
/// floor of any TUI application.
pub fn row_fingerprint(cells: &[Cell]) -> u64 {
    let mut h = FNV1A_OFFSET_64;
    for cell in cells {
        // Length-prefix `text` so `["ab","c"]` and `["a","bc"]`
        // produce different hashes.
        h = fnv1a_update_u8(h, cell.text.len() as u8);
        h = fnv1a_update(h, cell.text.as_bytes());
        h = fnv1a_update_u8(h, cell.style.fg.len() as u8);
        h = fnv1a_update(h, cell.style.fg.as_bytes());
        h = fnv1a_update_u8(h, cell.style.bg.len() as u8);
        h = fnv1a_update(h, cell.style.bg.as_bytes());
        let attrs: u8 = (cell.style.bold as u8)
            | ((cell.style.dim as u8) << 1)
            | ((cell.style.underline as u8) << 2)
            | ((cell.style.italic as u8) << 3);
        h = fnv1a_update_u8(h, attrs);
        // Cell terminator so adjacent cells cannot bleed bytes
        // (e.g. `text="ab"`, `fg=""` vs `text="a"`, `fg="b"`).
        h = fnv1a_update_u8(h, 0xff);
    }
    h
}

impl BufferState {
    /// Index a `(col, row)` 1-based position into `self.cells`.
    /// Returns `None` if the coordinates are out of range.
    pub fn cell_index(&self, col: i64, row: i64) -> Option<usize> {
        if col < 1 || row < 1 || col > self.cols || row > self.rows {
            return None;
        }
        Some(((row - 1) * self.cols + (col - 1)) as usize)
    }

    /// Compute and cache `row_hashes`. After this call,
    /// `self.row_hashes` is `Some(v)` with `v.len() == self.rows`.
    ///
    /// Cost: `O(N)` where `N = cols * rows`. Idempotent (calling
    /// twice produces the same `row_hashes`).
    pub fn compute_row_hashes(&mut self) {
        if self.cols <= 0 || self.rows <= 0 {
            self.row_hashes = Some(Vec::new());
            return;
        }
        let cols = self.cols as usize;
        let rows = self.rows as usize;
        let mut hashes = Vec::with_capacity(rows);
        for r in 0..rows {
            let start = r * cols;
            let end = start + cols;
            // Guard against malformed buffers: cells_len < expected.
            if end > self.cells.len() {
                hashes.push(FNV1A_OFFSET_64);
                continue;
            }
            hashes.push(row_fingerprint(&self.cells[start..end]));
        }
        self.row_hashes = Some(hashes);
    }
}

// ── Errors ────────────────────────────────────────────────────────

/// Errors raised during pack parsing / op validation. Each variant
/// maps to one of the codes in [`err`].
#[derive(Debug)]
pub enum RendererError {
    InvalidArg(String),
    OutOfBounds(String),
    /// Reserved for future Buffer constructors (`BufferNew` / `BufferResize`)
    /// that may move into the addon. The error code is part of the
    /// public surface today so the variant is held in reserve.
    #[allow(dead_code)]
    InvalidSize(String),
}

impl RendererError {
    pub fn code(&self) -> u32 {
        match self {
            RendererError::InvalidArg(_) => err::RENDERER_INVALID_ARG,
            RendererError::OutOfBounds(_) => err::RENDERER_OUT_OF_BOUNDS,
            RendererError::InvalidSize(_) => err::RENDERER_INVALID_SIZE,
        }
    }

    pub fn message(&self) -> String {
        match self {
            RendererError::InvalidArg(m) => format!("RendererInvalidArg: {m}"),
            RendererError::OutOfBounds(m) => format!("RendererOutOfBounds: {m}"),
            RendererError::InvalidSize(m) => format!("RendererInvalidSize: {m}"),
        }
    }
}

// ── Pack field lookup helpers ─────────────────────────────────────

/// Find a pack entry by name. Returns `None` if absent. The Taida
/// `@(...)` pack is unordered so we do a linear scan; pack arity is
/// small (≤ 7 fields for any of our types) and this is not a hot
/// loop relative to the cells walk.
fn pack_get<'a>(pack: &BorrowedPack<'a>, name: &str) -> Option<BorrowedValue<'a>> {
    for (key, value) in pack.iter() {
        if key == name {
            return Some(value);
        }
    }
    None
}

fn need_pack<'a>(value: &BorrowedValue<'a>, what: &str) -> Result<BorrowedPack<'a>, RendererError> {
    value
        .as_pack()
        .ok_or_else(|| RendererError::InvalidArg(format!("{what} must be a pack")))
}

fn need_str<'a>(value: &BorrowedValue<'a>, what: &str) -> Result<String, RendererError> {
    value
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| RendererError::InvalidArg(format!("{what} must be a Str")))
}

fn need_int(value: &BorrowedValue<'_>, what: &str) -> Result<i64, RendererError> {
    value
        .as_int()
        .ok_or_else(|| RendererError::InvalidArg(format!("{what} must be an Int")))
}

fn need_bool(value: &BorrowedValue<'_>, what: &str) -> Result<bool, RendererError> {
    value
        .as_bool()
        .ok_or_else(|| RendererError::InvalidArg(format!("{what} must be a Bool")))
}

fn field_str(pack: &BorrowedPack<'_>, name: &str, default: &str) -> Result<String, RendererError> {
    match pack_get(pack, name) {
        None => Ok(default.to_string()),
        Some(v) => need_str(&v, name),
    }
}

fn field_bool(pack: &BorrowedPack<'_>, name: &str, default: bool) -> Result<bool, RendererError> {
    match pack_get(pack, name) {
        None => Ok(default),
        Some(v) => need_bool(&v, name),
    }
}

fn field_int(pack: &BorrowedPack<'_>, name: &str, default: i64) -> Result<i64, RendererError> {
    match pack_get(pack, name) {
        None => Ok(default),
        Some(v) => need_int(&v, name),
    }
}

// ── Style parsing ─────────────────────────────────────────────────

/// Parse a 6-field style sub-pack.
///
/// The pack is the literal that the facade passes for `BufferWrite`'s
/// `style` argument and for `DiffOp.style`. Missing fields default to
/// the empty style — this matches the Taida-side `CellStyle()` shape.
pub fn parse_style(value: &BorrowedValue<'_>) -> Result<CellStyle, RendererError> {
    let pack = need_pack(value, "style")?;
    Ok(CellStyle {
        fg: field_str(&pack, "fg", "")?,
        bg: field_str(&pack, "bg", "")?,
        bold: field_bool(&pack, "bold", false)?,
        dim: field_bool(&pack, "dim", false)?,
        underline: field_bool(&pack, "underline", false)?,
        italic: field_bool(&pack, "italic", false)?,
    })
}

// ── Cell parsing ──────────────────────────────────────────────────

/// Parse a 7-field cell pack.
pub fn parse_cell(value: &BorrowedValue<'_>) -> Result<Cell, RendererError> {
    let pack = need_pack(value, "cell")?;
    let style = CellStyle {
        fg: field_str(&pack, "fg", "")?,
        bg: field_str(&pack, "bg", "")?,
        bold: field_bool(&pack, "bold", false)?,
        dim: field_bool(&pack, "dim", false)?,
        underline: field_bool(&pack, "underline", false)?,
        italic: field_bool(&pack, "italic", false)?,
    };
    let text = field_str(&pack, "text", " ")?;
    let text = if text.is_empty() {
        " ".to_string()
    } else {
        text
    };
    Ok(Cell { text, style })
}

// ── Buffer parsing ────────────────────────────────────────────────

/// Parse a `ScreenBuffer` pack into a mutable `BufferState`.
///
/// Walks the `cells` list once and clones every `Cell` into a fresh
/// `Vec<Cell>`. The cost is `O(N)` per call. The cells list length
/// must equal `cols * rows`; otherwise we surface
/// `RendererInvalidArg` so the caller learns about the inconsistency
/// instead of silently truncating or padding (TM_DESIGN.md
/// non-negotiable #4).
pub fn parse_buffer(value: &BorrowedValue<'_>) -> Result<BufferState, RendererError> {
    let pack = need_pack(value, "buffer")?;
    let cols = field_int(&pack, "cols", 0)?;
    let rows = field_int(&pack, "rows", 0)?;
    let cursor_col = field_int(&pack, "cursor_col", 1)?;
    let cursor_row = field_int(&pack, "cursor_row", 1)?;
    let cursor_visible = field_bool(&pack, "cursor_visible", true)?;
    let cells_value = pack_get(&pack, "cells")
        .ok_or_else(|| RendererError::InvalidArg("buffer.cells missing".to_string()))?;
    let cells_list = cells_value
        .as_list()
        .ok_or_else(|| RendererError::InvalidArg("buffer.cells must be a list".to_string()))?;
    let expected = (cols.max(0) as usize).saturating_mul(rows.max(0) as usize);
    if cells_list.len() != expected {
        return Err(RendererError::InvalidArg(format!(
            "buffer.cells length mismatch: have {}, expected cols*rows={}",
            cells_list.len(),
            expected
        )));
    }
    let mut cells = Vec::with_capacity(expected);
    for i in 0..cells_list.len() {
        let item = cells_list
            .get(i)
            .ok_or_else(|| RendererError::InvalidArg(format!("buffer.cells[{i}] missing")))?;
        cells.push(parse_cell(&item)?);
    }
    let mut state = BufferState {
        cols,
        rows,
        cells,
        cursor_col,
        cursor_row,
        cursor_visible,
        row_hashes: None,
    };
    // TMB-021: precompute row hashes so `diff_buffers` can skip
    // unchanged rows wholesale. Cost is `O(N)` and folds into the
    // `O(N)` cells walk we already performed above.
    state.compute_row_hashes();
    Ok(state)
}

// ── Pack field name C-strings ─────────────────────────────────────
//
// These nul-terminated literals are passed to `HostValueBuilder::pack`
// as the `names` array. Keeping them in `static` C-strings avoids
// per-call CString allocation in the hot path.

const FIELD_TEXT: &core::ffi::CStr = c"text";
const FIELD_FG: &core::ffi::CStr = c"fg";
const FIELD_BG: &core::ffi::CStr = c"bg";
const FIELD_BOLD: &core::ffi::CStr = c"bold";
const FIELD_DIM: &core::ffi::CStr = c"dim";
const FIELD_UNDERLINE: &core::ffi::CStr = c"underline";
const FIELD_ITALIC: &core::ffi::CStr = c"italic";

const FIELD_COLS: &core::ffi::CStr = c"cols";
const FIELD_ROWS: &core::ffi::CStr = c"rows";
const FIELD_CELLS: &core::ffi::CStr = c"cells";
const FIELD_CURSOR_COL: &core::ffi::CStr = c"cursor_col";
const FIELD_CURSOR_ROW: &core::ffi::CStr = c"cursor_row";
const FIELD_CURSOR_VISIBLE: &core::ffi::CStr = c"cursor_visible";

const FIELD_KIND: &core::ffi::CStr = c"kind";
const FIELD_COL: &core::ffi::CStr = c"col";
const FIELD_ROW: &core::ffi::CStr = c"row";
const FIELD_STYLE: &core::ffi::CStr = c"style";

const FIELD_OPS: &core::ffi::CStr = c"ops";
const FIELD_REQUIRES_FULL: &core::ffi::CStr = c"requires_full";

const FIELD_NEXT: &core::ffi::CStr = c"next";

// ── Pack builders (Rust → host) ───────────────────────────────────

/// Build the 6-field style sub-pack.
pub fn build_style(
    builder: &HostValueBuilder,
    style: &CellStyle,
) -> *mut taida_addon::TaidaAddonValueV1 {
    let names: [*const c_char; 6] = [
        FIELD_FG.as_ptr(),
        FIELD_BG.as_ptr(),
        FIELD_BOLD.as_ptr(),
        FIELD_DIM.as_ptr(),
        FIELD_UNDERLINE.as_ptr(),
        FIELD_ITALIC.as_ptr(),
    ];
    let values = [
        builder.str(&style.fg),
        builder.str(&style.bg),
        builder.bool(style.bold),
        builder.bool(style.dim),
        builder.bool(style.underline),
        builder.bool(style.italic),
    ];
    builder.pack(&names, &values)
}

/// Build the 7-field cell pack.
pub fn build_cell(builder: &HostValueBuilder, cell: &Cell) -> *mut taida_addon::TaidaAddonValueV1 {
    let names: [*const c_char; 7] = [
        FIELD_TEXT.as_ptr(),
        FIELD_FG.as_ptr(),
        FIELD_BG.as_ptr(),
        FIELD_BOLD.as_ptr(),
        FIELD_DIM.as_ptr(),
        FIELD_UNDERLINE.as_ptr(),
        FIELD_ITALIC.as_ptr(),
    ];
    let values = [
        builder.str(&cell.text),
        builder.str(&cell.style.fg),
        builder.str(&cell.style.bg),
        builder.bool(cell.style.bold),
        builder.bool(cell.style.dim),
        builder.bool(cell.style.underline),
        builder.bool(cell.style.italic),
    ];
    builder.pack(&names, &values)
}

/// Build the 6-field `ScreenBuffer` pack.
pub fn build_buffer(
    builder: &HostValueBuilder,
    buf: &BufferState,
) -> *mut taida_addon::TaidaAddonValueV1 {
    // Build the cells list first so the pack can take the pointer.
    let mut cell_ptrs: Vec<*mut taida_addon::TaidaAddonValueV1> =
        Vec::with_capacity(buf.cells.len());
    for cell in &buf.cells {
        cell_ptrs.push(build_cell(builder, cell));
    }
    let cells_list = builder.list(&cell_ptrs);

    let names: [*const c_char; 6] = [
        FIELD_COLS.as_ptr(),
        FIELD_ROWS.as_ptr(),
        FIELD_CELLS.as_ptr(),
        FIELD_CURSOR_COL.as_ptr(),
        FIELD_CURSOR_ROW.as_ptr(),
        FIELD_CURSOR_VISIBLE.as_ptr(),
    ];
    let values = [
        builder.int(buf.cols),
        builder.int(buf.rows),
        cells_list,
        builder.int(buf.cursor_col),
        builder.int(buf.cursor_row),
        builder.bool(buf.cursor_visible),
    ];
    builder.pack(&names, &values)
}

// ── DiffOp parsing / building ─────────────────────────────────────

/// One diff operation. Mirrors the Taida-facing `DiffOp` pack shape.
#[derive(Clone, Debug)]
pub struct DiffOp {
    pub kind: i64,
    pub col: i64,
    pub row: i64,
    pub text: String,
    pub style: CellStyle,
}

/// `DiffOpKind` enum tag values. These are part of the public API
/// (`taida/renderer.td::DiffOpKind`) and must stay frozen.
pub mod diff_kind {
    pub const MOVE_TO: i64 = 0;
    pub const WRITE: i64 = 1;
    pub const CLEAR_LINE: i64 = 2;
    pub const SHOW_CURSOR: i64 = 3;
    pub const HIDE_CURSOR: i64 = 4;
}

pub fn parse_diff_op(value: &BorrowedValue<'_>) -> Result<DiffOp, RendererError> {
    let pack = need_pack(value, "diff_op")?;
    let kind = field_int(&pack, "kind", diff_kind::WRITE)?;
    let col = field_int(&pack, "col", 1)?;
    let row = field_int(&pack, "row", 1)?;
    let text = field_str(&pack, "text", "")?;
    let style = match pack_get(&pack, "style") {
        Some(v) => parse_style(&v)?,
        None => CellStyle::empty(),
    };
    Ok(DiffOp {
        kind,
        col,
        row,
        text,
        style,
    })
}

pub fn build_diff_op(
    builder: &HostValueBuilder,
    op: &DiffOp,
) -> *mut taida_addon::TaidaAddonValueV1 {
    let style = build_style(builder, &op.style);
    let names: [*const c_char; 5] = [
        FIELD_KIND.as_ptr(),
        FIELD_COL.as_ptr(),
        FIELD_ROW.as_ptr(),
        FIELD_TEXT.as_ptr(),
        FIELD_STYLE.as_ptr(),
    ];
    let values = [
        builder.int(op.kind),
        builder.int(op.col),
        builder.int(op.row),
        builder.str(&op.text),
        style,
    ];
    builder.pack(&names, &values)
}

/// Build the `BufferDiff` result pack: `@(ops <= @[...], requires_full <= Bool)`.
pub fn build_diff_result(
    builder: &HostValueBuilder,
    ops: &[DiffOp],
    requires_full: bool,
) -> *mut taida_addon::TaidaAddonValueV1 {
    let mut op_ptrs: Vec<*mut taida_addon::TaidaAddonValueV1> = Vec::with_capacity(ops.len());
    for op in ops {
        op_ptrs.push(build_diff_op(builder, op));
    }
    let ops_list = builder.list(&op_ptrs);

    let names: [*const c_char; 2] = [FIELD_OPS.as_ptr(), FIELD_REQUIRES_FULL.as_ptr()];
    let values = [ops_list, builder.bool(requires_full)];
    builder.pack(&names, &values)
}

/// Build the `RenderFrame` result pack: `@(text <= Str, next <= ScreenBuffer)`.
pub fn build_frame_result(
    builder: &HostValueBuilder,
    text: &str,
    next: &BufferState,
) -> *mut taida_addon::TaidaAddonValueV1 {
    let next_pack = build_buffer(builder, next);
    let names: [*const c_char; 2] = [FIELD_TEXT.as_ptr(), FIELD_NEXT.as_ptr()];
    let values = [builder.str(text), next_pack];
    builder.pack(&names, &values)
}

/// Parse a `DiffOp` list (the `ops` argument to `RenderOps`).
pub fn parse_diff_ops(value: &BorrowedValue<'_>) -> Result<Vec<DiffOp>, RendererError> {
    let list = value
        .as_list()
        .ok_or_else(|| RendererError::InvalidArg("ops must be a list".to_string()))?;
    let mut out = Vec::with_capacity(list.len());
    for i in 0..list.len() {
        let item = list
            .get(i)
            .ok_or_else(|| RendererError::InvalidArg(format!("ops[{i}] missing")))?;
        out.push(parse_diff_op(&item)?);
    }
    Ok(out)
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cellstyle_empty_is_default() {
        let s = CellStyle::empty();
        assert!(s.is_empty());
        assert_eq!(s.fg, "");
        assert_eq!(s.bg, "");
        assert!(!s.bold);
        assert!(!s.dim);
        assert!(!s.underline);
        assert!(!s.italic);
    }

    #[test]
    fn cellstyle_is_empty_detects_any_field_set() {
        let mut s = CellStyle::empty();
        s.bold = true;
        assert!(!s.is_empty());
        let mut s = CellStyle::empty();
        s.fg = "red".to_string();
        assert!(!s.is_empty());
        let mut s = CellStyle::empty();
        s.bg = "blue".to_string();
        assert!(!s.is_empty());
    }

    #[test]
    fn cell_default_space_is_a_single_space() {
        let c = Cell::default_space();
        assert_eq!(c.text, " ");
        assert!(c.style.is_empty());
    }

    #[test]
    fn buffer_state_cell_index_in_bounds() {
        let buf = BufferState {
            cols: 4,
            rows: 3,
            cells: vec![Cell::default_space(); 12],
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        // (1, 1) → 0
        assert_eq!(buf.cell_index(1, 1), Some(0));
        // (4, 1) → 3
        assert_eq!(buf.cell_index(4, 1), Some(3));
        // (1, 2) → 4
        assert_eq!(buf.cell_index(1, 2), Some(4));
        // (4, 3) → 11 (last cell)
        assert_eq!(buf.cell_index(4, 3), Some(11));
    }

    #[test]
    fn buffer_state_cell_index_out_of_bounds() {
        let buf = BufferState {
            cols: 4,
            rows: 3,
            cells: vec![Cell::default_space(); 12],
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        assert_eq!(buf.cell_index(0, 1), None);
        assert_eq!(buf.cell_index(1, 0), None);
        assert_eq!(buf.cell_index(5, 1), None);
        assert_eq!(buf.cell_index(1, 4), None);
        assert_eq!(buf.cell_index(-1, 1), None);
    }

    #[test]
    fn renderer_error_codes_are_frozen() {
        // Part of the cross-platform error contract — changing any of
        // these is a breaking change to the addon surface.
        assert_eq!(err::RENDERER_INVALID_ARG, 6001);
        assert_eq!(err::RENDERER_OUT_OF_BOUNDS, 6002);
        assert_eq!(err::RENDERER_INVALID_SIZE, 6003);
        assert_eq!(err::RENDERER_BUILD_VALUE, 6004);
        assert_eq!(err::RENDERER_PANIC, 6005);
    }

    #[test]
    fn renderer_error_code_mapping() {
        assert_eq!(
            RendererError::InvalidArg("x".into()).code(),
            err::RENDERER_INVALID_ARG
        );
        assert_eq!(
            RendererError::OutOfBounds("x".into()).code(),
            err::RENDERER_OUT_OF_BOUNDS
        );
        assert_eq!(
            RendererError::InvalidSize("x".into()).code(),
            err::RENDERER_INVALID_SIZE
        );
    }

    #[test]
    fn renderer_error_message_format() {
        assert_eq!(
            RendererError::InvalidArg("foo".into()).message(),
            "RendererInvalidArg: foo"
        );
        assert_eq!(
            RendererError::OutOfBounds("bar".into()).message(),
            "RendererOutOfBounds: bar"
        );
        assert_eq!(
            RendererError::InvalidSize("baz".into()).message(),
            "RendererInvalidSize: baz"
        );
    }

    #[test]
    fn row_fingerprint_default_row_is_stable() {
        // Two identical rows of default space cells must produce the
        // same fingerprint. This is the core assumption of the
        // row-hash short-circuit in `diff_buffers`.
        let row_a = vec![Cell::default_space(); 120];
        let row_b = vec![Cell::default_space(); 120];
        assert_eq!(row_fingerprint(&row_a), row_fingerprint(&row_b));
    }

    #[test]
    fn row_fingerprint_distinguishes_text() {
        let mut row_a = vec![Cell::default_space(); 4];
        let mut row_b = vec![Cell::default_space(); 4];
        row_b[2].text = "X".to_string();
        assert_ne!(row_fingerprint(&row_a), row_fingerprint(&row_b));
        // Restoring should restore equality.
        row_a[2].text = "X".to_string();
        assert_eq!(row_fingerprint(&row_a), row_fingerprint(&row_b));
    }

    #[test]
    fn row_fingerprint_distinguishes_style() {
        let row_a = vec![Cell::default_space(); 2];
        let mut row_b = vec![Cell::default_space(); 2];
        row_b[0].style.bold = true;
        assert_ne!(row_fingerprint(&row_a), row_fingerprint(&row_b));

        let mut row_c = vec![Cell::default_space(); 2];
        row_c[1].style.fg = "red".to_string();
        assert_ne!(row_fingerprint(&row_a), row_fingerprint(&row_c));
    }

    #[test]
    fn row_fingerprint_avoids_byte_bleed_between_cells() {
        // Without the per-cell terminator, `["ab", ""]` could collide
        // with `["a", "b"]` after FNV mixing. The terminator makes
        // them distinct.
        let cells_a = vec![
            Cell {
                text: "ab".to_string(),
                style: CellStyle::empty(),
            },
            Cell::default_space(),
        ];
        let cells_b = vec![
            Cell {
                text: "a".to_string(),
                style: CellStyle::empty(),
            },
            Cell {
                text: "b".to_string(),
                style: CellStyle::empty(),
            },
        ];
        assert_ne!(row_fingerprint(&cells_a), row_fingerprint(&cells_b));
    }

    #[test]
    fn row_fingerprint_avoids_field_bleed_between_text_and_style() {
        // `text="ab"`, `fg=""` vs `text="a"`, `fg="b"` — must differ.
        let cells_a = vec![Cell {
            text: "ab".to_string(),
            style: CellStyle::empty(),
        }];
        let cells_b = vec![Cell {
            text: "a".to_string(),
            style: CellStyle {
                fg: "b".to_string(),
                ..CellStyle::empty()
            },
        }];
        assert_ne!(row_fingerprint(&cells_a), row_fingerprint(&cells_b));
    }

    #[test]
    fn compute_row_hashes_populates_one_entry_per_row() {
        let mut buf = BufferState {
            cols: 3,
            rows: 4,
            cells: vec![Cell::default_space(); 12],
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        buf.compute_row_hashes();
        let hashes = buf.row_hashes.as_ref().expect("row_hashes populated");
        assert_eq!(hashes.len(), 4);
        // All rows are identical default-space rows → all hashes equal.
        let h0 = hashes[0];
        for h in hashes {
            assert_eq!(*h, h0);
        }
    }

    #[test]
    fn compute_row_hashes_is_idempotent() {
        let mut buf = BufferState {
            cols: 2,
            rows: 2,
            cells: vec![Cell::default_space(); 4],
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        buf.compute_row_hashes();
        let first = buf.row_hashes.clone();
        buf.compute_row_hashes();
        assert_eq!(buf.row_hashes, first);
    }

    #[test]
    fn compute_row_hashes_reflects_cell_change() {
        let mut buf = BufferState {
            cols: 2,
            rows: 3,
            cells: vec![Cell::default_space(); 6],
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        buf.compute_row_hashes();
        let before = buf.row_hashes.clone().expect("populated");
        // Change cell at row 2 (idx 2 or 3 → row 2 in row-major 2 cols).
        buf.cells[2].text = "X".to_string();
        buf.compute_row_hashes();
        let after = buf.row_hashes.expect("populated");
        // Row 0 and row 2 unchanged; row 1 changed.
        assert_eq!(before[0], after[0]);
        assert_ne!(before[1], after[1]);
        assert_eq!(before[2], after[2]);
    }

    #[test]
    fn compute_row_hashes_handles_zero_dim_buffer() {
        let mut buf = BufferState {
            cols: 0,
            rows: 0,
            cells: Vec::new(),
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        buf.compute_row_hashes();
        assert_eq!(buf.row_hashes, Some(Vec::new()));
    }

    #[test]
    fn fnv1a_constants_are_frozen() {
        // Locking the FNV-1a 64 spec constants prevents an accidental
        // change from invalidating cached row_hashes serialised by an
        // older version.
        assert_eq!(FNV1A_OFFSET_64, 0xcbf2_9ce4_8422_2325);
        assert_eq!(FNV1A_PRIME_64, 0x0000_0100_0000_01b3);
    }

    #[test]
    fn diff_kind_constants_are_frozen() {
        // Mirrors `taida/renderer.td::DiffOpKind`. Renumbering breaks
        // the addon ABI — the facade compares numeric tags.
        assert_eq!(diff_kind::MOVE_TO, 0);
        assert_eq!(diff_kind::WRITE, 1);
        assert_eq!(diff_kind::CLEAR_LINE, 2);
        assert_eq!(diff_kind::SHOW_CURSOR, 3);
        assert_eq!(diff_kind::HIDE_CURSOR, 4);
    }
}
