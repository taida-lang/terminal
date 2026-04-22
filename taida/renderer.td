// taida/renderer.td
//
// Virtual screen buffer + diff engine + renderer (Phase 8 / TMB-020).
//
// This file is a **dispatch thunk** over the native renderer entries
// added in Phase 8 (`bufferPut` / `bufferWrite` / `bufferFillRect` /
// `bufferClear` / `bufferDiff` / `renderFull` / `renderFrame` /
// `renderOps`).
//
// Layer C (`TM_DESIGN.md`): the **public surface** stays the same as
// `@a.4` — symbol names, pack shapes, parameter order, throw types.
// Only the implementation moves from pure Taida to native to fix
// TMB-020 (O(N²) list-replace).
//
// History:
//   - Phase 4: API surface pinned (placeholder returns).
//   - Phase 7 (TMB-019): pure-Taida implementation on top of
//     Take/Drop/Append/Concat. Functionally correct but O(N²).
//   - Phase 8 (TMB-020): native implementation behind unchanged
//     facade names. Hot-path computations move to Rust where the
//     buffer is a `Vec<Cell>` with O(1) cell mutation.
//
// Naming convention:
//   - Native sentinels (lowercase): `bufferPut`, `bufferWrite`, ...
//     pre-injected by the interpreter from `native/addon.toml`.
//   - Public surface (uppercase): `BufferPut`, `BufferWrite`, ...
//     re-exported via the `<<<` clause at the bottom and re-aliased
//     in `terminal.td`.
//
// `BufferNew` / `BufferResize` stay pure-Taida because they only
// allocate; their O(N) cost is unavoidable and not the source of
// the TMB-020 regression.

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

// -- BufferNew (pure Taida — allocation only, not a hot path) --

// Build a row-major cells list of length `count` filled with the
// default cell. Called once per `BufferNew` / `BufferResize`; the
// O(N) Append cost is amortised across the lifetime of the buffer
// because subsequent put/write/diff calls go through native.
_makeCellsAppend acc remaining fill =
  nextAcc <=[ Append[acc, fill]()
  _makeCellsLoop(nextAcc, remaining - 1, fill)
=> :@()

_makeCellsLoop acc remaining fill =
  | remaining < 1 |> acc
  | _ |> _makeCellsAppend(acc, remaining, fill)
=> :@()

///@ Purpose: Create an empty buffer of the specified size
///@ Throws: RendererInvalidSize if cols < 1 or rows < 1
_bufferNewInner cols rows =
  fill <= Cell
  cells <= _makeCellsLoop(@[fill], cols * rows - 1, fill)
  ScreenBuffer(cols <= cols, rows <= rows, cells <= cells, cursor_col <= 1, cursor_row <= 1, cursor_visible <= true)
=> :@()

BufferNew cols rows =
  | cols < 1 |> Error(type <= "RendererInvalidSize", message <= "cols must be >= 1").throw()
  | rows < 1 |> Error(type <= "RendererInvalidSize", message <= "rows must be >= 1").throw()
  | _ |> _bufferNewInner(cols, rows)
=> :@()

// -- BufferResize (pure Taida — same rationale as BufferNew) --

///@ Purpose: Resize a buffer, allocating a fresh cells list
///@ AI-Context: v1 contract: cells are reset to `fill` rather than
///@   preserved across the resize. Cursor position is clamped into
///@   the new bounds.
_bufferResizeInner buf cols rows fill =
  fillCell <= Cell
  cells <= _makeCellsLoop(@[fillCell], cols * rows - 1, fillCell)
  cc <= If[buf.cursor_col > cols, cols, buf.cursor_col]()
  cr <= If[buf.cursor_row > rows, rows, buf.cursor_row]()
  ScreenBuffer(cols <= cols, rows <= rows, cells <= cells, cursor_col <= cc, cursor_row <= cr, cursor_visible <= buf.cursor_visible)
=> :@()

BufferResize buf cols rows fill =
  | cols < 1 |> Error(type <= "RendererInvalidSize", message <= "cols must be >= 1").throw()
  | rows < 1 |> Error(type <= "RendererInvalidSize", message <= "rows must be >= 1").throw()
  | _ |> _bufferResizeInner(buf, cols, rows, fill)
=> :@()

// -- Native dispatch --
//
// The native renderer entries (`bufferPut`, `bufferWrite`,
// `bufferFillRect`, `bufferClear`, `bufferDiff`, `renderFull`,
// `renderFrame`, `renderOps`) are pre-injected as addon sentinels
// **only into the package facade env** (`terminal.td`), not into
// sub-imports like this file. Aliasing them under uppercase names
// happens in `terminal.td`; this module exports only the **pure
// Taida** types and constructors (`Cell` / `CellStyle` /
// `ScreenBuffer` / `DiffOpKind` / `DiffOp` / `BufferNew` /
// `BufferResize`) so it can be re-imported safely.

<<< @(Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, BufferNew, BufferResize)
