// taida/renderer.td
//
// Virtual screen buffer, diff engine, and renderer.
//
// This is Layer C (Renderer foundation) per TM_DESIGN.md:
//   - No syscalls
//   - No hidden side effects
//   - Pure computation on buffer state
//
// Buffer layout: cells is a row-major flat list.
//   Index = (row - 1) * cols + (col - 1)   (1-based col/row)
//
// Phase 7 (TMB-019): BufferPut / BufferWrite / BufferFillRect / RenderFull
// / BufferDiff / RenderOps / RenderFrame were placeholder returns in Phase 4.
// This file implements them in pure Taida on top of the Take / Drop / Append
// / Concat list molds. Mutual recursion is unified into single functions
// because the facade module loader does not resolve forward references
// beyond one level (TMB-010 note).

// -- Imports --

>>> ./ansi.td => @(CursorMoveTo, CursorHide, CursorShow, ClearLine)
>>> ./style.td => @(Stylize, ResetStyle)
>>> ./width.td => @(DisplayWidth, MeasureGrapheme, NormalizeCellText)

// -- Cell --

///@ Purpose: One cell of data (character + style)
///@ Since: a.6
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
///@ Since: a.4
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
///@ Since: a.6
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
///@ Since: a.6
DiffOpKind <= @(
  MoveTo     <= 0
  Write      <= 1
  ClearLine  <= 2
  ShowCursor <= 3
  HideCursor <= 4
)

// -- DiffOp --

///@ Purpose: A single diff operation
///@ Since: a.6
DiffOp <= @(
  kind  <= DiffOpKind.Write
  col   <= 1
  row   <= 1
  text  <= ""
  style <= @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
)

// -- Internal helpers --

_cellIndex cols col row =
  (row - 1) * cols + (col - 1)
=> :Int

_hasStyle cell =
  | cell.fg == "" |>
    | cell.bg == "" |>
      | cell.bold == false |>
        | cell.dim == false |>
          | cell.underline == false |>
            | cell.italic == false |> false
            | _ |> true
          | _ |> true
        | _ |> true
      | _ |> true
    | _ |> true
  | _ |> true
=> :Bool

_cellStyleOpts cell =
  @(fg <= cell.fg, bg <= cell.bg, bold <= cell.bold, dim <= cell.dim, underline <= cell.underline, italic <= cell.italic)
=> :@()

_emptyStyle =
  @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
=> :@()

_styleHasAny style =
  | style.fg == "" |>
    | style.bg == "" |>
      | style.bold == false |>
        | style.dim == false |>
          | style.underline == false |>
            | style.italic == false |> false
            | _ |> true
          | _ |> true
        | _ |> true
      | _ |> true
    | _ |> true
  | _ |> true
=> :Bool

_cellEq a b =
  | a.text == b.text |>
    | a.fg == b.fg |>
      | a.bg == b.bg |>
        | a.bold == b.bold |>
          | a.dim == b.dim |>
            | a.underline == b.underline |>
              | a.italic == b.italic |> true
              | _ |> false
            | _ |> false
          | _ |> false
        | _ |> false
      | _ |> false
    | _ |> false
  | _ |> false
=> :Bool

_buildCell ch style =
  Cell(text <= ch, fg <= style.fg, bg <= style.bg, bold <= style.bold, dim <= style.dim, underline <= style.underline, italic <= style.italic)
=> :@()

