// taida/terminal.td
//
// Taida-side facade for `taida-lang/terminal` v1.
//
// This file is the source of truth for the user-facing surface of the
// terminal package. The Rust addon (`src/lib.rs`, `src/key.rs`,
// `src/size.rs`, `src/tty.rs`) provides the implementations; this file pins the
// public names so a user import like:
//
//   >>> taida-lang/terminal => @(TerminalSize, ReadKey, KeyKind, IsTerminal)
//
// always sees the same surface symbols, regardless of which native
// build of the addon is loaded.
//
// ── How the facade is loaded ──────────────────────────────────────
//
// When the interpreter sees an addon-backed package import, it:
//   1. Loads the cdylib + verifies the ABI handshake.
//   2. Executes this file as a module in a fresh environment with
//      every `native/addon.toml::[functions]` entry pre-bound as an
//      addon sentinel (`terminalSize`, `readKey`, `isTerminal`).
//   3. Collects the facade's `<<<` exports and binds them into the
//      importer's scope.
//
// That means `TerminalSize <= terminalSize` copies the addon sentinel
// under an uppercase alias; the mold-call `TerminalSize[]()` in user
// code then routes to the Rust `terminalSize` function via the normal
// addon dispatch path. `KeyKind` is a pure-Taida pack; user code
// compares `key.kind` against `KeyKind.Char`, `KeyKind.ArrowUp`, etc.
//
// Renumbering, reordering, or adding `KeyKind` variants requires
// bumping the addon ABI. Phase 4 GO is the lock-in point.
//
// ── ReadKey return shape ──────────────────────────────────────────
//
//   ReadKey[]() => @(
//     kind  <= KeyKind:...   // Int tag, compare against KeyKind.*
//     text  <= ""            // Char's text, empty for non-Char
//     ctrl  <= false
//     alt   <= false
//     shift <= false
//   )
//
// ── TerminalSize return shape ─────────────────────────────────────
//
//   TerminalSize[]() => @(
//     cols  <= 1   // always >= 1 on success
//     rows  <= 1
//   )
//
// Field order is part of the v1 lock (cols then rows).

///@ Purpose: キー種別を表す列挙パック（28バリアント）
///@ AI-Context: ReadKey の戻り値 `kind` フィールドと比較して使う。
///@   タグ値（Int）は v1 ABI で凍結済み。追加・並び替えは ABI bump が必要。
///@ Example:
///@   key <= ReadKey[]()
///@   key.kind |== KeyKind.Enter => stdout("Enter pressed")
///@   key.kind |== KeyKind.Char  => stdout(key.text)
KeyKind <= @(
  Char       <= 0
  Enter      <= 1
  Escape     <= 2
  Tab        <= 3
  Backspace  <= 4
  Delete     <= 5
  ArrowUp    <= 6
  ArrowDown  <= 7
  ArrowLeft  <= 8
  ArrowRight <= 9
  Home       <= 10
  End        <= 11
  PageUp     <= 12
  PageDown   <= 13
  Insert     <= 14
  F1         <= 15
  F2         <= 16
  F3         <= 17
  F4         <= 18
  F5         <= 19
  F6         <= 20
  F7         <= 21
  F8         <= 22
  F9         <= 23
  F10        <= 24
  F11        <= 25
  F12        <= 26
  Unknown    <= 27
)

// Public-surface aliases for the Rust addon functions. The identifiers
// `terminalSize`, `readKey`, and `isTerminal` are pre-injected into this facade's
// environment by the interpreter (see
// `src/interpreter/module_eval.rs::load_addon_facade`) as addon
// sentinels, so the assignments below simply rename the sentinels
// under the uppercase names the RC2/TM design locks as the public surface.
///@ Purpose: ターミナルのカラム数・行数を取得する
///@ Returns: @(cols: Int, rows: Int) — 両方 >= 1
///@ Throws:
///@   TerminalSizeNotATty: stdout が TTY でない場合
///@   TerminalSizeIoctl: ioctl(TIOCGWINSZ) が失敗した場合
///@ Example:
///@   size <= TerminalSize[]()
///@   stdout(size.cols)
///@   stdout(size.rows)
///@ AI-SideEffects: ioctl システムコールを発行する（読み取り専用、副作用なし）
TerminalSize <= terminalSize

