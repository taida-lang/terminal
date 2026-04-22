//! Renderer diff + render entries (Phase 8 / TMB-020).
//!
//! Owns the four addon entries that produce ANSI strings or diff
//! op lists from `ScreenBuffer` values:
//!
//! - `BufferDiff[](prev, next) → @(ops, requires_full)`
//! - `RenderFull[](buf) → Str`
//! - `RenderOps[](ops) → Str`
//! - `RenderFrame[](prev, next) → @(text, next)`
//!
//! ## ANSI sequence contract
//!
//! These functions emit the **exact same** ANSI literals the
//! Layer B facade produces (`taida/ansi.td` / `taida/style.td`):
//!
//! - `CursorMoveTo(col, row)` → `"\x1b[{row};{col}H"`
//! - `CursorHide()` → `"\x1b[?25l"`
//! - `CursorShow()` → `"\x1b[?25h"`
//! - `ClearLine()` → `"\x1b[2K\r"`
//! - `Stylize(text, style)` → `"<SGR prefix>text\x1b[0m"`
//! - `ResetStyle()` → `"\x1b[0m"`
//!
//! Duplicating the strings in Rust avoids cross-module callbacks
//! through the addon ABI (which would re-introduce the per-cell
//! allocation cost the migration is designed to eliminate). Both
//! tables are kept in sync by the `ansi_strings_match_facade`
//! regression test in the test suite.
//!
//! ## Diff strategy
//!
//! `buffer_diff` walks both buffers cell-by-cell:
//!
//! 1. If `prev.cols != next.cols` or `prev.rows != next.rows`,
//!    return `requires_full = true` so the caller falls back to
//!    `render_full`.
//! 2. Otherwise emit one `Write` op per changed cell, then trailing
//!    visibility / cursor-move ops.
//!
//! The output is **not** "minimum cost" — adjacent runs of changed
//! cells are not coalesced. That matches the pure-Taida
//! implementation in `@a.4`. Coalescing is a future optimisation
//! tracked separately so the diff op shape stays byte-stable.
//!
//! ## Cursor / visibility ops
//!
//! - `prev.cursor_visible != next.cursor_visible` → emit `ShowCursor`
//!   or `HideCursor`.
//! - `prev.cursor_col != next.cursor_col` or `prev.cursor_row !=
//!   next.cursor_row` → emit `MoveTo(next.cursor_col, next.cursor_row)`.
//!
//! These ops are appended **after** the cell diff (matching the
//! Taida-side `_diffSameSize` walker order).

use std::fmt::Write as _;

use taida_addon::bridge::{BorrowedValue, HostValueBuilder, borrow_arg};
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

use crate::renderer::state::{
    self, BufferState, Cell, CellStyle, DiffOp, RendererError, build_diff_result,
    build_frame_result, diff_kind, parse_buffer, parse_diff_ops,
};

// ── ANSI builders ─────────────────────────────────────────────────

/// `\x1b[{row};{col}H` — match `taida/ansi.td::CursorMoveTo`.
fn cursor_move_to(out: &mut String, col: i64, row: i64) {
    let _ = write!(out, "\x1b[{};{}H", row, col);
}

const ANSI_CURSOR_HIDE: &str = "\x1b[?25l";
const ANSI_CURSOR_SHOW: &str = "\x1b[?25h";
const ANSI_CLEAR_LINE: &str = "\x1b[2K\r";
const ANSI_RESET: &str = "\x1b[0m";

/// SGR code for a basic palette color name. Mirrors
/// `taida/style.td::_fgCode` / `_bgCode`. Unknown colors → no code
/// (caller skips emitting that side of the SGR sequence).
fn fg_code(name: &str) -> Option<u32> {
    match name {
        "black" => Some(30),
        "red" => Some(31),
        "green" => Some(32),
        "yellow" => Some(33),
        "blue" => Some(34),
        "magenta" => Some(35),
        "cyan" => Some(36),
        "white" => Some(37),
        "bright_black" => Some(90),
        "bright_red" => Some(91),
        "bright_green" => Some(92),
        "bright_yellow" => Some(93),
        "bright_blue" => Some(94),
        "bright_magenta" => Some(95),
        "bright_cyan" => Some(96),
        "bright_white" => Some(97),
        _ => None,
    }
}

