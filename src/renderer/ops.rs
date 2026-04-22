//! Renderer mutation operations (Phase 8 / TMB-020).
//!
//! This module owns the addon entries for `BufferPut`, `BufferWrite`,
//! `BufferFillRect`, and `BufferClear`. Each entry takes a buffer
//! pack (and optional position / cell / text / style arguments),
//! mutates a freshly-cloned `BufferState`, and returns a fresh
//! `ScreenBuffer` pack to the host.
//!
//! ## Width policy
//!
//! `buffer_write_impl` mirrors the pure-Taida `_bwWorker` from
//! `taida/renderer.td` so the visual output is byte-identical to the
//! `@a.4` facade. The policy (frozen in Phase 4):
//!
//! 1. ASCII printable → width 1.
//! 2. Combining mark → width 0 (skipped).
//! 3. East Asian Wide / Fullwidth → width 2 (occupies two cells; the
//!    second cell receives a styled space placeholder).
//! 4. Truncate at right edge — never wrap to the next row.
//!
//! The codepoint ranges are duplicated from `taida/width.td` because
//! cross-module callbacks across the FFI boundary would re-introduce
//! the per-cell allocation cost the migration is meant to eliminate.
//! Both width tables are kept in sync by the
//! `width_tables_match_facade` regression test in `tests/`.

use taida_addon::bridge::{BorrowedValue, HostValueBuilder, borrow_arg};
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

use crate::renderer::state::{
    self, BufferState, Cell, CellStyle, RendererError, build_buffer, parse_buffer, parse_cell,
    parse_style,
};

/// Helper: borrow argument `idx` or return `NullPointer`-style error
/// status. Returns the borrowed value to the caller for parsing.
fn arg_at<'a>(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    idx: usize,
) -> Option<BorrowedValue<'a>> {
    unsafe { borrow_arg(args_ptr, args_len, idx) }
}

// ── Width policy ──────────────────────────────────────────────────

/// Width category for one Unicode scalar value. Mirrors
/// `taida/width.td::MeasureGrapheme` — see module docs for why the
/// table is duplicated.
fn char_width(cp: u32) -> u32 {
    if is_control(cp) {
        return 0;
    }
    if is_combining(cp) {
        return 0;
    }
    if is_wide(cp) {
        return 2;
    }
    1
}

fn is_control(cp: u32) -> bool {
    cp < 0x20 || cp == 0x7F || (0x80..=0x9F).contains(&cp)
}

fn is_combining(cp: u32) -> bool {
    matches!(
        cp,
        0x0300..=0x036F
        | 0x1100..=0x117F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20FF
        | 0xFE20..=0xFE2F
        | 0xFE00..=0xFE0F
        | 0x200B
        | 0x200C
        | 0x200D
        | 0xFEFF
        | 0x309A..=0x309B
        | 0xE0100..=0xE01EF
    )
}

fn is_wide(cp: u32) -> bool {
    matches!(
        cp,
        0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF01..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1100..=0x115F
        | 0x20000..=0x3FFFD
    )
}

// ── Mutation primitives ───────────────────────────────────────────

/// Put a single cell at `(col, row)` after bounds checking. Mutates
/// `buf` in place.
fn put_cell(buf: &mut BufferState, col: i64, row: i64, cell: Cell) -> Result<(), RendererError> {
    let idx = buf
        .cell_index(col, row)
        .ok_or_else(|| RendererError::OutOfBounds(format!("col {col} row {row} out of bounds")))?;
    buf.cells[idx] = cell;
    Ok(())
}

/// Put a cell **without** bounds checking. Used internally by
/// `buffer_write_impl` after the worker confirms the cell is in range.
fn put_cell_unchecked(buf: &mut BufferState, col: i64, row: i64, cell: Cell) {
    let idx = ((row - 1) * buf.cols + (col - 1)) as usize;
    buf.cells[idx] = cell;
}

/// Write text starting at `(col, row)`. Truncates at right edge.
/// Width-2 graphemes occupy two cells; width-0 graphemes are skipped.
///
/// This is the hot path that drove TMB-020 — pure-Taida did
/// per-character `Take`/`Drop`/`Append`/`Concat` (each `O(N)`),
/// degrading to `O(W·N)` for width `W` text on an `N` cell buffer.
/// Here every cell write is `O(1)` against the in-place `Vec<Cell>`.
pub fn write_text(buf: &mut BufferState, mut col: i64, row: i64, text: &str, style: &CellStyle) {
    if col < 1 || row < 1 || col > buf.cols || row > buf.rows {
        // Caller already validated; we still defend so a future
        // refactor cannot accidentally overwrite arbitrary memory.
        return;
    }
    for ch in text.chars() {
        let cp = ch as u32;
        let w = char_width(cp);
        if w == 0 {
            continue;
        }
        if col + (w as i64) - 1 > buf.cols {
            // Right-edge truncation: would cross the boundary. Stop.
            break;
        }
        let mut buf_str = [0u8; 4];
        let ch_str = ch.encode_utf8(&mut buf_str);
        let cell = Cell {
            text: ch_str.to_string(),
            style: style.clone(),
        };
        put_cell_unchecked(buf, col, row, cell);
        if w == 2 {
            // Wide-char placeholder: the second cell holds " " with
            // the same style so cursor math stays consistent.
            let placeholder = Cell {
                text: " ".to_string(),
                style: style.clone(),
            };
            put_cell_unchecked(buf, col + 1, row, placeholder);
        }
        col += w as i64;
        if col > buf.cols {
            break;
        }
    }
}

