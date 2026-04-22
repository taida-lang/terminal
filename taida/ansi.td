// taida/ansi.td
//
// ANSI escape sequence helpers for screen and cursor control.
//
// All functions in this file return pure `Str` values containing ANSI
// escape sequences. They do NOT write to stdout -- the caller decides
// when to write via `stdout(...)`.
//
// This is Layer B (ANSI facade) per TM_DESIGN.md:
//   - No syscalls
//   - No hidden side effects
//   - Returns ANSI escape strings only
//
// -- Screen Control (TM-2f) --

///@ Purpose: Clear the entire screen and move cursor to (1,1)
///@ Returns: Str
///@ Example:
///@   stdout(ClearScreen())
///@ AI-SideEffects: none
ClearScreen =
  "\x1b[2J\x1b[H"
=> :Str

///@ Purpose: Clear current line and move cursor to beginning
///@ Returns: Str
///@ Example:
///@   stdout(ClearLine())
///@ AI-SideEffects: none
ClearLine =
  "\x1b[2K\r"
=> :Str

///@ Purpose: Switch to alternate screen buffer
///@ Returns: Str
///@ Example:
///@   stdout(AltScreenEnter())
///@ AI-SideEffects: none
AltScreenEnter =
  "\x1b[?1049h"
=> :Str

///@ Purpose: Switch back to main screen buffer
///@ Returns: Str
///@ Example:
///@   stdout(AltScreenLeave())
///@ AI-SideEffects: none
AltScreenLeave =
  "\x1b[?1049l"
=> :Str

// -- Cursor Control (TM-2g) --

///@ Purpose: Move cursor to (col, row) position (1-based)
///@ Params:
///@   col: Int -- 1-based column (< 1 throws CursorMoveInvalidPosition)
///@   row: Int -- 1-based row (< 1 throws CursorMoveInvalidPosition)
///@ Returns: Str
///@ Throws:
///@   CursorMoveInvalidPosition: col < 1 or row < 1
///@ Example:
///@   stdout(CursorMoveTo(10, 5))
///@ AI-SideEffects: none
CursorMoveTo col row =
  | col < 1 |> Error(type <= "CursorMoveInvalidPosition", message <= "col must be >= 1").throw()
  | row < 1 |> Error(type <= "CursorMoveInvalidPosition", message <= "row must be >= 1").throw()
  | _ |> "\x1b[" + row.toString() + ";" + col.toString() + "H"
=> :Str

///@ Purpose: Hide the cursor
///@ Returns: Str
///@ Example:
///@   stdout(CursorHide())
///@ AI-SideEffects: none
CursorHide =
  "\x1b[?25l"
=> :Str

///@ Purpose: Show the cursor
///@ Returns: Str
///@ Example:
///@   stdout(CursorShow())
///@ AI-SideEffects: none
CursorShow =
  "\x1b[?25h"
=> :Str

// -- Mouse Tracking (TM-3c) --

///@ Purpose: Enable mouse tracking (SGR 1006 + button/motion)
///@ Returns: Str
///@ Example:
///@   stdout(MouseTrackingEnter())
///@ AI-SideEffects: none
MouseTrackingEnter =
  "\x1b[?1000h\x1b[?1002h\x1b[?1006h"
=> :Str

///@ Purpose: Disable mouse tracking
///@ Returns: Str
///@ Example:
///@   stdout(MouseTrackingLeave())
///@ AI-SideEffects: none
MouseTrackingLeave =
  "\x1b[?1006l\x1b[?1002l\x1b[?1000l"
=> :Str

<<< @(ClearScreen, ClearLine, AltScreenEnter, AltScreenLeave, CursorMoveTo, CursorHide, CursorShow, MouseTrackingEnter, MouseTrackingLeave)