fn bg_code(name: &str) -> Option<u32> {
    match name {
        "black" => Some(40),
        "red" => Some(41),
        "green" => Some(42),
        "yellow" => Some(43),
        "blue" => Some(44),
        "magenta" => Some(45),
        "cyan" => Some(46),
        "white" => Some(47),
        "bright_black" => Some(100),
        "bright_red" => Some(101),
        "bright_green" => Some(102),
        "bright_yellow" => Some(103),
        "bright_blue" => Some(104),
        "bright_magenta" => Some(105),
        "bright_cyan" => Some(106),
        "bright_white" => Some(107),
        _ => None,
    }
}

/// Emit a styled run as `<SGR><text>\x1b[0m`. If the style has no
/// active fields, emit `text` verbatim (no reset). Mirrors the
/// `_hasStyle` short-circuit in the pure-Taida facade.
fn write_styled(out: &mut String, text: &str, style: &CellStyle) {
    if style.is_empty() {
        out.push_str(text);
        return;
    }
    out.push_str("\x1b[");
    let mut first = true;
    let emit = |code: u32, out: &mut String, first: &mut bool| {
        if !*first {
            out.push(';');
        }
        let _ = write!(out, "{code}");
        *first = false;
    };
    if style.bold {
        emit(1, out, &mut first);
    }
    if style.dim {
        emit(2, out, &mut first);
    }
    if style.italic {
        emit(3, out, &mut first);
    }
    if style.underline {
        emit(4, out, &mut first);
    }
    if let Some(c) = fg_code(&style.fg) {
        emit(c, out, &mut first);
    }
    if let Some(c) = bg_code(&style.bg) {
        emit(c, out, &mut first);
    }
    if first {
        // No SGR codes were active despite is_empty()==false (e.g.
        // unknown fg name). Fall back to plain text.
        // Truncate the leading "\x1b[" we wrote.
        out.truncate(out.len() - 2);
        out.push_str(text);
        return;
    }
    out.push('m');
    out.push_str(text);
    out.push_str(ANSI_RESET);
}

// ── render_full ───────────────────────────────────────────────────

/// Render `buf` as a full ANSI screen update. Mirrors the pure-Taida
/// `_renderFullInner`:
///
/// 1. `CursorHide()`
/// 2. For each row r=1..rows: `CursorMoveTo(1, r)` + per-cell text.
/// 3. `CursorMoveTo(buf.cursor_col, buf.cursor_row)`
/// 4. If `buf.cursor_visible`, `CursorShow()`.
pub fn render_full(buf: &BufferState) -> String {
    if buf.cols == 0 || buf.rows == 0 {
        return String::new();
    }
    // Conservative pre-allocation: every cell ≤ ~4 bytes plus
    // per-row CursorMoveTo overhead, plus a small per-styled-cell
    // SGR worst case. Under-allocating is fine — `String` will grow.
    let approx = (buf.cols * buf.rows * 8 + buf.rows * 16 + 32) as usize;
    let mut out = String::with_capacity(approx);
    out.push_str(ANSI_CURSOR_HIDE);
    for r in 1..=buf.rows {
        cursor_move_to(&mut out, 1, r);
        for c in 1..=buf.cols {
            let idx = ((r - 1) * buf.cols + (c - 1)) as usize;
            let cell = &buf.cells[idx];
            let text: &str = if cell.text.is_empty() {
                " "
            } else {
                cell.text.as_str()
            };
            write_styled(&mut out, text, &cell.style);
        }
    }
    cursor_move_to(&mut out, buf.cursor_col, buf.cursor_row);
    if buf.cursor_visible {
        out.push_str(ANSI_CURSOR_SHOW);
    }
    out
}

// ── render_ops ────────────────────────────────────────────────────

fn render_one_op(out: &mut String, op: &DiffOp) {
    match op.kind {
        x if x == diff_kind::MOVE_TO => cursor_move_to(out, op.col, op.row),
        x if x == diff_kind::CLEAR_LINE => out.push_str(ANSI_CLEAR_LINE),
        x if x == diff_kind::SHOW_CURSOR => out.push_str(ANSI_CURSOR_SHOW),
        x if x == diff_kind::HIDE_CURSOR => out.push_str(ANSI_CURSOR_HIDE),
        x if x == diff_kind::WRITE => {
            cursor_move_to(out, op.col, op.row);
            write_styled(out, &op.text, &op.style);
        }
        _ => {}
    }
}

