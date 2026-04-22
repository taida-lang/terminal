//! Renderer blit operation (Phase 9 / TMB-022).
//!
//! This module owns the addon entry for `BufferBlit`, the
//! sub-buffer → main-buffer composition primitive.
//!
//! ## Why this exists (TMB-022)
//!
//! Phase 8 / TMB-020 moved per-cell put / write / fill_rect / diff /
//! render to Rust, so each cell-level mutation is `O(1)`. However the
//! **compose** step that lays a sub-buffer into a main buffer — a core
//! operation for any pane-based TUI — was left in Taida. Hachikuma
//! implemented it as a loop of `BufferWrite` calls per row, which
//! hits Taida's `O(n)` list indexing and collapses to `O(N²)` over
//! all cells. `perf_real_smoke.td` measured 120×40 at 48075 ms in
//! @a.5, 471× over the R-2 budget of 50 ms.
//!
//! `BufferBlit` moves the compose loop into Rust where `main.cells`
//! and `sub.cells` are `Vec<Cell>` with `O(1)` random access. The
//! entire copy is a pair of nested row / column loops over pre-sized
//! storage with no per-cell allocation beyond the `Cell::clone()`
//! required by the structural copy.
//!
//! ## Contract
//!
//! ```text
//! BufferBlit[](main: ScreenBuffer, sub: ScreenBuffer, col: Int, row: Int)
//!   -> ScreenBuffer
//! ```
//!
//! - `(col, row)` is a 1-based absolute coordinate in `main`. The
//!   sub-buffer's `(1, 1)` cell lands at `main[col, row]`.
//! - When the sub-buffer extends past `main`'s right or bottom edge,
//!   the overflowing cells are **silently clipped**. This mirrors
//!   `BufferFillRect`'s clamp behaviour and gives callers a consistent
//!   "lay this in if it fits, otherwise as much as fits" semantic.
//! - When `(col, row)` points past `main`'s bounds (e.g. a sub-buffer
//!   that starts off-screen to the right), the copy is a no-op — the
//!   main buffer is returned unchanged.
//! - `col < 1` or `row < 1` → `RendererOutOfBounds`. This matches the
//!   facade contract of `BufferFillRect` which also rejects non-positive
//!   starting coordinates outright. A caller that legitimately needs
//!   to partially scroll a pane off the top/left should do the
//!   coordinate arithmetic themselves.
//! - Wide-char placeholder cells in `sub` are carried through verbatim:
//!   because the cells list is a pre-rendered `Vec<Cell>` and the
//!   placeholder `" "` cell is already its own entry (written by
//!   `buffer_write_impl`), a row-by-row copy preserves the visual
//!   pairing automatically.
//! - Right-edge wide-char drop (mirrors `ops::write_text` line ~138):
//!   when clipping would land a wide-char **lead** cell at
//!   `main.cols` with its placeholder spilling past the edge, the
//!   lead cell is not copied either. This keeps the same
//!   "don't write a half wide char" contract `BufferWrite` uses and
//!   prevents `ScreenBuffer`'s invariant of paired wide cells from
//!   breaking after a blit. Downstream `BufferDiff` / `RenderFull`
//!   assume every wide lead has a placeholder — we must not hand them
//!   an inconsistent buffer.
//! - Style attributes (`fg`, `bg`, `bold`, `dim`, `underline`,
//!   `italic`) are copied per-cell because they are part of `Cell`.
//!
//! ## Performance
//!
//! For a 120×40 sub copied into a 120×40 main the expected cost is
//! `O(N)` where `N = 4800` cells, with one `Cell::clone()` per cell
//! (text `String` + style). On a typical developer machine this
//! measures well under 200 µs; see `benches/renderer_perf.rs`.

use taida_addon::bridge::{BorrowedValue, HostValueBuilder, borrow_arg};
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

use crate::renderer::ops::cell_is_wide_lead;
use crate::renderer::state::{self, BufferState, RendererError, build_buffer, parse_buffer};

/// Helper: borrow argument `idx` or return `None`.
fn arg_at<'a>(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    idx: usize,
) -> Option<BorrowedValue<'a>> {
    unsafe { borrow_arg(args_ptr, args_len, idx) }
}

// ── Core blit primitive ───────────────────────────────────────────