fn fill_rect(buf: &mut BufferState, col0: i64, row0: i64, width: i64, height: i64, cell: &Cell) {
    if width < 1 || height < 1 {
        return;
    }
    if col0 > buf.cols || row0 > buf.rows {
        return;
    }
    let end_col = (col0 + width).min(buf.cols + 1);
    let end_row = (row0 + height).min(buf.rows + 1);
    for r in row0..end_row {
        for c in col0..end_col {
            put_cell_unchecked(buf, c, r, cell.clone());
        }
    }
}

fn clear_buffer(buf: &mut BufferState, fill: &Cell) {
    for cell in &mut buf.cells {
        *cell = fill.clone();
    }
}

// ── FFI helpers ───────────────────────────────────────────────────

/// Emit a renderer error and return `Error` status. Centralises the
/// `out_error` write so every entry has identical failure shape.
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

fn emit_build_failure(
    builder: &HostValueBuilder,
    out_error: *mut *mut TaidaAddonErrorV1,
    what: &str,
) -> TaidaAddonStatus {
    let msg = format!("RendererBuildValue: failed to build {what}");
    let e = builder.error(state::err::RENDERER_BUILD_VALUE, &msg);
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
// `benches/renderer_perf.rs` measures the hot path **after**
// marshalling, so it needs to call the in-Rust mutation primitives
// directly without going through `parse_buffer` / `build_buffer`.
// Production callers go through the `*_impl` entries above.
#[doc(hidden)]
pub mod __bench {
    pub use super::write_text;
}

// ── Entries ───────────────────────────────────────────────────────

/// `BufferPut[](buf, col, row, cell) → ScreenBuffer`
pub fn buffer_put_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 4 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let (a0, a1, a2, a3) = match (
        arg_at(args_ptr, args_len, 0),
        arg_at(args_ptr, args_len, 1),
        arg_at(args_ptr, args_len, 2),
        arg_at(args_ptr, args_len, 3),
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buf = parse_buffer(&a0)?;
        let col = a1
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("col must be Int".to_string()))?;
        let row = a2
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("row must be Int".to_string()))?;
        let cell = parse_cell(&a3)?;
        put_cell(&mut buf, col, row, cell)?;
        // TMB-021: row_hashes from parse_buffer are now stale; the
        // pack we hand back will be re-parsed (and re-hashed) on the
        // next addon entry call, but we invalidate eagerly so any
        // future in-process consumer of the mutated `BufferState`
        // observes `None` rather than a wrong fingerprint.
        buf.row_hashes = None;
        Ok::<BufferState, RendererError>(buf)
    }));

    match result {
        Ok(Ok(buf)) => {
            let value = build_buffer(&builder, &buf);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "BufferPut result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_put"),
    }
}

/// `BufferWrite[](buf, col, row, text, style) → ScreenBuffer`
pub fn buffer_write_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 5 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let (a0, a1, a2, a3, a4) = match (
        arg_at(args_ptr, args_len, 0),
        arg_at(args_ptr, args_len, 1),
        arg_at(args_ptr, args_len, 2),
        arg_at(args_ptr, args_len, 3),
        arg_at(args_ptr, args_len, 4),
    ) {
        (Some(a), Some(b), Some(c), Some(d), Some(e)) => (a, b, c, d, e),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buf = parse_buffer(&a0)?;
        let col = a1
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("col must be Int".to_string()))?;
        let row = a2
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("row must be Int".to_string()))?;
        let text = a3
            .as_str()
            .ok_or_else(|| RendererError::InvalidArg("text must be Str".to_string()))?
            .to_string();
        let style = parse_style(&a4)?;
        // Bounds check the starting position only — the worker handles
        // right-edge truncation as part of the width policy.
        if buf.cell_index(col, row).is_none() {
            return Err(RendererError::OutOfBounds(format!(
                "col {col} row {row} out of bounds"
            )));
        }
        write_text(&mut buf, col, row, &text, &style);
        // TMB-021: invalidate stale row_hashes from parse_buffer.
        buf.row_hashes = None;
        Ok::<BufferState, RendererError>(buf)
    }));

    match result {
        Ok(Ok(buf)) => {
            let value = build_buffer(&builder, &buf);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "BufferWrite result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_write"),
    }
}