pub fn render_ops_to_string(ops: &[DiffOp]) -> String {
    // Worst-case heuristic: each op ≤ 32 bytes for cursor_move +
    // small text + reset.
    let mut out = String::with_capacity(ops.len() * 32);
    for op in ops {
        render_one_op(&mut out, op);
    }
    out
}

// ── buffer_diff ───────────────────────────────────────────────────

fn cells_equal(a: &Cell, b: &Cell) -> bool {
    a.text == b.text && a.style == b.style
}

/// Walk one row of a same-size buffer pair and append a `Write` op
/// per differing cell. `row_idx` is the **0-based** row index;
/// emitted ops use the 1-based `row` value.
#[inline]
fn diff_row(
    prev: &BufferState,
    next: &BufferState,
    row_idx_zero: i64,
    cols: i64,
    ops: &mut Vec<DiffOp>,
) {
    let start = (row_idx_zero * cols) as usize;
    let end = start + cols as usize;
    let prev_row = &prev.cells[start..end];
    let next_row = &next.cells[start..end];
    let row = row_idx_zero + 1;
    for c_idx in 0..cols as usize {
        if !cells_equal(&prev_row[c_idx], &next_row[c_idx]) {
            ops.push(DiffOp {
                kind: diff_kind::WRITE,
                col: c_idx as i64 + 1,
                row,
                text: next_row[c_idx].text.clone(),
                style: next_row[c_idx].style.clone(),
            });
        }
    }
}

pub fn diff_buffers(prev: &BufferState, next: &BufferState) -> (Vec<DiffOp>, bool) {
    if prev.cols != next.cols || prev.rows != next.rows {
        return (Vec::new(), true);
    }
    let cols = next.cols;
    let rows = next.rows;
    let mut ops: Vec<DiffOp> = Vec::new();

    // TMB-021: Dirty-region tracking via per-row content fingerprints.
    //
    // When `row_hashes` is populated on **both** sides (the production
    // path always populates it via `parse_buffer`), we walk hash-by-hash
    // and only descend into per-cell comparison for rows whose
    // fingerprints differ. For an unchanged 120×40 frame this collapses
    // 4800 cell compares into 40 `u64` compares (`< 5 µs` measured).
    //
    // Hand-built buffers (tests / older bench helpers) leave
    // `row_hashes = None` and fall back to the per-cell walk below —
    // the result is identical, only slower.
    match (&prev.row_hashes, &next.row_hashes) {
        (Some(prev_h), Some(next_h))
            if prev_h.len() == rows as usize && next_h.len() == rows as usize =>
        {
            for r_idx in 0..rows {
                if prev_h[r_idx as usize] != next_h[r_idx as usize] {
                    diff_row(prev, next, r_idx, cols, &mut ops);
                }
            }
        }
        _ => {
            // Slow path: no fingerprints available, walk every cell.
            // Functionally equivalent to the row-hash branch above —
            // kept so test buffers built without `compute_row_hashes`
            // still produce correct diffs.
            let total = (cols * rows) as usize;
            for idx in 0..total {
                if !cells_equal(&prev.cells[idx], &next.cells[idx]) {
                    let row = (idx as i64) / cols + 1;
                    let col = (idx as i64) % cols + 1;
                    ops.push(DiffOp {
                        kind: diff_kind::WRITE,
                        col,
                        row,
                        text: next.cells[idx].text.clone(),
                        style: next.cells[idx].style.clone(),
                    });
                }
            }
        }
    }
    // Visibility op: only emit if prev != next.
    if prev.cursor_visible != next.cursor_visible {
        let kind = if next.cursor_visible {
            diff_kind::SHOW_CURSOR
        } else {
            diff_kind::HIDE_CURSOR
        };
        ops.push(DiffOp {
            kind,
            col: 1,
            row: 1,
            text: String::new(),
            style: CellStyle::empty(),
        });
    }
    // Cursor move op: emit if either coordinate differs.
    if prev.cursor_col != next.cursor_col || prev.cursor_row != next.cursor_row {
        ops.push(DiffOp {
            kind: diff_kind::MOVE_TO,
            col: next.cursor_col,
            row: next.cursor_row,
            text: String::new(),
            style: CellStyle::empty(),
        });
    }
    (ops, false)
}

