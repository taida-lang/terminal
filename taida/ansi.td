// taida/ansi.td
//
// ANSI escape sequence helpers for screen and cursor control.
//
// All functions in this file return pure `Str` values containing ANSI
// escape sequences. They do NOT write to stdout -- the caller decides
// when to write via `stdout(...)`.
//
// This is the pure ANSI control facade:
//   - No syscalls
//   - No hidden side effects
//   - Returns ANSI escape strings only
//
// Public functions are exported as camelCase names.
//
// -- Screen Control --

///@ Purpose: Clear the entire screen and move cursor to (1,1)
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[2J ESC[H`
///@ Example:
///@   stdout(clearScreen())
///@ AI-Context: 純粋関数。ANSI 文字列を返すだけで、stdout への書き出しは
///@   呼び出し側が行う。TUI フレーム描画ではなく、CLI 起動時のクリアに使う。
///@ AI-SideEffects: none
clearScreen =
  "\x1b[2J\x1b[H"
=> :Str

///@ Purpose: Clear current line and move cursor to beginning
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[2K \r`
///@ Example:
///@   stdout(clearLine())
///@ AI-Context: 行頭に戻る `\r` を含む。プロンプト再描画パターンで多用。
///@   `lineEditorRender` 戻り値の line フィールドは既にこの prefix を含む。
///@ AI-SideEffects: none
clearLine =
  "\x1b[2K\r"
=> :Str

///@ Purpose: Switch to alternate screen buffer
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[?1049h`
///@ Example:
///@   stdout(altScreenEnter())
///@   // ... TUI rendering ...
///@   stdout(altScreenLeave())
///@ AI-Context: TUI の標準パターンの 1 つ。enter / leave は対で呼ぶ。
///@   leave なしで終了するとターミナルにゴミが残る。
///@ AI-SideEffects: none (ANSI string 返却のみ; 副作用は stdout 書き出し時)
altScreenEnter =
  "\x1b[?1049h"
=> :Str

///@ Purpose: Switch back to main screen buffer
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[?1049l`
///@ Example:
///@   stdout(altScreenLeave())
///@ AI-Context: `altScreenEnter` と対で必ず呼ぶ。プログラム終了時の
///@   cleanup hook (e.g. signal handler) で出力するのが安全。
///@ AI-SideEffects: none
altScreenLeave =
  "\x1b[?1049l"
=> :Str

// -- Cursor Control --

///@ Purpose: Move cursor to (col, row) position (1-based)
///@ Params:
///@   - col: Int -- 1-based column (>= 1; col < 1 throws CursorMoveInvalidPosition)
///@   - row: Int -- 1-based row (>= 1; row < 1 throws CursorMoveInvalidPosition)
///@ Returns: Str — ANSI escape sequence `ESC[<row>;<col>H`
///@ Throws:
///@   - CursorMoveInvalidPosition: col < 1 or row < 1
///@ Example:
///@   stdout(cursorMoveTo(10, 5))   // col=10, row=5
///@ AI-Context: ANSI は (row;col) 順だが、本 facade は (col, row) 順で受ける
///@   (renderer の cursor_col / cursor_row 順序と一致)。1-based に注意。
///@ AI-Hint: TUI 描画ループでは renderFrame が cursor 移動 ANSI を内部で
///@   生成するので、この関数を frame 描画中に直接呼ぶ必要はない。
///@ AI-SideEffects: none
cursorMoveTo col row =
  | col < 1 |> Error(type <= "CursorMoveInvalidPosition", message <= "col must be >= 1").throw()
  | row < 1 |> Error(type <= "CursorMoveInvalidPosition", message <= "row must be >= 1").throw()
  | _ |> "\x1b[" + row.toString() + ";" + col.toString() + "H"
=> :Str

///@ Purpose: Hide the cursor
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[?25l`
///@ Example:
///@   stdout(cursorHide())
///@ AI-Context: TUI 描画中のカーソル点滅を抑制したいときに使う。
///@   プログラム終了時には cursorShow を必ず呼んで復元する。
///@ AI-SideEffects: none
cursorHide =
  "\x1b[?25l"
=> :Str

///@ Purpose: Show the cursor
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[?25h`
///@ Example:
///@   stdout(cursorShow())
///@ AI-Context: cursorHide と対で呼ぶ。終了 cleanup hook での明示が望ましい。
///@ AI-SideEffects: none
cursorShow =
  "\x1b[?25h"
=> :Str

// -- Mouse Tracking --

///@ Purpose: Enable mouse tracking (SGR 1006 + button/motion)
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[?1000h ESC[?1002h ESC[?1006h`
///@ Example:
///@   rawModeEnter()
///@   stdout(mouseTrackingEnter())
///@   event <= readEvent()  // event.kind == EventKind.mouse on click
///@   stdout(mouseTrackingLeave())
///@   rawModeLeave()
///@ AI-Context: 3 つの ANSI シーケンスを連結: ?1000h (button report),
///@   ?1002h (button + motion report), ?1006h (SGR extended pixel coords)。
///@   `readEvent` でマウスイベントを受け取るために raw mode と併用する。
///@ AI-Hint: enter / leave は必ず対で。leave 忘れるとシェルにマウス escape が
///@   ダダ漏れになる。
///@ AI-SideEffects: none
mouseTrackingEnter =
  "\x1b[?1000h\x1b[?1002h\x1b[?1006h"
=> :Str

///@ Purpose: Disable mouse tracking
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence (mouseTrackingEnter の逆順 leave)
///@ Example:
///@   stdout(mouseTrackingLeave())
///@ AI-Context: enter で有効化した 3 モードをすべて解除。順序は逆。
///@ AI-SideEffects: none
mouseTrackingLeave =
  "\x1b[?1006l\x1b[?1002l\x1b[?1000l"
=> :Str

<<< @(clearScreen, clearLine, altScreenEnter, altScreenLeave, cursorMoveTo, cursorHide, cursorShow, mouseTrackingEnter, mouseTrackingLeave)