/// `BufferFillRect[](buf, col, row, width, height, cell) → ScreenBuffer`
pub fn buffer_fill_rect_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 6 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let (a0, a1, a2, a3, a4, a5) = match (
        arg_at(args_ptr, args_len, 0),
        arg_at(args_ptr, args_len, 1),
        arg_at(args_ptr, args_len, 2),
        arg_at(args_ptr, args_len, 3),
        arg_at(args_ptr, args_len, 4),
        arg_at(args_ptr, args_len, 5),
    ) {
        (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f)) => (a, b, c, d, e, f),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buf = parse_buffer(&a0)?;
        let col = a1
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("col must be Int".to_string()))?;
        let row = a2
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("row must be Int".to_string()))?;
        let width = a3
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("width must be Int".to_string()))?;
        let height = a4
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("height must be Int".to_string()))?;
        let cell = parse_cell(&a5)?;
        // Match facade contract: col<1 / row<1 → OutOfBounds.
        // width<1 / height<1 → no-op (same buffer back).
        // col/row past edge → no-op.
        if col < 1 {
            return Err(RendererError::OutOfBounds("col out of bounds".to_string()));
        }
        if row < 1 {
            return Err(RendererError::OutOfBounds("row out of bounds".to_string()));
        }
        fill_rect(&mut buf, col, row, width, height, &cell);
        // TMB-021: invalidate stale row_hashes from parse_buffer.
        buf.row_hashes = None;
        Ok::<BufferState, RendererError>(buf)
    }));

    match result {
        Ok(Ok(buf)) => {
            let value = build_buffer(&builder, &buf);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "BufferFillRect result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_fill_rect"),
    }
}