///@ Purpose: キーボードから1キー分の入力を読み取る（raw モード）
///@ Returns: @(kind: KeyKind, text: Str, ctrl: Bool, alt: Bool, shift: Bool)
///@ Throws:
///@   ReadKeyNotATty: stdin が TTY でない場合
///@   ReadKeyRawMode: raw モードの開始/終了に失敗した場合
///@   ReadKeyEof: EOF を検出した場合
///@   ReadKeyInterrupted: シグナル割り込みが発生した場合
///@ Example:
///@   key <= ReadKey[]()
///@   key.kind |== KeyKind.Escape => stdout("Escaped!")
///@ AI-SideEffects: stdin を一時的に raw モードに変更し、RAII で自動復元する
///@ AI-Context: ブロッキング呼び出し。1キー読み取り後に raw モードを解除して返る。
ReadKey      <= readKey

///@ Purpose: 指定ストリームが TTY かどうかを判定する
///@ Params:
///@   stream: "stdin" | "stdout" | "stderr"
///@ Returns: Bool
///@ Throws:
///@   IsTerminalInvalidStream: stream が "stdin" / "stdout" / "stderr" 以外の場合
///@ Example:
///@   interactive <= IsTerminal[]("stdin")
///@   stdout(interactive.toString())
///@ AI-SideEffects: `isatty` システムコールを発行する（読み取り専用、副作用なし）
IsTerminal   <= isTerminal

// ── Raw Mode (TM-2d) ────────────────────────────────────────────

///@ Purpose: stdin を raw モードに切り替える
///@ Returns: @() — 空パック
///@ Throws:
///@   RawModeNotATty: stdin が TTY でない場合
///@   RawModeAlreadyActive: 既に raw モードの場合（二重 enter 禁止）
///@   RawModeEnterFailed: termios 操作に失敗した場合
///@ Example:
///@   RawModeEnter[]()
///@   key <= ReadKey[]()
///@   RawModeLeave[]()
///@ AI-SideEffects: stdin の termios を変更する。RawModeLeave で復元必須。
///@ AI-Context: TUI アプリで RawModeEnter → ReadKey xN → RawModeLeave の
///@   パターンに使う。raw モード中の ReadKey は自身の enter/leave をスキップする。
RawModeEnter <= rawModeEnter

///@ Purpose: stdin を raw モードから復元する
///@ Returns: @() — 空パック
///@ Throws:
///@   RawModeNotActive: raw モードでない状態で呼んだ場合
///@   RawModeLeaveFailed: termios 復元に失敗した場合
///@ Example:
///@   RawModeEnter[]()
///@   key <= ReadKey[]()
///@   RawModeLeave[]()
///@ AI-SideEffects: stdin の termios を復元する
RawModeLeave <= rawModeLeave

// ── Screen / Cursor / Style facade imports (TM-2f/2g/2h) ────────
//
// These are pure Taida facades defined in ansi.td and style.td.
// They return ANSI escape strings — no side effects.

>>> ./ansi.td => @(ClearScreen, ClearLine, AltScreenEnter, AltScreenLeave, CursorMoveTo, CursorHide, CursorShow, MouseTrackingEnter, MouseTrackingLeave)

>>> ./style.td => @(Color, ResetStyle, Stylize, Color256, ColorRgb, Stylize256, StylizeRgb)

// ── Width / Renderer facade imports (TM-4a..4i) ─────────────────
//
// Pure Taida renderer foundation: Unicode width, virtual buffer, diff engine.

>>> ./width.td => @(WidthMode, MeasureGrapheme, DisplayWidth, NormalizeCellText, TruncateWidth, PadWidth)

>>> ./renderer.td => @(Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, BufferNew, BufferResize)

// ── Renderer native dispatch (Phase 8 / TMB-020) ────────────────
//
// The 8 hot-path renderer entries (`bufferPut` / `bufferWrite` /
// `bufferFillRect` / `bufferClear` / `bufferDiff` / `renderFull` /
// `renderFrame` / `renderOps`) are pre-injected as addon sentinels
// in this facade's env. We alias them under their public uppercase
// names so callers continue to see the `@a.4` surface unchanged.
// The pure-Taida types (`Cell` / `CellStyle` / `ScreenBuffer` /
// `DiffOpKind` / `DiffOp` / `BufferNew` / `BufferResize`) come from
// `renderer.td` above.

///@ Purpose: 単一セルを (col, row) に書き込む
///@ Returns: ScreenBuffer — 同じサイズの新パック
///@ Throws: RendererOutOfBounds — col<1 / row<1 / col>cols / row>rows
///@ AI-Context: 内部表現は Vec<Cell> に直接書き込むため O(1)。
BufferPut <= bufferPut

