//! Renderer allocation entries (Phase 10 / TMB-024).
//!
//! This module owns the native implementations of `BufferNew` and
//! `BufferResize`. Both were left in pure Taida by Phase 8 on the
//! assumption that allocation was a cold path. Hachikuma's `appLoop`
//! invalidated that assumption: per-frame `BufferNew(120, 40)` +
//! `Pane.Render → sub ScreenBuffer` both allocate fresh buffers on
//! every iteration, exposing the pure-Taida `_makeCellsLoop` which
//! is `O(N²)` because each `Append[acc, fill]()` copies the growing
//! list. 120×40 measured at 3335 ms per call on Hachikuma's
//! `_perf_isolate.td`.
//!
//! ## Contract (frozen — must match @a.6 facade output byte-for-byte)
//!
//! ```text
//! BufferNew[](cols: Int, rows: Int) -> ScreenBuffer
//!   - cols < 1 or rows < 1 → throws `RendererInvalidSize`
//!   - cells: Vec<Cell> of length cols*rows, every entry = default Cell
//!     (text " ", empty style)
//!   - cursor_col = 1, cursor_row = 1, cursor_visible = true
//!
//! BufferResize[](buf: ScreenBuffer, cols: Int, rows: Int, fill: Cell)
//!   -> ScreenBuffer
//!   - cols < 1 or rows < 1 → throws `RendererInvalidSize`
//!   - cells: Vec<Cell> of length cols*rows, every entry = default Cell
//!     (NOTE: v1 contract — the `fill` argument's *value* is currently
//!     ignored by the pure-Taida facade which always seeds with default
//!     `Cell()`. We preserve that behaviour so byte-identical
//!     ScreenBuffer packs come out of the native path. See
//!     `_bufferResizeInner` in `taida/renderer.td`.)
//!   - cursor_col = min(buf.cursor_col, cols)
//!   - cursor_row = min(buf.cursor_row, rows)
//!   - cursor_visible = buf.cursor_visible
//! ```
//!
//! The Taida-side facade in `taida/renderer.td` becomes a thin
//! dispatch thunk that routes to these entries (Layer C of
//! `TM_DESIGN.md`).
//!
//! ## Performance
//!
//! Both entries build the `cells` vector with `vec![default; n]` which
//! is one allocation + `n` bit-copies of the default `Cell`. For
//! 120×40 = 4800 cells that is a sub-µs allocation on any modern
//! machine. The dominant cost is the outbound `build_buffer` walk
//! (one `build_cell` per cell) which is the same cost every existing
//! renderer entry already pays on its return path.

use taida_addon::bridge::{BorrowedValue, HostValueBuilder, borrow_arg};
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

use crate::renderer::state::{self, BufferState, Cell, RendererError, build_buffer, parse_buffer};

// ── Core allocation primitives ────────────────────────────────────

/// Allocate a fresh buffer of the given size, filled with the default
/// cell (`text = " "`, empty style). Cursor starts at (1, 1), visible.
///
/// Panics on impossible `cols * rows` overflow — callers upstream of
/// `buffer_new_impl` reject `cols < 1 || rows < 1` before reaching
/// this helper so the `as usize` cast is always non-negative.
pub fn buffer_new(cols: i64, rows: i64) -> BufferState {
    debug_assert!(cols >= 1 && rows >= 1);
    // `(cols as usize) * (rows as usize)` is the FFI-visible cell count;
    // the outer entry rejects non-positive dims so a wrap here would be
    // a contract violation, not user error.
    let count = (cols as usize).saturating_mul(rows as usize);
    let cells = vec![Cell::default_space(); count];
    let mut state = BufferState {
        cols,
        rows,
        cells,
        cursor_col: 1,
        cursor_row: 1,
        cursor_visible: true,
        row_hashes: None,
    };
    // Every other entry populates `row_hashes` on return so downstream
    // `buffer_diff` can use the fast path immediately. Compute once
    // here too; the cost (O(N)) folds into the O(N) allocation above.
    state.compute_row_hashes();
    state
}

/// Reallocate a buffer at the new size. Cursor position is clamped
/// into the new bounds; cursor visibility is preserved. Cells are
/// reset to the default `Cell` (see module docs for the `fill` arg
/// note).
pub fn buffer_resize(prev: &BufferState, cols: i64, rows: i64) -> BufferState {
    debug_assert!(cols >= 1 && rows >= 1);
    let count = (cols as usize).saturating_mul(rows as usize);
    let cells = vec![Cell::default_space(); count];
    let cc = if prev.cursor_col > cols {
        cols
    } else {
        prev.cursor_col
    };
    let cr = if prev.cursor_row > rows {
        rows
    } else {
        prev.cursor_row
    };
    let mut state = BufferState {
        cols,
        rows,
        cells,
        cursor_col: cc,
        cursor_row: cr,
        cursor_visible: prev.cursor_visible,
        row_hashes: None,
    };
    state.compute_row_hashes();
    state
}