// List replace-at helper: produce xs with xs[idx] replaced by newV.
// Pure-Taida implementation on top of Take + Drop + Append + Concat.
_listSet xs idx newV =
  prefix <=[ Take[xs, idx]()
  suffix <=[ Drop[xs, idx + 1]()
  withNew <=[ Append[prefix, newV]()
  Concat[withNew, suffix]()
=> :@()

_makeCellsAppend acc remaining fill =
  nextAcc <=[ Append[acc, fill]()
  _makeCellsLoop(nextAcc, remaining - 1, fill)
=> :@()

_makeCellsLoop acc remaining fill =
  | remaining < 1 |> acc
  | _ |> _makeCellsAppend(acc, remaining, fill)
=> :@()

// Put cell at (col, row) into a buffer without bounds checking.
// Used both by BufferPut's guarded path and internal helpers.
_bufferPutInner buf col row cell =
  idx <= _cellIndex(buf.cols, col, row)
  newCells <= _listSet(buf.cells, idx, cell)
  ScreenBuffer(cols <= buf.cols, rows <= buf.rows, cells <= newCells, cursor_col <= buf.cursor_col, cursor_row <= buf.cursor_row, cursor_visible <= buf.cursor_visible)
=> :@()

// -- BufferNew --

///@ Purpose: Create an empty buffer of the specified size
///@ Throws: RendererInvalidSize if cols < 1 or rows < 1
///@ Since: a.6
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

// -- BufferResize --

///@ Purpose: Resize a buffer, preserving existing content where possible
///@ Since: a.6
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

// -- BufferClear --

///@ Purpose: Clear the entire buffer with a fill cell
///@ Since: a.6
BufferClear buf fill =
  cells <= _makeCellsLoop(@[fill], buf.cols * buf.rows - 1, fill)
  ScreenBuffer(cols <= buf.cols, rows <= buf.rows, cells <= cells, cursor_col <= buf.cursor_col, cursor_row <= buf.cursor_row, cursor_visible <= buf.cursor_visible)
=> :@()

// -- BufferPut --

///@ Purpose: Write a single cell at a given position
///@ Throws: RendererOutOfBounds
///@ Since: a.6
BufferPut buf col row cell =
  | col < 1 |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row < 1 |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | col > buf.cols |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row > buf.rows |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | _ |> _bufferPutInner(buf, col, row, cell)
=> :@()

// -- BufferWrite --
//
// Single-function recursion (no mutual refs). The worker walks chars,
// handles width-0 (combining) by skipping, width-2 (wide) by emitting
// a styled placeholder in the second cell, and truncates at right edge.

_bwWorker buf col row text idx style len =
  | idx == len |> buf
  | col > buf.cols |> buf
  | _ |>
    CharAt[text, idx]() ]=> ch
    CodePoint[ch]() ]=> cp
    step <= If[cp > 65535, 2, 1]()
    m <= MeasureGrapheme(ch)
    | m.width == 0 |> _bwWorker(buf, col, row, text, idx + step, style, len)
    | col + m.width - 1 > buf.cols |> buf
    | _ |>
      cell <= _buildCell(ch, style)
      updated <= _bufferPutInner(buf, col, row, cell)
      withPlaceholder <= If[m.width == 2, _bufferPutInner(updated, col + 1, row, _buildCell(" ", style)), updated]()
      _bwWorker(withPlaceholder, col + m.width, row, text, idx + step, style, len)
=> :@()

///@ Purpose: Write text with style at a given position, advancing cursor by display width
///@ Params:
///@   buf:   ScreenBuffer
///@   col:   Int (1-based, must be in range)
///@   row:   Int (1-based, must be in range)
///@   text:  Str — normalized cell text (TAB / newline are pre-expanded)
///@   style: @(fg, bg, bold, dim, underline, italic) — all 6 fields required
///@ Throws: RendererOutOfBounds if col/row < 1 or > buf.cols / buf.rows
///@ Notes: truncates at right edge; wide chars (width 2) occupy 2 cells,
///@   second cell is a space placeholder with the same style; width 0
///@   graphemes (combining marks / control) are skipped.
///@ Since: a.6
BufferWrite buf col row text style =
  | col < 1 |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row < 1 |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | col > buf.cols |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row > buf.rows |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | _ |> _bwWorker(buf, col, row, text, 0, style, text.length())
=> :@()

// -- BufferFillRect --
//
// Nested single-function workers: column loop + row loop. The column
// worker fills one row's column slice, the row worker advances to the
// next row until height is exhausted.

_frColWorker buf col0 curCol curRow width cell =
  endCol <= col0 + width
  | curCol >= endCol |> buf
  | curCol > buf.cols |> buf
  | _ |>
    updated <= _bufferPutInner(buf, curCol, curRow, cell)
    _frColWorker(updated, col0, curCol + 1, curRow, width, cell)
=> :@()

_frRowWorker buf col0 row0 curRow width height cell =
  endRow <= row0 + height
  | curRow >= endRow |> buf
  | curRow > buf.rows |> buf
  | _ |>
    filled <= _frColWorker(buf, col0, col0, curRow, width, cell)
    _frRowWorker(filled, col0, row0, curRow + 1, width, height, cell)
=> :@()

///@ Purpose: Fill a rectangular region with a cell
///@ Since: a.6
BufferFillRect buf col row width height cell =
  | col < 1 |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row < 1 |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | width < 1 |> buf
  | height < 1 |> buf
  | col > buf.cols |> buf
  | row > buf.rows |> buf
  | _ |> _frRowWorker(buf, col, row, row, width, height, cell)
=> :@()

// -- RenderFull --
//
// Two single-function workers: per-cell within a row, per-row within the
// buffer. The outer worker is defined AFTER the inner one to respect the
// forward-reference constraint.

_rfCellWorker buf c r acc =
  | c > buf.cols |> acc
  | _ |>
    idx <= _cellIndex(buf.cols, c, r)
    buf.cells.get(idx) ]=> cell
    text <= If[cell.text == "", " ", cell.text]()
    styled <= If[_hasStyle(cell) == true, Stylize(text, _cellStyleOpts(cell)), text]()
    _rfCellWorker(buf, c + 1, r, acc + styled)
=> :Str

_rfRowWorker buf r acc =
  | r > buf.rows |> acc
  | _ |>
    rowStart <= CursorMoveTo(1, r)
    rowText <= _rfCellWorker(buf, 1, r, "")
    _rfRowWorker(buf, r + 1, acc + rowStart + rowText)
=> :Str

_renderFullInner buf =
  hidden <= CursorHide()
  body <= _rfRowWorker(buf, 1, "")
  cursorMove <= CursorMoveTo(buf.cursor_col, buf.cursor_row)
  visibility <= If[buf.cursor_visible == true, CursorShow(), ""]()
  hidden + body + cursorMove + visibility
=> :Str

///@ Purpose: Render the entire buffer as an ANSI string
///@ Since: a.6
RenderFull buf =
  | buf.cols == 0 |> ""
  | buf.rows == 0 |> ""
  | _ |> _renderFullInner(buf)
=> :Str

// -- BufferDiff --
//
// Single-function recursion for cell walk. Cursor visibility + position
// changes are appended after the cell diff as separate ops.

_diffCellsWorker prev next idx acc total =
  | idx == total |> acc
  | _ |>
    prev.cells.get(idx) ]=> prevCell
    next.cells.get(idx) ]=> nextCell
    equal <= _cellEq(prevCell, nextCell)
    | equal == true |> _diffCellsWorker(prev, next, idx + 1, acc, total)
    | _ |>
      rowIdx <=[ Div[idx, next.cols]()
      colIdx <=[ Mod[idx, next.cols]()
      col <= colIdx + 1
      row <= rowIdx + 1
      styleOpts <= _cellStyleOpts(nextCell)
      op <= DiffOp(kind <= DiffOpKind.Write, col <= col, row <= row, text <= nextCell.text, style <= styleOpts)
      newAcc <=[ Append[acc, op]()
      _diffCellsWorker(prev, next, idx + 1, newAcc, total)
=> :@()

_diffAppendVisOp ops kindTag =
  op <= DiffOp(kind <= kindTag, col <= 1, row <= 1, text <= "", style <= _emptyStyle())
  Append[ops, op]()
=> :@()

_diffAppendMoveOp ops col row =
  op <= DiffOp(kind <= DiffOpKind.MoveTo, col <= col, row <= row, text <= "", style <= _emptyStyle())
  Append[ops, op]()
=> :@()

_diffVisibility prev next ops =
  | prev.cursor_visible == next.cursor_visible |> ops
  | next.cursor_visible == true |> _diffAppendVisOp(ops, DiffOpKind.ShowCursor)
  | _ |> _diffAppendVisOp(ops, DiffOpKind.HideCursor)
=> :@()

_diffPosition prev next ops =
  sameCol <= prev.cursor_col == next.cursor_col
  sameRow <= prev.cursor_row == next.cursor_row
  | sameCol == true |>
    | sameRow == true |> ops
    | _ |> _diffAppendMoveOp(ops, next.cursor_col, next.cursor_row)
  | _ |> _diffAppendMoveOp(ops, next.cursor_col, next.cursor_row)
=> :@()

_diffSameSize prev next =
  cellOps <= _diffCellsWorker(prev, next, 0, @[], prev.cells.length())
  withVis <= _diffVisibility(prev, next, cellOps)
  withPos <= _diffPosition(prev, next, withVis)
  @(ops <= withPos, requires_full <= false)
=> :@()

_diffFullFallback =
  @(ops <= @[], requires_full <= true)
=> :@()

///@ Purpose: Generate a minimal list of diff operations between two buffers
///@ Since: a.6
BufferDiff prev next =
  | prev.cols == next.cols |>
    | prev.rows == next.rows |> _diffSameSize(prev, next)
    | _ |> _diffFullFallback()
  | _ |> _diffFullFallback()
=> :@()

// -- RenderOps --

_renderOp op =
  | op.kind == DiffOpKind.MoveTo |> CursorMoveTo(op.col, op.row)
  | op.kind == DiffOpKind.ClearLine |> ClearLine()
  | op.kind == DiffOpKind.ShowCursor |> CursorShow()
  | op.kind == DiffOpKind.HideCursor |> CursorHide()
  | op.kind == DiffOpKind.Write |>
    move <= CursorMoveTo(op.col, op.row)
    body <= If[_styleHasAny(op.style) == true, Stylize(op.text, op.style), op.text]()
    move + body
  | _ |> ""
=> :Str

_roWorker ops idx total acc =
  | idx == total |> acc
  | _ |>
    ops.get(idx) ]=> op
    _roWorker(ops, idx + 1, total, acc + _renderOp(op))
=> :Str

///@ Purpose: Convert a list of DiffOps to an ANSI string
///@ Since: a.6
RenderOps ops =
  _roWorker(ops, 0, ops.length(), "")
=> :Str

// -- RenderFrame --

///@ Purpose: Compare prev and next buffers, produce minimal ANSI output
///@ Since: a.6
RenderFrame prev next =
  diff <= BufferDiff(prev, next)
  | diff.requires_full == true |> @(text <= RenderFull(next), next <= next)
  | _ |> @(text <= RenderOps(diff.ops), next <= next)
=> :@()

<<< @(Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, BufferNew, BufferResize, BufferClear, BufferPut, BufferWrite, BufferFillRect, RenderFull, BufferDiff, RenderOps, RenderFrame)