// ── FFI helpers ───────────────────────────────────────────────────

fn arg_at<'a>(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    idx: usize,
) -> Option<BorrowedValue<'a>> {
    unsafe { borrow_arg(args_ptr, args_len, idx) }
}

fn emit_error(
    builder: &HostValueBuilder,
    out_error: *mut *mut TaidaAddonErrorV1,
    err: RendererError,
) -> TaidaAddonStatus {
    let e = builder.error(err.code(), &err.message());
    if !out_error.is_null() {
        unsafe { *out_error = e };
    }
    TaidaAddonStatus::Error
}

fn emit_panic(
    builder: &HostValueBuilder,
    out_error: *mut *mut TaidaAddonErrorV1,
    what: &str,
) -> TaidaAddonStatus {
    let msg = format!("RendererPanic: {what} panicked (caught at FFI boundary)");
    let e = builder.error(state::err::RENDERER_PANIC, &msg);
    if !out_error.is_null() {
        unsafe { *out_error = e };
    }
    TaidaAddonStatus::Error
}

// ── Bench-only re-exports ────────────────────────────────────────
//
// `benches/renderer_perf.rs` measures `render_full` / `diff_buffers`
// / `render_ops_to_string` directly without paying the FFI
// marshalling cost. Production callers go through the `*_impl`
// entries below.
#[doc(hidden)]
pub mod __bench {
    pub use super::{diff_buffers, render_full, render_ops_to_string};
}

// ── Entries ───────────────────────────────────────────────────────

/// `BufferDiff[](prev, next) → @(ops, requires_full)`
pub fn buffer_diff_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 2 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let (a0, a1) = match (arg_at(args_ptr, args_len, 0), arg_at(args_ptr, args_len, 1)) {
        (Some(a), Some(b)) => (a, b),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let prev = parse_buffer(&a0)?;
        let next = parse_buffer(&a1)?;
        Ok::<(BufferState, BufferState), RendererError>((prev, next))
    }));

    match result {
        Ok(Ok((prev, next))) => {
            let (ops, requires_full) = diff_buffers(&prev, &next);
            let value = build_diff_result(&builder, &ops, requires_full);
            if value.is_null() {
                return emit_error(
                    &builder,
                    out_error,
                    RendererError::InvalidArg("BufferDiff result build failed".to_string()),
                );
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_diff"),
    }
}

/// `RenderFull[](buf) → Str`
pub fn render_full_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 1 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let a0 = match arg_at(args_ptr, args_len, 0) {
        Some(v) => v,
        None => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let buf = parse_buffer(&a0)?;
        Ok::<String, RendererError>(render_full(&buf))
    }));

    match result {
        Ok(Ok(text)) => {
            let value = builder.str(&text);
            if value.is_null() {
                return emit_error(
                    &builder,
                    out_error,
                    RendererError::InvalidArg("RenderFull str build failed".to_string()),
                );
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "render_full"),
    }
}

/// `RenderOps[](ops) → Str`
pub fn render_ops_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 1 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let a0 = match arg_at(args_ptr, args_len, 0) {
        Some(v) => v,
        None => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ops = parse_diff_ops(&a0)?;
        Ok::<String, RendererError>(render_ops_to_string(&ops))
    }));

    match result {
        Ok(Ok(text)) => {
            let value = builder.str(&text);
            if value.is_null() {
                return emit_error(
                    &builder,
                    out_error,
                    RendererError::InvalidArg("RenderOps str build failed".to_string()),
                );
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "render_ops"),
    }
}