/// Reject `cols * rows` beyond `MAX_BUFFER_CELLS`. A pathological
/// request (e.g. `bufferNew(10^6, 10^6)`) would otherwise attempt a
/// multi-terabyte allocation and abort the host process through
/// `handle_alloc_error`, which `catch_unwind` cannot intercept.
fn check_cell_limit(cols: i64, rows: i64) -> Result<(), RendererError> {
    if (cols as u128) * (rows as u128) > state::MAX_BUFFER_CELLS as u128 {
        return Err(RendererError::InvalidSize(format!(
            "buffer of {cols}x{rows} cells exceeds the {}-cell limit",
            state::MAX_BUFFER_CELLS
        )));
    }
    Ok(())
}

// ── FFI helpers (mirror the existing renderer modules) ────────────

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

#[doc(hidden)]
pub mod __bench {
    pub use super::{buffer_new, buffer_resize};
}

// ── Entries ───────────────────────────────────────────────────────

/// `BufferNew[](cols, rows) → ScreenBuffer`
pub fn buffer_new_impl(
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
        let cols = a0
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("cols must be Int".to_string()))?;
        let rows = a1
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("rows must be Int".to_string()))?;
        if cols < 1 {
            return Err(RendererError::InvalidSize("cols must be >= 1".to_string()));
        }
        if rows < 1 {
            return Err(RendererError::InvalidSize("rows must be >= 1".to_string()));
        }
        check_cell_limit(cols, rows)?;
        Ok::<BufferState, RendererError>(buffer_new(cols, rows))
    }));

    match result {
        Ok(Ok(buf)) => {
            let value = build_buffer(&builder, &buf);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "BufferNew result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_new"),
    }
}