/// Copy every in-bounds cell of `sub` into `main` starting at
/// `(col, row)` (1-based). Overflowing cells are silently clipped.
///
/// This is the hot path. `main` is mutated in place so `BufferBlit`
/// pays the `O(N)` buffer clone only once (inside `parse_buffer`) —
/// the blit itself is a straight linear memcpy-equivalent over
/// `main.cells`.
pub fn blit_into(main: &mut BufferState, sub: &BufferState, col: i64, row: i64) {
    // Out-of-bounds start past the right / bottom edge → no-op. A
    // negative start is caller error and is rejected at the entry
    // boundary; here we defend against a future change to that
    // contract by clamping to the main buffer's bounds.
    if sub.cols < 1 || sub.rows < 1 {
        return;
    }
    if col > main.cols || row > main.rows {
        return;
    }
    // Compute the in-main bounding box of the blit. We copy
    // `sub.cells[sr, sc]` → `main.cells[row + sr - 1, col + sc - 1]`
    // for `sr ∈ [1, min(sub.rows, main.rows - row + 1)]` and
    // `sc ∈ [1, min(sub.cols, main.cols - col + 1)]`.
    let height = {
        let avail = main.rows - row + 1;
        sub.rows.min(avail)
    };
    let width = {
        let avail = main.cols - col + 1;
        sub.cols.min(avail)
    };
    if width < 1 || height < 1 {
        return;
    }

    // Row-major copy. `main.cells[(row - 1 + dr) * main.cols + (col - 1 + dc)]`
    // receives `sub.cells[dr * sub.cols + dc]` for each
    // `(dr, dc) ∈ [0, height) × [0, width)`.
    //
    // Right-edge wide-char drop: if the clip chopped the sub down
    // (`width < sub.cols`) *and* the last cell we'd copy is a
    // wide-char lead, shrink the per-row copy by one so the lead's
    // missing placeholder cannot break `ScreenBuffer`'s wide-char
    // pairing invariant downstream. This mirrors the policy
    // `ops::write_text` uses at the same spot (line ~138): "would
    // cross the boundary → stop".
    let main_cols = main.cols as usize;
    let sub_cols = sub.cols as usize;
    let height_u = height as usize;
    let main_base_row = (row - 1) as usize;
    let main_base_col = (col - 1) as usize;
    let clipped = (width as usize) < sub_cols;

    for dr in 0..height_u {
        let sub_row_start = dr * sub_cols;
        let mut copy_width = width as usize;
        if clipped && copy_width > 0 {
            // `width` is the max we could copy; check the last sub cell
            // in that range for a wide-char lead and drop if so.
            let last_sub_idx = sub_row_start + copy_width - 1;
            if cell_is_wide_lead(&sub.cells[last_sub_idx].text) {
                copy_width -= 1;
            }
        }
        if copy_width == 0 {
            continue;
        }
        let main_row_start = (main_base_row + dr) * main_cols + main_base_col;
        // Clone sub-slice into main-slice. `clone_from_slice` requires
        // equal-length slices; `copy_width` was constructed to satisfy
        // that.
        let main_slice = &mut main.cells[main_row_start..main_row_start + copy_width];
        let sub_slice = &sub.cells[sub_row_start..sub_row_start + copy_width];
        for (m, s) in main_slice.iter_mut().zip(sub_slice.iter()) {
            *m = s.clone();
        }
    }
}

// ── FFI helpers (mirror `ops.rs` so error shape is identical) ─────

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

// ── Bench-only re-exports ─────────────────────────────────────────
//
// `benches/renderer_perf.rs` measures the blit hot path **after**
// marshalling, so it needs direct access to `blit_into`.
#[doc(hidden)]
pub mod __bench {
    pub use super::blit_into;
}

// ── Entry ─────────────────────────────────────────────────────────