///@ Purpose: テキストを (col, row) から書き、表示幅で進める
///@ Params:
///@   buf:   ScreenBuffer
///@   col:   Int (1-based, 範囲内必須)
///@   row:   Int (1-based, 範囲内必須)
///@   text:  Str
///@   style: @(fg, bg, bold, dim, underline, italic) — 6 フィールド全て必要
///@ Returns: ScreenBuffer
///@ Throws: RendererOutOfBounds — 開始位置が範囲外
///@ Notes: 右端で truncate；wide char は 2 セル使用、2 セル目はスペース placeholder；
///@   width 0 grapheme (combining mark / control) はスキップ。
BufferWrite <= bufferWrite

///@ Purpose: 矩形領域を cell で塗りつぶす
///@ Throws: RendererOutOfBounds — col<1 / row<1。width<1 / height<1 は no-op。
BufferFillRect <= bufferFillRect

///@ Purpose: バッファ全体を fill cell で塗りつぶす
BufferClear <= bufferClear

///@ Purpose: 2 つのバッファ間の最小 diff 操作リストを生成する
///@ Returns: @(ops <= @[DiffOp...], requires_full <= Bool)
///@ AI-Context: requires_full=true は cols/rows が異なる場合。
///@   呼び出し側は RenderFull(next) にフォールバックする。
BufferDiff <= bufferDiff

///@ Purpose: バッファ全体を ANSI 文字列としてレンダリングする
///@ Returns: Str — CursorHide + 行毎 CursorMoveTo + cell text + ResetStyle + CursorMoveTo(cursor) + (visible なら CursorShow)
RenderFull <= renderFull

///@ Purpose: DiffOp リストを ANSI 文字列に変換する
///@ Returns: Str
RenderOps <= renderOps

///@ Purpose: prev / next の差分を最小 ANSI 出力として生成する
///@ Returns: @(text <= Str, next <= ScreenBuffer)
///@ AI-Context: requires_full なら RenderFull(next)、それ以外は RenderOps(diff.ops)。
RenderFrame <= renderFrame

///@ Purpose: sub バッファを main バッファの (col, row) 位置に合成する
///@ Params:
///@   main: ScreenBuffer — コピー先（結果はこの shape を保つ）
///@   sub:  ScreenBuffer — コピー元
///@   col:  Int (1-based) — main 内の左上 col。`sub` の (1,1) がここに lay-out される
///@   row:  Int (1-based) — main 内の左上 row
///@ Returns: ScreenBuffer — main と同じ cols/rows の新パック
///@ Throws: RendererOutOfBounds — col<1 / row<1
///@ Notes:
///@   - main からはみ出す sub のセルは silently clip（BufferFillRect と同じ規約）。
///@   - (col, row) が main の範囲外（右/下）を指すなら no-op（main をそのまま返す）。
///@   - wide char placeholder cell（text=" " の 2 セル目）は sub からそのまま運ばれる。
///@   - style 属性（fg/bg/bold/dim/underline/italic）はセル毎に保持される。
///@ AI-Context: TUI の pane 合成用プリミティブ。pure-Taida ループで sub を main に
///@   重ねると Taida の list index O(n) が効いて O(N²) になる（TMB-022）。
///@   この native 実装は Vec<Cell> 上の線形 copy で O(N) に抑える。
BufferBlit <= bufferBlit

// ── Event Types (TM-3a) ────────────────────────────────────────

///@ Purpose: イベント種別を表す列挙パック（4バリアント）
///@ AI-Context: ReadEvent の戻り値 `kind` フィールドと比較して使う。
///@ Example:
///@   event <= ReadEvent[]()
///@   event.kind |== EventKind.Key => stdout("Key event")
///@   event.kind |== EventKind.Mouse => stdout("Mouse event")
///@   event.kind |== EventKind.Resize => stdout("Resize event")
EventKind <= @(
  Key     <= 0
  Mouse   <= 1
  Resize  <= 2
  Unknown <= 3
)

///@ Purpose: マウスイベント種別を表す列挙パック（6バリアント）
///@ AI-Context: ReadEvent の戻り値 `mouse.kind` フィールドと比較して使う。
///@ Example:
///@   event <= ReadEvent[]()
///@   event.kind |== EventKind.Mouse =>
///@     event.mouse.kind |== MouseKind.Down => stdout("Click!")
MouseKind <= @(
  Down      <= 0
  Up        <= 1
  Move      <= 2
  Drag      <= 3
  ScrollUp  <= 4
  ScrollDown <= 5
)