/// `BufferClear[](buf, fill) → ScreenBuffer`
pub fn buffer_clear_impl(
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
        let mut buf = parse_buffer(&a0)?;
        let fill = parse_cell(&a1)?;
        clear_buffer(&mut buf, &fill);
        // TMB-021: invalidate stale row_hashes from parse_buffer.
        buf.row_hashes = None;
        Ok::<BufferState, RendererError>(buf)
    }));

    match result {
        Ok(Ok(buf)) => {
            let value = build_buffer(&builder, &buf);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "BufferClear result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_clear"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buf(cols: i64, rows: i64) -> BufferState {
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

    #[test]
    fn ascii_chars_are_width_1() {
        for cp in 0x20u32..=0x7E {
            assert_eq!(char_width(cp), 1, "U+{cp:04X} should be width 1");
        }
    }

    #[test]
    fn cjk_ideograph_is_width_2() {
        assert_eq!(char_width(0x4E00), 2); // 一
        assert_eq!(char_width(0x6F22), 2); // 漢
        assert_eq!(char_width(0xAC00), 2); // 가
    }

    #[test]
    fn fullwidth_latin_is_width_2() {
        assert_eq!(char_width(0xFF21), 2); // FULLWIDTH LATIN CAPITAL A
    }

    #[test]
    fn combining_marks_are_width_0() {
        assert_eq!(char_width(0x0300), 0);
        assert_eq!(char_width(0x036F), 0);
        assert_eq!(char_width(0xFE0F), 0); // variation selector
        assert_eq!(char_width(0x200D), 0); // ZWJ
    }

    #[test]
    fn control_chars_are_width_0() {
        assert_eq!(char_width(0x00), 0);
        assert_eq!(char_width(0x09), 0); // TAB
        assert_eq!(char_width(0x1B), 0); // ESC
        assert_eq!(char_width(0x7F), 0); // DEL
        assert_eq!(char_width(0x80), 0);
    }

    #[test]
    fn put_cell_unchecked_indexes_row_major() {
        let mut buf = make_buf(4, 3);
        let red = Cell {
            text: "X".to_string(),
            style: CellStyle::empty(),
        };
        put_cell_unchecked(&mut buf, 1, 1, red.clone());
        assert_eq!(buf.cells[0].text, "X");
        put_cell_unchecked(&mut buf, 4, 3, red.clone());
        assert_eq!(buf.cells[11].text, "X");
        put_cell_unchecked(&mut buf, 2, 2, red.clone());
        // Index formula: (row - 1) * cols + (col - 1) for (col=2, row=2, cols=4) = 5.
        assert_eq!(buf.cells[5].text, "X");
    }

    #[test]
    fn put_cell_bounds_checked() {
        let mut buf = make_buf(4, 3);
        let cell = Cell::default_space();
        assert!(put_cell(&mut buf, 0, 1, cell.clone()).is_err());
        assert!(put_cell(&mut buf, 1, 0, cell.clone()).is_err());
        assert!(put_cell(&mut buf, 5, 1, cell.clone()).is_err());
        assert!(put_cell(&mut buf, 1, 4, cell.clone()).is_err());
        assert!(put_cell(&mut buf, 1, 1, cell.clone()).is_ok());
    }

    #[test]
    fn write_text_truncates_at_right_edge() {
        let mut buf = make_buf(5, 1);
        let style = CellStyle::empty();
        write_text(&mut buf, 1, 1, "abcdefghij", &style);
        let row: String = buf.cells.iter().map(|c| c.text.clone()).collect();
        assert_eq!(row, "abcde");
    }

    #[test]
    fn write_text_wide_char_takes_two_cells_with_placeholder() {
        let mut buf = make_buf(4, 1);
        let style = CellStyle::empty();
        write_text(&mut buf, 1, 1, "漢字", &style);
        // "漢" at col 1 (width 2) → cells[0] = "漢", cells[1] = " "
        // "字" at col 3 (width 2) → cells[2] = "字", cells[3] = " "
        assert_eq!(buf.cells[0].text, "漢");
        assert_eq!(buf.cells[1].text, " ");
        assert_eq!(buf.cells[2].text, "字");
        assert_eq!(buf.cells[3].text, " ");
    }

    #[test]
    fn write_text_wide_char_truncates_when_only_one_cell_remains() {
        let mut buf = make_buf(3, 1);
        let style = CellStyle::empty();
        write_text(&mut buf, 1, 1, "漢字", &style);
        // "漢" fits at col 1-2, "字" needs cols 3-4 but row only has 3.
        assert_eq!(buf.cells[0].text, "漢");
        assert_eq!(buf.cells[1].text, " ");
        assert_eq!(buf.cells[2].text, " "); // unchanged default
    }

    #[test]
    fn write_text_combining_marks_are_skipped() {
        let mut buf = make_buf(5, 1);
        let style = CellStyle::empty();
        // "a\u{0300}b" — combining grave between a and b.
        write_text(&mut buf, 1, 1, "a\u{0300}b", &style);
        assert_eq!(buf.cells[0].text, "a");
        assert_eq!(buf.cells[1].text, "b");
        assert_eq!(buf.cells[2].text, " ");
    }

    #[test]
    fn write_text_propagates_style_to_placeholder() {
        let mut buf = make_buf(2, 1);
        let mut style = CellStyle::empty();
        style.fg = "red".to_string();
        write_text(&mut buf, 1, 1, "漢", &style);
        assert_eq!(buf.cells[0].text, "漢");
        assert_eq!(buf.cells[0].style.fg, "red");
        assert_eq!(buf.cells[1].text, " ");
        assert_eq!(buf.cells[1].style.fg, "red");
    }

    #[test]
    fn fill_rect_clamps_to_buffer_edges() {
        let mut buf = make_buf(5, 5);
        let cell = Cell {
            text: "#".to_string(),
            style: CellStyle::empty(),
        };
        fill_rect(&mut buf, 3, 3, 10, 10, &cell);
        // Should fill the bottom-right 3×3 block (cols 3-5, rows 3-5).
        for r in 1..=5i64 {
            for c in 1..=5i64 {
                let idx = ((r - 1) * 5 + (c - 1)) as usize;
                if c >= 3 && r >= 3 {
                    assert_eq!(buf.cells[idx].text, "#", "expected # at ({c},{r})");
                } else {
                    assert_eq!(buf.cells[idx].text, " ", "expected space at ({c},{r})");
                }
            }
        }
    }

    #[test]
    fn fill_rect_zero_size_is_noop() {
        let mut buf = make_buf(3, 3);
        let cell = Cell {
            text: "#".to_string(),
            style: CellStyle::empty(),
        };
        fill_rect(&mut buf, 1, 1, 0, 0, &cell);
        for c in &buf.cells {
            assert_eq!(c.text, " ");
        }
        fill_rect(&mut buf, 1, 1, -1, 1, &cell);
        for c in &buf.cells {
            assert_eq!(c.text, " ");
        }
    }

    #[test]
    fn clear_buffer_overwrites_every_cell() {
        let mut buf = make_buf(3, 2);
        let style = CellStyle::empty();
        write_text(&mut buf, 1, 1, "abc", &style);
        let fill = Cell {
            text: ".".to_string(),
            style: CellStyle::empty(),
        };
        clear_buffer(&mut buf, &fill);
        for c in &buf.cells {
            assert_eq!(c.text, ".");
        }
    }
}
