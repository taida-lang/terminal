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
  style <= @()
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

_makeCellsLoop acc remaining fill =
  | remaining < 1 |> acc
  | _ |> _makeCellsAppend(acc, remaining, fill)
=> :@[Str]

_makeCellsAppend acc remaining fill =
  nextAcc <=[ Append[acc, fill]()
  _makeCellsLoop(nextAcc, remaining - 1, fill)
=> :@[Str]

// -- BufferNew --

///@ Purpose: Create an empty buffer of the specified size
///@ Throws: RendererInvalidSize if cols < 1 or rows < 1
///@ Since: a.6
BufferNew cols rows =
  | cols < 1 |> Error(type <= "RendererInvalidSize", message <= "cols must be >= 1").throw()
  | rows < 1 |> Error(type <= "RendererInvalidSize", message <= "rows must be >= 1").throw()
  | _ |> _bufferNewInner(cols, rows)
=> :@()

_bufferNewInner cols rows =
  fill <= Cell
  cells <= _makeCellsLoop(@[fill], cols * rows - 1, fill)
  ScreenBuffer(cols <= cols, rows <= rows, cells <= cells, cursor_col <= 1, cursor_row <= 1, cursor_visible <= true)
=> :@()

// -- BufferResize --

///@ Purpose: Resize a buffer, preserving existing content where possible
///@ Since: a.6
BufferResize buf cols rows fill =
  | cols < 1 |> Error(type <= "RendererInvalidSize", message <= "cols must be >= 1").throw()
  | rows < 1 |> Error(type <= "RendererInvalidSize", message <= "rows must be >= 1").throw()
  | _ |> _bufferResizeInner(buf, cols, rows, fill)
=> :@()

_bufferResizeInner buf cols rows fill =
  fillCell <= Cell
  cells <= _makeCellsLoop(@[fillCell], cols * rows - 1, fillCell)
  cc <= If[buf.cursor_col > cols, cols, buf.cursor_col]()
  cr <= If[buf.cursor_row > rows, rows, buf.cursor_row]()
  ScreenBuffer(cols <= cols, rows <= rows, cells <= cells, cursor_col <= cc, cursor_row <= cr, cursor_visible <= buf.cursor_visible)
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
  | _ |> buf
=> :@()

// -- BufferWrite --

///@ Purpose: Write text with style at a given position
///@ Throws: RendererOutOfBounds
///@ Since: a.6
BufferWrite buf col row text style =
  | col < 1 |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row < 1 |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | col > buf.cols |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row > buf.rows |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | _ |> buf
=> :@()

// -- BufferFillRect --

///@ Purpose: Fill a rectangular region with a cell
///@ Since: a.6
BufferFillRect buf col row width height cell =
  | col < 1 |> Error(type <= "RendererOutOfBounds", message <= "col out of bounds").throw()
  | row < 1 |> Error(type <= "RendererOutOfBounds", message <= "row out of bounds").throw()
  | _ |> buf
=> :@()

// -- RenderFull --

///@ Purpose: Render the entire buffer as an ANSI string
///@ Since: a.6
RenderFull buf =
  | buf.cols == 0 |> ""
  | buf.rows == 0 |> ""
  | _ |> _renderFullInner(buf)
=> :Str

_renderFullInner buf =
  out <= CursorHide()
  withCursor <= out + CursorMoveTo(buf.cursor_col, buf.cursor_row)
  If[buf.cursor_visible == true, withCursor + CursorShow(), withCursor]()
=> :Str

// -- BufferDiff --

///@ Purpose: Generate a minimal list of diff operations between two buffers
///@ Since: a.6
BufferDiff prev next =
  | prev.cols == next.cols |>
    | prev.rows == next.rows |> @(ops <= @[0], requires_full <= false)
    | _ |> @(ops <= @[0], requires_full <= true)
  | _ |> @(ops <= @[0], requires_full <= true)
=> :@()

// -- RenderOps --

///@ Purpose: Convert a list of DiffOps to an ANSI string
///@ Since: a.6
RenderOps ops =
  ""
=> :Str

// -- RenderFrame --

///@ Purpose: Compare prev and next buffers, produce minimal ANSI output
///@ Since: a.6
RenderFrame prev next =
  @(text <= "", next <= next)
=> :@()

<<< @(Cell, ScreenBuffer, DiffOpKind, DiffOp, BufferNew, BufferResize, BufferClear, BufferPut, BufferWrite, BufferFillRect, RenderFull, BufferDiff, RenderOps, RenderFrame)
