// taida/renderer.td
//
// Virtual screen buffer + diff engine + renderer (Phase 8 / TMB-020,
// Phase 9 / TMB-022, Phase 10 / TMB-024).
//
// This file is a **pure-Taida types module** for the renderer. All
// runtime dispatch aliases (`BufferPut`, `BufferWrite`, `BufferNew`,
// `BufferResize`, ...) are defined in `terminal.td` because addon
// sentinels are pre-injected only into the top-level facade's env.
//
// Layer C (`TM_DESIGN.md`): the **public surface** stays the same as
// `@a.6` — symbol names, pack shapes, parameter order, throw types.
// Only the implementation moves from pure Taida to native.
//
// History:
//   - Phase 4: API surface pinned (placeholder returns).
//   - Phase 7 (TMB-019): pure-Taida implementation on top of
//     Take/Drop/Append/Concat. Functionally correct but O(N²).
//   - Phase 8 (TMB-020): native implementation behind unchanged
//     facade names (put / write / fill_rect / diff / render_*).
//   - Phase 9 (TMB-022): `BufferBlit` native primitive.
//   - Phase 10 (TMB-024): `BufferNew` / `BufferResize` native
//     allocation. Removes the last pure-Taida hot path (O(N²)
//     `_makeCellsLoop` via repeated `Append[acc, fill]()`).
//
// As of Phase 10 this file exports **only** pure-Taida type packs.
// The runtime dispatch is the responsibility of `terminal.td`.

// -- Cell --

///@ Purpose: One cell of data (character + style)
///@ AI-Context: Default text is " ". The renderer normalizes empty
///@   text to " " for rendering. Wide-char placeholder cells also
///@   carry " " as the second-half text.
Cell <= @(
  text      <= " "
  fg        <= ""
  bg        <= ""
  bold      <= false
  dim       <= false
  underline <= false
  italic    <= false
)

///@ Purpose: Default style options for BufferWrite callers
///@ AI-Context: BufferWrite style arg must be this 6-field shape. Use
///@   `CellStyle(fg <= "red", bg <= "", bold <= false, dim <= false,
///@   underline <= false, italic <= false)` — every field must be present.
CellStyle <= @(
  fg        <= ""
  bg        <= ""
  bold      <= false
  dim       <= false
  underline <= false
  italic    <= false
)

// -- ScreenBuffer --

///@ Purpose: Virtual screen buffer (row-major flat cells)
///@ AI-Context: Use `BufferNew[](cols, rows)` to allocate. Direct
///@   construction is allowed but the cells list length must equal
///@   cols*rows or the native renderer rejects the buffer with
///@   `RendererInvalidArg`.
ScreenBuffer <= @(
  cols           <= 0
  rows           <= 0
  cells          <= @[" "]
  cursor_col     <= 1
  cursor_row     <= 1
  cursor_visible <= true
)

// -- DiffOpKind --

///@ Purpose: Diff operation kind
///@ AI-Context: Tag values are frozen — the Rust `renderer::diff`
///@   matches against these literals. Renumbering breaks the addon ABI.
DiffOpKind <= @(
  MoveTo     <= 0
  Write      <= 1
  ClearLine  <= 2
  ShowCursor <= 3
  HideCursor <= 4
)

// -- DiffOp --

///@ Purpose: A single diff operation
DiffOp <= @(
  kind  <= DiffOpKind.Write
  col   <= 1
  row   <= 1
  text  <= ""
  style <= @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
)

// -- Native dispatch --
//
// The native renderer entries (`bufferPut`, `bufferWrite`,
// `bufferFillRect`, `bufferClear`, `bufferDiff`, `renderFull`,
// `renderFrame`, `renderOps`, `bufferBlit`, `bufferNew`,
// `bufferResize`) are pre-injected as addon sentinels **only into the
// package facade env** (`terminal.td`), not into sub-imports like this
// file. Aliasing them under uppercase names happens in `terminal.td`;
// this module exports only the **pure Taida types** (`Cell` /
// `CellStyle` / `ScreenBuffer` / `DiffOpKind` / `DiffOp`) so it can be
// re-imported safely.

<<< @(Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp)