/// `BufferBlit[](main, sub, col, row) → ScreenBuffer`
pub fn buffer_blit_impl(
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
        let mut main = parse_buffer(&a0)?;
        let sub = parse_buffer(&a1)?;
        let col = a2
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("col must be Int".to_string()))?;
        let row = a3
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("row must be Int".to_string()))?;
        // Contract: negative / zero start is caller error, not a
        // clamp. See module docs for rationale.
        if col < 1 {
            return Err(RendererError::OutOfBounds(format!(
                "col {col} must be >= 1"
            )));
        }
        if row < 1 {
            return Err(RendererError::OutOfBounds(format!(
                "row {row} must be >= 1"
            )));
        }
        blit_into(&mut main, &sub, col, row);
        // TMB-021: any row_hashes parsed by `parse_buffer` above are
        // now stale because we just mutated `main.cells`. Invalidate
        // eagerly so a future in-process consumer sees `None` instead
        // of a wrong fingerprint. The next `parse_buffer` on the
        // returned pack will recompute them.
        main.row_hashes = None;
        Ok::<BufferState, RendererError>(main)
    }));

    match result {
        Ok(Ok(buf)) => {
            let value = build_buffer(&builder, &buf);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "BufferBlit result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_blit"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::state::{Cell, CellStyle};

    fn make_buf(cols: i64, rows: i64, fill: &str) -> BufferState {
        let cell = Cell {
            text: fill.to_string(),
            style: CellStyle::empty(),
        };
        BufferState {
            cols,
            rows,
            cells: vec![cell; (cols * rows) as usize],
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        }
    }

    fn row_texts(buf: &BufferState) -> Vec<String> {
        let cols = buf.cols as usize;
        let rows = buf.rows as usize;
        (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| buf.cells[r * cols + c].text.clone())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn blit_identity_same_size() {
        // sub == main size, pasted at (1,1) → every cell is overwritten.
        let mut main = make_buf(4, 3, ".");
        let sub = make_buf(4, 3, "#");
        blit_into(&mut main, &sub, 1, 1);
        for cell in &main.cells {
            assert_eq!(cell.text, "#");
        }
    }

    #[test]
    fn blit_partial_in_middle() {
        // 2×2 sub placed at (3,2) in a 5×4 main.
        let mut main = make_buf(5, 4, ".");
        let sub = make_buf(2, 2, "#");
        blit_into(&mut main, &sub, 3, 2);
        let rows = row_texts(&main);
        assert_eq!(rows[0], ".....");
        assert_eq!(rows[1], "..##.");
        assert_eq!(rows[2], "..##.");
        assert_eq!(rows[3], ".....");
    }

    #[test]
    fn blit_clips_at_right_edge() {
        // 5×5 sub pasted at (3,3) in a 4×4 main → only 2×2 copied.
        let mut main = make_buf(4, 4, ".");
        let sub = make_buf(5, 5, "#");
        blit_into(&mut main, &sub, 3, 3);
        let rows = row_texts(&main);
        assert_eq!(rows[0], "....");
        assert_eq!(rows[1], "....");
        assert_eq!(rows[2], "..##");
        assert_eq!(rows[3], "..##");
    }

    #[test]
    fn blit_clips_at_bottom_edge() {
        // 3×3 sub pasted at (1,3) in a 3×4 main → only 3×2 copied.
        let mut main = make_buf(3, 4, ".");
        let sub = make_buf(3, 3, "#");
        blit_into(&mut main, &sub, 1, 3);
        let rows = row_texts(&main);
        assert_eq!(rows[0], "...");
        assert_eq!(rows[1], "...");
        assert_eq!(rows[2], "###");
        assert_eq!(rows[3], "###");
    }

    #[test]
    fn blit_start_past_right_is_noop() {
        let mut main = make_buf(4, 3, ".");
        let before = main.cells.clone();
        let sub = make_buf(2, 2, "#");
        blit_into(&mut main, &sub, 10, 1);
        assert_eq!(main.cells, before);
    }

    #[test]
    fn blit_start_past_bottom_is_noop() {
        let mut main = make_buf(4, 3, ".");
        let before = main.cells.clone();
        let sub = make_buf(2, 2, "#");
        blit_into(&mut main, &sub, 1, 10);
        assert_eq!(main.cells, before);
    }

    #[test]
    fn blit_preserves_style_per_cell() {
        let mut main = make_buf(3, 1, ".");
        let mut sub = make_buf(2, 1, "a");
        // Give the two sub cells distinct styles.
        sub.cells[0].style.fg = "red".to_string();
        sub.cells[1].style.bold = true;
        blit_into(&mut main, &sub, 1, 1);
        assert_eq!(main.cells[0].text, "a");
        assert_eq!(main.cells[0].style.fg, "red");
        assert!(!main.cells[0].style.bold);
        assert_eq!(main.cells[1].text, "a");
        assert_eq!(main.cells[1].style.fg, "");
        assert!(main.cells[1].style.bold);
        // The third main cell is untouched.
        assert_eq!(main.cells[2].text, ".");
    }

    #[test]
    fn blit_preserves_wide_char_placeholder() {
        // A wide char in the sub takes two consecutive cells with the
        // second one holding a placeholder " ". Copy must preserve
        // that pairing verbatim.
        let mut main = make_buf(4, 1, ".");
        let mut sub = make_buf(3, 1, "?");
        sub.cells[0] = Cell {
            text: "漢".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[1] = Cell {
            text: " ".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[2] = Cell {
            text: "x".to_string(),
            style: CellStyle::empty(),
        };
        blit_into(&mut main, &sub, 1, 1);
        assert_eq!(main.cells[0].text, "漢");
        assert_eq!(main.cells[1].text, " "); // placeholder preserved
        assert_eq!(main.cells[2].text, "x");
        assert_eq!(main.cells[3].text, "."); // main untouched past sub.cols
    }

    #[test]
    fn blit_zero_size_sub_is_noop() {
        let mut main = make_buf(3, 3, ".");
        let before = main.cells.clone();
        let sub = BufferState {
            cols: 0,
            rows: 0,
            cells: Vec::new(),
            cursor_col: 1,
            cursor_row: 1,
            cursor_visible: true,
            row_hashes: None,
        };
        blit_into(&mut main, &sub, 1, 1);
        assert_eq!(main.cells, before);
    }

    #[test]
    fn blit_drops_wide_lead_at_right_edge_when_clipped() {
        // main is 20 cols wide; sub is 3 cols wide and its 3rd cell is
        // a wide-char lead. Place sub so its 3rd cell lands at col 20
        // (main.cols) — the placeholder would need col 21 which does
        // not exist. Per BufferWrite contract the whole wide-char is
        // dropped, not just the placeholder.
        //
        // Start col = 20 - 3 + 1 = 18, i.e. `blit_into(main, sub, 18, 1)`.
        // Wait — that lays sub cols 1..3 at main cols 18..20 without
        // any clipping. To force the drop path we need `width < sub_cols`
        // so the clip check fires. Place the sub so its 3rd cell would
        // land on main col 21 (which gets clipped) and its 2nd cell —
        // the wide-char lead — lands on main col 20.
        let mut main = make_buf(20, 1, ".");
        let mut sub = make_buf(3, 1, "_");
        // sub layout: ['x', '漢'(wide-lead), ' '(placeholder)]
        sub.cells[0] = Cell {
            text: "x".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[1] = Cell {
            text: "漢".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[2] = Cell {
            text: " ".to_string(),
            style: CellStyle::empty(),
        };
        // col = 19 → sub[0] at main[18], sub[1] at main[19] (= col 20,
        // the right edge), sub[2] at main[20] (= col 21, clipped).
        blit_into(&mut main, &sub, 19, 1);
        // After the copy: sub[0] wrote at main col 19 → cells[18].
        // The wide-char lead at sub[1] would land at main col 20 but
        // its placeholder (sub[2]) cannot fit → drop the lead too.
        assert_eq!(main.cells[18].text, "x");
        assert_eq!(main.cells[19].text, ".", "wide-char lead must drop");
        // Every other cell stays ".".
        for i in (0..main.cells.len()).filter(|i| *i != 18) {
            assert_eq!(
                main.cells[i].text, ".",
                "cell {i} should be untouched by blit"
            );
        }
    }

    #[test]
    fn blit_keeps_wide_lead_when_fully_in_bounds() {
        // Same fixture but place the wide-char so both lead *and*
        // placeholder fit. This guards the fix against being too
        // aggressive (dropping wide chars that are actually in-bounds).
        let mut main = make_buf(20, 1, ".");
        let mut sub = make_buf(3, 1, "_");
        sub.cells[0] = Cell {
            text: "x".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[1] = Cell {
            text: "漢".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[2] = Cell {
            text: " ".to_string(),
            style: CellStyle::empty(),
        };
        // col = 18 → sub[0..3] land at main cols 18..20 (all in-bounds).
        blit_into(&mut main, &sub, 18, 1);
        assert_eq!(main.cells[17].text, "x");
        assert_eq!(main.cells[18].text, "漢");
        assert_eq!(main.cells[19].text, " ");
    }

    #[test]
    fn blit_right_edge_drop_does_not_affect_unclipped_wide_chars() {
        // A wide char interior to `sub` must never be dropped — the
        // check only fires for the *last* cell in the clipped row.
        let mut main = make_buf(4, 1, ".");
        let mut sub = make_buf(5, 1, "_");
        // sub: ['漢', ' ', 'a', 'b', 'c']
        sub.cells[0] = Cell {
            text: "漢".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[1] = Cell {
            text: " ".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[2] = Cell {
            text: "a".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[3] = Cell {
            text: "b".to_string(),
            style: CellStyle::empty(),
        };
        sub.cells[4] = Cell {
            text: "c".to_string(),
            style: CellStyle::empty(),
        };
        // main has 4 cols → copy_width = 4, clipped at sub col 4.
        // sub[3] = 'b' is narrow → no drop. Result: "漢 ab".
        blit_into(&mut main, &sub, 1, 1);
        assert_eq!(main.cells[0].text, "漢");
        assert_eq!(main.cells[1].text, " ");
        assert_eq!(main.cells[2].text, "a");
        assert_eq!(main.cells[3].text, "b");
    }

    #[test]
    fn blit_large_over_larger_produces_overlay() {
        // 120×40 sub into 120×40 main — this is the production shape
        // that drove TMB-022. Correctness test (perf is a separate
        // criterion bench).
        let mut main = make_buf(120, 40, ".");
        let sub = make_buf(120, 40, "#");
        blit_into(&mut main, &sub, 1, 1);
        for cell in &main.cells {
            assert_eq!(cell.text, "#");
        }
    }
}