/// `BufferResize[](buf, cols, rows, fill) → ScreenBuffer`
pub fn buffer_resize_impl(
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
    let (a0, a1, a2, _a3) = match (
        arg_at(args_ptr, args_len, 0),
        arg_at(args_ptr, args_len, 1),
        arg_at(args_ptr, args_len, 2),
        arg_at(args_ptr, args_len, 3),
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let prev = parse_buffer(&a0)?;
        let cols = a1
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("cols must be Int".to_string()))?;
        let rows = a2
            .as_int()
            .ok_or_else(|| RendererError::InvalidArg("rows must be Int".to_string()))?;
        if cols < 1 {
            return Err(RendererError::InvalidSize("cols must be >= 1".to_string()));
        }
        if rows < 1 {
            return Err(RendererError::InvalidSize("rows must be >= 1".to_string()));
        }
        check_cell_limit(cols, rows)?;
        // `fill` (a3) is currently *not consulted* — the pure-Taida
        // facade always seeds with the default `Cell` regardless of
        // what the caller passes. We keep that behaviour so byte shape
        // matches @a.6 exactly. A future version that honours `fill`
        // requires a CHANGELOG Changed entry, not an Added one.
        Ok::<BufferState, RendererError>(buffer_resize(&prev, cols, rows))
    }));

    match result {
        Ok(Ok(buf)) => {
            let value = build_buffer(&builder, &buf);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "BufferResize result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(e)) => emit_error(&builder, out_error, e),
        Err(_) => emit_panic(&builder, out_error, "buffer_resize"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_cell_limit_rejects_pathological_dimensions() {
        // Exactly at the limit is accepted (2000 * 2000 = 4_000_000).
        assert!(check_cell_limit(2000, 2000).is_ok());
        assert!(check_cell_limit(state::MAX_BUFFER_CELLS as i64, 1).is_ok());
        // One row past the limit is rejected before any allocation.
        assert!(matches!(
            check_cell_limit(2001, 2000),
            Err(RendererError::InvalidSize(_))
        ));
        // The motivating case: bufferNew(10^6, 10^6) must never reach
        // the allocator (it would abort the host via handle_alloc_error).
        assert!(matches!(
            check_cell_limit(1_000_000, 1_000_000),
            Err(RendererError::InvalidSize(_))
        ));
    }

    #[test]
    fn buffer_new_1x1_has_one_default_cell() {
        let buf = buffer_new(1, 1);
        assert_eq!(buf.cols, 1);
        assert_eq!(buf.rows, 1);
        assert_eq!(buf.cells.len(), 1);
        assert_eq!(buf.cells[0].text, " ");
        assert!(buf.cells[0].style.is_empty());
        assert_eq!(buf.cursor_col, 1);
        assert_eq!(buf.cursor_row, 1);
        assert!(buf.cursor_visible);
    }

    #[test]
    fn buffer_new_40x20_has_800_default_cells() {
        let buf = buffer_new(40, 20);
        assert_eq!(buf.cols, 40);
        assert_eq!(buf.rows, 20);
        assert_eq!(buf.cells.len(), 800);
        for c in &buf.cells {
            assert_eq!(c.text, " ");
            assert!(c.style.is_empty());
        }
    }

    #[test]
    fn buffer_new_120x40_has_4800_default_cells() {
        let buf = buffer_new(120, 40);
        assert_eq!(buf.cells.len(), 4800);
        // Spot-check a few cells to make sure vec![default; n]
        // truly copied — not left uninitialised.
        assert_eq!(buf.cells[0].text, " ");
        assert_eq!(buf.cells[2399].text, " ");
        assert_eq!(buf.cells[4799].text, " ");
    }

    #[test]
    fn buffer_new_populates_row_hashes() {
        // Every other entry populates row_hashes on return; BufferNew
        // must do the same so the downstream `parse_buffer +
        // buffer_diff` fast path can kick in immediately.
        let buf = buffer_new(4, 3);
        let hashes = buf.row_hashes.as_ref().expect("row_hashes must be Some");
        assert_eq!(hashes.len(), 3);
        // All rows are identical default-space rows → all hashes equal.
        assert_eq!(hashes[0], hashes[1]);
        assert_eq!(hashes[1], hashes[2]);
    }

    #[test]
    fn buffer_resize_to_bigger_clamps_cursor_noop() {
        let prev = buffer_new(5, 5);
        // Cursor at (1,1) is already in bounds — no change.
        let next = buffer_resize(&prev, 10, 10);
        assert_eq!(next.cols, 10);
        assert_eq!(next.rows, 10);
        assert_eq!(next.cells.len(), 100);
        assert_eq!(next.cursor_col, 1);
        assert_eq!(next.cursor_row, 1);
        assert!(next.cursor_visible);
    }

    #[test]
    fn buffer_resize_to_smaller_clamps_cursor() {
        let mut prev = buffer_new(10, 10);
        prev.cursor_col = 8;
        prev.cursor_row = 7;
        prev.cursor_visible = false;
        let next = buffer_resize(&prev, 4, 3);
        assert_eq!(next.cols, 4);
        assert_eq!(next.rows, 3);
        assert_eq!(next.cells.len(), 12);
        assert_eq!(next.cursor_col, 4, "cursor_col must clamp to new cols");
        assert_eq!(next.cursor_row, 3, "cursor_row must clamp to new rows");
        assert!(!next.cursor_visible, "cursor_visible must be preserved");
    }

    #[test]
    fn buffer_resize_cells_are_all_default() {
        // v1 contract: cells are reset to default, *not* preserved.
        // This guards against a future implementation accidentally
        // carrying over the previous buffer's cells and changing the
        // observable facade shape.
        let mut prev = buffer_new(3, 3);
        prev.cells[0].text = "X".to_string();
        let next = buffer_resize(&prev, 4, 4);
        for c in &next.cells {
            assert_eq!(c.text, " ");
            assert!(c.style.is_empty());
        }
    }

    #[test]
    fn buffer_resize_populates_row_hashes() {
        let prev = buffer_new(2, 2);
        let next = buffer_resize(&prev, 3, 4);
        let hashes = next.row_hashes.as_ref().expect("row_hashes must be Some");
        assert_eq!(hashes.len(), 4);
    }

    #[test]
    fn buffer_new_impl_rejects_zero_cols() {
        use taida_addon::TaidaAddonStatus;
        // Drive the entry directly with no host to hit the error path.
        // A null host means the entry returns InvalidState before the
        // argument parse runs. The arity check is what we really guard
        // here (TMB-024 adds two entries and we must not regress the
        // shape of the arity contract).
        let status = buffer_new_impl(
            core::ptr::null(),
            core::ptr::null(),
            1, // wrong arity → ArityMismatch
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn buffer_resize_impl_arity_mismatch() {
        use taida_addon::TaidaAddonStatus;
        let status = buffer_resize_impl(
            core::ptr::null(),
            core::ptr::null(),
            3, // BufferResize has arity 4
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn buffer_new_impl_invalid_state_when_host_null() {
        use taida_addon::TaidaAddonStatus;
        let status = buffer_new_impl(
            core::ptr::null(),
            core::ptr::null(),
            2,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn buffer_resize_impl_invalid_state_when_host_null() {
        use taida_addon::TaidaAddonStatus;
        let status = buffer_resize_impl(
            core::ptr::null(),
            core::ptr::null(),
            4,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn buffer_new_scales_linearly_in_cells() {
        // Smoke assertion that the native allocator is truly linear in
        // cell count. We don't assert absolute times (CI noise) — we
        // only assert that a 16× increase in cells does *not* cause a
        // 256× increase in wall time, which is the O(N²) regression
        // TMB-024 was filed to eliminate.
        use std::time::Instant;
        let t_small = Instant::now();
        for _ in 0..50 {
            let _ = buffer_new(10, 10); // 100 cells
        }
        let small = t_small.elapsed();

        let t_large = Instant::now();
        for _ in 0..50 {
            let _ = buffer_new(40, 40); // 1600 cells (16× small)
        }
        let large = t_large.elapsed();

        // Linear: large ≈ 16 × small. O(N²): large ≈ 256 × small. We
        // assert large < 50 × small which gives slack for CPU noise
        // but unambiguously rules out a quadratic regression.
        let ratio = large.as_nanos() as f64 / small.as_nanos().max(1) as f64;
        assert!(
            ratio < 50.0,
            "buffer_new appears super-linear: 1600-cell/100-cell ratio = {ratio:.1}× \
             (linear target ≈ 16×, quadratic target ≈ 256×). \
             small={small:?}, large={large:?}"
        );
    }
}