/// `RenderFrame[](prev, next) → @(text, next)`
pub fn render_frame_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 2 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let (a0, a1) = match (arg_at(args_ptr, args_len, 0), arg_at(args_ptr, args_len, 1)) {
        (Some(a), Some(b)) => (a, b),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let prev = parse_buffer(&a0)?;
        let next = parse_buffer(&a1)?;
        Ok::<(BufferState, BufferState), RendererError>((prev, next))
    }));

    match result {
        Ok(Ok((prev, next))) => {
            let (ops, requires_full) = diff_buffers(&prev, &next);
            let text = if requires_full {
                render_full(&next)
            } else {
                render_ops_to_string(&ops)
            };
            let value = build_frame_result(&builder, &text, &next);
            if value.is_null() {
                return emit_error(
                    &builder,
                    out_error,
                    RendererError::InvalidArg("RenderFrame result build failed".to_string()),
                );
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "render_frame"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buf(cols: i64, rows: i64) -> BufferState {
        // Default helper: leave `row_hashes = None` so the diff tests
        // exercise the slow-path (per-cell walk) by default. Tests
        // that specifically want to cover the dirty-region path call
        // `compute_row_hashes()` after constructing the buffer.
        BufferState {
            cols,
            rows,
            cells: vec![Cell::default_space(); (cols * rows) as usize],
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        }
    }

    /// Like `make_buf` but with `row_hashes` precomputed — exercises
    /// the dirty-region fast-path used by the production `parse_buffer`
    /// call site.
    fn make_buf_hashed(cols: i64, rows: i64) -> BufferState {
        let mut buf = make_buf(cols, rows);
        buf.compute_row_hashes();
        buf
    }

    #[test]
    fn cursor_move_to_uses_row_then_col() {
        let mut s = String::new();
        cursor_move_to(&mut s, 5, 10);
        assert_eq!(s, "\x1b[10;5H");
    }

    #[test]
    fn ansi_constants_match_facade() {
        // Locked against `taida/ansi.td`. Changing any of these is
        // a breaking change to the renderer output.
        assert_eq!(ANSI_CURSOR_HIDE, "\x1b[?25l");
        assert_eq!(ANSI_CURSOR_SHOW, "\x1b[?25h");
        assert_eq!(ANSI_CLEAR_LINE, "\x1b[2K\r");
        assert_eq!(ANSI_RESET, "\x1b[0m");
    }

    #[test]
    fn fg_code_palette_matches_facade() {
        // SGR foreground codes 30-37 / 90-97. Locked against
        // `taida/style.td`.
        assert_eq!(fg_code("black"), Some(30));
        assert_eq!(fg_code("red"), Some(31));
        assert_eq!(fg_code("green"), Some(32));
        assert_eq!(fg_code("yellow"), Some(33));
        assert_eq!(fg_code("blue"), Some(34));
        assert_eq!(fg_code("magenta"), Some(35));
        assert_eq!(fg_code("cyan"), Some(36));
        assert_eq!(fg_code("white"), Some(37));
        assert_eq!(fg_code("bright_black"), Some(90));
        assert_eq!(fg_code("bright_white"), Some(97));
        assert_eq!(fg_code(""), None);
        assert_eq!(fg_code("orange"), None);
    }

    #[test]
    fn bg_code_palette_matches_facade() {
        assert_eq!(bg_code("black"), Some(40));
        assert_eq!(bg_code("white"), Some(47));
        assert_eq!(bg_code("bright_black"), Some(100));
        assert_eq!(bg_code("bright_white"), Some(107));
        assert_eq!(bg_code(""), None);
    }

    #[test]
    fn write_styled_plain_when_no_style() {
        let mut s = String::new();
        write_styled(&mut s, "hello", &CellStyle::empty());
        assert_eq!(s, "hello");
    }

    #[test]
    fn write_styled_emits_sgr_then_text_then_reset() {
        let mut style = CellStyle::empty();
        style.fg = "red".to_string();
        style.bold = true;
        let mut s = String::new();
        write_styled(&mut s, "x", &style);
        // Order in SGR: bold(1) then dim(2) then italic(3) then
        // underline(4) then fg then bg. So `\x1b[1;31m`.
        assert_eq!(s, "\x1b[1;31mx\x1b[0m");
    }

    #[test]
    fn write_styled_unknown_color_falls_back_to_plain() {
        let mut style = CellStyle::empty();
        style.fg = "chartreuse".to_string();
        let mut s = String::new();
        write_styled(&mut s, "x", &style);
        assert_eq!(s, "x");
    }

    #[test]
    fn render_full_empty_buffer_returns_empty() {
        let buf = BufferState {
            cols: 0,
            rows: 0,
            cells: Vec::new(),
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        assert_eq!(render_full(&buf), "");
    }

    #[test]
    fn render_full_emits_hide_rows_and_cursor() {
        let buf = make_buf(2, 1);
        let out = render_full(&buf);
        // \x1b[?25l + \x1b[1;1H + "  " + \x1b[1;1H + \x1b[?25h
        let expected = "\x1b[?25l\x1b[1;1H  \x1b[1;1H\x1b[?25h";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_full_invisible_cursor_skips_show() {
        let mut buf = make_buf(1, 1);
        buf.cursor_visible = false;
        let out = render_full(&buf);
        let expected = "\x1b[?25l\x1b[1;1H \x1b[1;1H";
        assert_eq!(out, expected);
    }

    #[test]
    fn render_one_op_move_to() {
        let mut s = String::new();
        let op = DiffOp {
            kind: diff_kind::MOVE_TO,
            col: 3,
            row: 7,
            text: String::new(),
            style: CellStyle::empty(),
        };
        render_one_op(&mut s, &op);
        assert_eq!(s, "\x1b[7;3H");
    }

    #[test]
    fn render_one_op_clear_line() {
        let mut s = String::new();
        let op = DiffOp {
            kind: diff_kind::CLEAR_LINE,
            col: 1,
            row: 1,
            text: String::new(),
            style: CellStyle::empty(),
        };
        render_one_op(&mut s, &op);
        assert_eq!(s, "\x1b[2K\r");
    }

    #[test]
    fn render_one_op_show_hide_cursor() {
        let mut s = String::new();
        let show = DiffOp {
            kind: diff_kind::SHOW_CURSOR,
            col: 1,
            row: 1,
            text: String::new(),
            style: CellStyle::empty(),
        };
        let hide = DiffOp {
            kind: diff_kind::HIDE_CURSOR,
            col: 1,
            row: 1,
            text: String::new(),
            style: CellStyle::empty(),
        };
        render_one_op(&mut s, &show);
        render_one_op(&mut s, &hide);
        assert_eq!(s, "\x1b[?25h\x1b[?25l");
    }

    #[test]
    fn render_one_op_write_emits_move_then_styled_text() {
        let mut s = String::new();
        let op = DiffOp {
            kind: diff_kind::WRITE,
            col: 5,
            row: 2,
            text: "x".to_string(),
            style: CellStyle::empty(),
        };
        render_one_op(&mut s, &op);
        assert_eq!(s, "\x1b[2;5Hx");
    }

    #[test]
    fn diff_identical_buffers_returns_no_ops() {
        let prev = make_buf(3, 2);
        let next = make_buf(3, 2);
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert!(ops.is_empty());
    }

    #[test]
    fn diff_size_change_requires_full() {
        let prev = make_buf(3, 2);
        let next = make_buf(4, 2);
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(requires_full);
        assert!(ops.is_empty());
    }

    #[test]
    fn diff_one_changed_cell_emits_one_write_op() {
        let prev = make_buf(3, 2);
        let mut next = make_buf(3, 2);
        next.cells[2] = Cell {
            text: "X".to_string(),
            style: CellStyle::empty(),
        };
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert_eq!(ops.len(), 1);
        let op = &ops[0];
        assert_eq!(op.kind, diff_kind::WRITE);
        // index 2 in 3×2 row-major → col 3, row 1
        assert_eq!(op.col, 3);
        assert_eq!(op.row, 1);
        assert_eq!(op.text, "X");
    }

    #[test]
    fn diff_visibility_change_emits_visibility_op() {
        let prev = make_buf(2, 1);
        let mut next = make_buf(2, 1);
        next.cursor_visible = false;
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, diff_kind::HIDE_CURSOR);
    }

    #[test]
    fn diff_cursor_move_emits_move_op() {
        let prev = make_buf(3, 3);
        let mut next = make_buf(3, 3);
        next.cursor_col = 2;
        next.cursor_row = 3;
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, diff_kind::MOVE_TO);
        assert_eq!(ops[0].col, 2);
        assert_eq!(ops[0].row, 3);
    }

    #[test]
    fn render_ops_concatenates_in_order() {
        let ops = vec![
            DiffOp {
                kind: diff_kind::WRITE,
                col: 1,
                row: 1,
                text: "a".to_string(),
                style: CellStyle::empty(),
            },
            DiffOp {
                kind: diff_kind::MOVE_TO,
                col: 2,
                row: 2,
                text: String::new(),
                style: CellStyle::empty(),
            },
        ];
        let out = render_ops_to_string(&ops);
        assert_eq!(out, "\x1b[1;1Ha\x1b[2;2H");
    }

    // ── TMB-021: dirty-region fast-path regression coverage ──────

    #[test]
    fn diff_identical_buffers_with_row_hashes_returns_no_ops() {
        // Same as `diff_identical_buffers_returns_no_ops` but exercises
        // the row-hash fast-path. Both buffers are identical default
        // grids → row hashes match for every row → no per-cell walk.
        let prev = make_buf_hashed(8, 4);
        let next = make_buf_hashed(8, 4);
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert!(ops.is_empty());
    }

    #[test]
    fn diff_one_changed_cell_with_row_hashes_emits_one_write_op() {
        // Modifying cell (col=4, row=2) in an 8×3 buffer must produce
        // exactly one Write op, even on the dirty-region path. The
        // mutated buffer's `row_hashes[1]` differs from baseline; the
        // fast-path descends into row 1 and finds the single diff.
        let prev = make_buf_hashed(8, 3);
        let mut next = make_buf(8, 3);
        next.cells[8 + 3] = Cell {
            text: "X".to_string(),
            style: CellStyle::empty(),
        };
        next.compute_row_hashes();
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].kind, diff_kind::WRITE);
        // index 11 in 8×3 row-major → col 4, row 2
        assert_eq!(ops[0].col, 4);
        assert_eq!(ops[0].row, 2);
        assert_eq!(ops[0].text, "X");
    }

    #[test]
    fn diff_two_diff_cells_in_different_rows_with_row_hashes() {
        // Verifies multiple-row dirty-region descent: changes in rows
        // 0 and 2 of a 4-row buffer → two ops, in row-major order.
        let prev = make_buf_hashed(3, 4);
        let mut next = make_buf(3, 4);
        next.cells[1] = Cell {
            text: "A".to_string(),
            style: CellStyle::empty(),
        }; // row 0, col 1 → 1-based (col=2, row=1)
        next.cells[7] = Cell {
            text: "B".to_string(),
            style: CellStyle::empty(),
        }; // row 2, col 1 → 1-based (col=2, row=3)
        next.compute_row_hashes();
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert_eq!(ops.len(), 2);
        assert_eq!((ops[0].col, ops[0].row, ops[0].text.as_str()), (2, 1, "A"));
        assert_eq!((ops[1].col, ops[1].row, ops[1].text.as_str()), (2, 3, "B"));
    }

    #[test]
    fn diff_fast_path_and_slow_path_agree_on_random_change() {
        // Build two non-trivial buffers, run the diff with and without
        // row_hashes, and confirm both paths produce the same op list.
        // Catches accidental divergence between the two branches.
        let mut a = make_buf(5, 4);
        let mut b = make_buf(5, 4);
        // Sprinkle a few changes across multiple rows.
        b.cells[0].text = "1".to_string();
        b.cells[6].style.bold = true;
        b.cells[14].style.fg = "red".to_string();
        b.cells[19].text = "Z".to_string();

        // Slow path: leave row_hashes None on both.
        let (slow_ops, slow_full) = diff_buffers(&a, &b);

        // Fast path: precompute row_hashes on both.
        a.compute_row_hashes();
        b.compute_row_hashes();
        let (fast_ops, fast_full) = diff_buffers(&a, &b);

        assert_eq!(slow_full, fast_full);
        assert_eq!(slow_ops.len(), fast_ops.len(), "op count differs");
        for (s, f) in slow_ops.iter().zip(fast_ops.iter()) {
            assert_eq!(s.kind, f.kind);
            assert_eq!(s.col, f.col);
            assert_eq!(s.row, f.row);
            assert_eq!(s.text, f.text);
            assert_eq!(s.style, f.style);
        }
    }

    #[test]
    fn diff_fast_path_handles_only_one_side_having_row_hashes() {
        // If `prev` has row_hashes but `next` does not (or vice versa),
        // the differ must fall back to the slow-path and still return
        // the correct ops. Guards against future refactors that make
        // the fast-path depend on a single side.
        let mut prev = make_buf(4, 2);
        let mut next = make_buf(4, 2);
        next.cells[5] = Cell {
            text: "X".to_string(),
            style: CellStyle::empty(),
        };
        prev.compute_row_hashes();
        // next.row_hashes intentionally left as None
        let (ops, requires_full) = diff_buffers(&prev, &next);
        assert!(!requires_full);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].text, "X");
    }
}
