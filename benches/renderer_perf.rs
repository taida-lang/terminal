//! Renderer performance benchmark (Phase 8 / TMB-020).
//!
//! These benches measure the **internal** Rust implementations in
//! `src/renderer/{state,ops,diff}.rs` directly — they do not go
//! through the FFI marshalling layer, because the criterion harness
//! cannot easily set up a `TaidaHostV1` callback table.
//!
//! ## Budget (TMB-020 acceptance criteria)
//!
//! - `BufferWrite` 120 chars on a 120×40 buffer: `< 500 µs`
//! - `composePane` (40 rows × 1 BufferWrite each) on 120×40: `< 5 ms`
//! - `RenderFrame` identical 120×40: `< 100 µs`
//! - `RenderFrame` 1-cell-diff 120×40: `< 2 ms`
//! - `RenderFull` 120×40: `< 5 ms`
//!
//! The pure-Taida implementation that this Phase replaces did
//! `composePane` 40×20 in 6081 ms (Hachikuma P-12-2 smoke); the new
//! Rust path should fit the entire budget below by an order of
//! magnitude on a typical developer machine.
//!
//! ## Note on units
//!
//! criterion reports throughput in nanoseconds by default. The
//! `<` budgets above are upper bounds — the bench passes if the
//! median falls under them. Regression gating (Phase 8 / TM-8g) is
//! handled by `scripts/check-bench-budget.sh` (hard gate against
//! the absolute budgets above) plus `scripts/compare-bench-baseline.sh`
//! (informational diff vs the committed `benches/baseline.json`)
//! invoked from `.github/workflows/bench.yml`.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

use taida_lang_terminal::renderer_bench_api::{
    self, BufferState, Cell, CellStyle, DiffOp, diff_kind,
};

/// Standard TUI buffer size used by the benches: 120 columns × 40
/// rows = 4800 cells, mirroring Hachikuma's typical pane layout.
const COLS: i64 = 120;
const ROWS: i64 = 40;

fn make_buffer(cols: i64, rows: i64) -> BufferState {
    let total = (cols * rows) as usize;
    let mut buf = BufferState {
        cols,
        rows,
        cells: vec![Cell::default_space(); total],
        cursor_col: 1,
        cursor_row: 1,
        cursor_visible: true,
        row_hashes: None,
    };
    // TMB-021: production callers always go through `parse_buffer`,
    // which populates `row_hashes` so `diff_buffers` can short-circuit
    // unchanged rows. Mirror that invariant in the bench fixtures so
    // the numbers reported here reflect the deployed hot-path.
    buf.compute_row_hashes();
    buf
}

/// Variant of `make_buffer` that **omits** `row_hashes`. Used by
/// regression-style benches that want to measure the slow fallback
/// path (no fingerprints available).
#[allow(dead_code)]
fn make_buffer_no_hashes(cols: i64, rows: i64) -> BufferState {
    let total = (cols * rows) as usize;
    BufferState {
        cols,
        rows,
        cells: vec![Cell::default_space(); total],
        cursor_col: 1,
        cursor_row: 1,
        cursor_visible: true,
        row_hashes: None,
    }
}

fn buffer_write_120_chars(c: &mut Criterion) {
    // Budget: < 500 µs. Pure-Taida hit O(W·N) so this used to be
    // multi-millisecond on the same shape.
    c.bench_function("buffer_write_120chars_120x40", |b| {
        let style = CellStyle::empty();
        let text = "x".repeat(120);
        b.iter_batched(
            || make_buffer(COLS, ROWS),
            |mut buf| {
                renderer_bench_api::write_text(&mut buf, 1, 1, &text, &style);
                buf
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn compose_pane_40_rows(c: &mut Criterion) {
    // Budget: < 5 ms. Mirrors the Hachikuma `composePane` smoke
    // that drove TMB-020 — 40 rows × 1 `BufferWrite("x" * 120)`.
    c.bench_function("compose_pane_40rows_120x40", |b| {
        let style = CellStyle::empty();
        let text = "x".repeat(120);
        b.iter_batched(
            || make_buffer(COLS, ROWS),
            |mut buf| {
                for r in 1..=40i64 {
                    renderer_bench_api::write_text(&mut buf, 1, r, &text, &style);
                }
                buf
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn render_full_120x40(c: &mut Criterion) {
    // Budget: < 5 ms. This is the read-only path emitted on a
    // resize / first paint.
    c.bench_function("render_full_120x40", |b| {
        let buf = make_buffer(COLS, ROWS);
        b.iter(|| renderer_bench_api::render_full(&buf));
    });
}

fn render_frame_identical(c: &mut Criterion) {
    // Budget: < 100 µs. Every cell matches → diff returns no ops →
    // render_ops emits an empty string. The path measures the cell
    // walk + comparison cost only.
    c.bench_function("render_frame_identical_120x40", |b| {
        let prev = make_buffer(COLS, ROWS);
        let next = make_buffer(COLS, ROWS);
        b.iter(|| {
            let (ops, requires_full) = renderer_bench_api::diff_buffers(&prev, &next);
            let _ = renderer_bench_api::render_ops_to_string(&ops);
            let _ = requires_full;
        });
    });
}

fn render_frame_one_cell_diff(c: &mut Criterion) {
    // Budget: < 2 ms. One Write op → render_ops emits a single
    // CursorMoveTo + char. With TMB-021's row-hash fast-path, only
    // row 1 is descended into (the other 39 rows match by hash).
    c.bench_function("render_frame_one_cell_diff_120x40", |b| {
        let prev = make_buffer(COLS, ROWS);
        let mut next = make_buffer(COLS, ROWS);
        next.cells[0] = Cell {
            text: "X".to_string(),
            style: CellStyle::empty(),
        };
        // After the mutation, `next.row_hashes` is stale — recompute
        // so the bench measures the production fast-path correctly.
        // (Production code re-parses the buffer after every mutation,
        // which calls `compute_row_hashes` inside `parse_buffer`.)
        next.row_hashes = None;
        renderer_bench_api::compute_row_hashes(&mut next);
        b.iter(|| {
            let (ops, _) = renderer_bench_api::diff_buffers(&prev, &next);
            renderer_bench_api::render_ops_to_string(&ops)
        });
    });
}

/// Sanity bench at multiple sizes so a regression at small N is
/// also caught (a non-linear constant factor on small buffers
/// signals an allocation bug).
fn render_full_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("render_full_scaling");
    for &(cols, rows) in &[(40i64, 20i64), (80, 24), (120, 40)] {
        let buf = make_buffer(cols, rows);
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{cols}x{rows}")),
            &buf,
            |b, buf| {
                b.iter(|| renderer_bench_api::render_full(buf));
            },
        );
    }
    group.finish();
}

fn render_ops_throughput(c: &mut Criterion) {
    // Build a synthetic 120-op list (one Write per row) and measure
    // the render throughput. The Taida-side `RenderOps` ultimately
    // walks one pass, so this measures the constant factor.
    c.bench_function("render_ops_120_writes", |b| {
        let style = CellStyle::empty();
        let ops: Vec<DiffOp> = (1..=120i64)
            .map(|c| DiffOp {
                kind: diff_kind::WRITE,
                col: c,
                row: 1,
                text: "x".to_string(),
                style: style.clone(),
            })
            .collect();
        b.iter(|| renderer_bench_api::render_ops_to_string(&ops));
    });
}

criterion_group!(
    benches,
    buffer_write_120_chars,
    compose_pane_40_rows,
    render_full_120x40,
    render_frame_identical,
    render_frame_one_cell_diff,
    render_full_scaling,
    render_ops_throughput,
);
criterion_main!(benches);