// ── ReadEvent (TM-3d) ──────────────────────────────────────────

///@ Purpose: 統合イベントを1つ読み取る（キー / マウス / リサイズ）
///@ Returns: @(kind: EventKind, key: @(...), mouse: @(...), resize: @(...))
///@ Throws:
///@   ReadEventNotInRawMode: raw モードでない場合
///@   ReadEventNotATty: stdin が TTY でない場合
///@   ReadEventReadFailed: read(2) が失敗した場合
///@   ReadEventEof: stdin が閉じた場合
///@   ReadEventInterrupted: シグナル割り込みが発生した場合
///@ Example:
///@   RawModeEnter[]()
///@   stdout(MouseTrackingEnter[]())
///@   event <= ReadEvent[]()
///@   event.kind |== EventKind.Key => stdout(event.key.text)
///@   event.kind |== EventKind.Mouse => stdout("mouse at " + event.mouse.col.toString())
///@   event.kind |== EventKind.Resize => stdout("new size: " + event.resize.cols.toString())
///@   stdout(MouseTrackingLeave[]())
///@   RawModeLeave[]()
///@ AI-SideEffects: ブロッキング呼び出し。stdin + SIGWINCH を poll で多重化。
///@ AI-Context: raw モード必須。ReadKey の上位互換。
ReadEvent <= readEvent

// ── Write (TMB-016) ────────────────────────────────────────────

///@ Purpose: stdout に改行なしで即時書き出す（TUI 用）
///@ Params:
///@   bytes: Str — 書き出すバイト列（UTF-8）。ANSI エスケープを含んでよい。
///@ Returns: Int — 書き込んだバイト数
///@ Throws:
///@   WriteFailed: write_all / flush が I/O エラーで失敗した場合 (EPIPE 等)
///@   WriteBuildValue: 戻り値 Int のホスト側確保に失敗した場合
///@   WritePanic: write path 内で panic が発生した場合（FFI 境界で捕捉）
///@ Example:
///@   Write[]("\x1b[2J\x1b[H")          // clear + home cursor
///@   Write[](CursorMoveTo[](10, 5))    // カーソル移動（改行なし）
///@   n <= Write[]("hello")             // n == 5
///@ AI-SideEffects: stdout に即時書き出す（flush 付き）。改行の暗黙追加は行わない。
///@ AI-Context: `stdout()` builtin は push 単位で `\n` を暗黙追加する行指向 I/O のため、
///@   ANSI エスケープを連続送信する TUI 用途にはこの Write[]() を使う。
///@   non-TTY (pipe / redirect) でも panic せず動作する（成功経路）。
Write <= write

// ── Prompt / Line Editor facade imports (TM-5a..5f) ─────────────
//
// Pure Taida line editor state machine: event in, state out.
// No blocking I/O — users compose their own ReadEvent loop.

>>> ./prompt.td => @(PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, LineEditorNew, LineEditorStep, LineEditorRender)

// ── UX Widgets facade imports (TM-5g..5h) ───────────────────────
//
// Pure Taida widgets: spinner, progress bar, status line.
// All return Str or pure state — no side effects.

>>> ./widgets.td => @(SpinnerState, SpinnerNext, SpinnerRender, ProgressOptions, ProgressBar, StatusLine)

<<< @(TerminalSize, ReadKey, KeyKind, IsTerminal, RawModeEnter, RawModeLeave, ClearScreen, ClearLine, AltScreenEnter, AltScreenLeave, CursorMoveTo, CursorHide, CursorShow, Stylize, Color, ResetStyle, Color256, ColorRgb, Stylize256, StylizeRgb, EventKind, MouseKind, ReadEvent, MouseTrackingEnter, MouseTrackingLeave, Write, WidthMode, MeasureGrapheme, DisplayWidth, NormalizeCellText, TruncateWidth, PadWidth, Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, BufferNew, BufferResize, BufferClear, BufferPut, BufferWrite, BufferFillRect, BufferBlit, RenderFull, BufferDiff, RenderOps, RenderFrame, PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, LineEditorNew, LineEditorStep, LineEditorRender, SpinnerState, SpinnerNext, SpinnerRender, ProgressOptions, ProgressBar, StatusLine)
