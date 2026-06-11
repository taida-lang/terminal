// taida/terminal.td
//
// Taida-side facade for `taida-lang/terminal` v1.
//
// This file is the source of truth for the user-facing surface of the
// terminal package. The Rust addon (`src/lib.rs`, `src/key.rs`,
// `src/size.rs`, `src/tty.rs`) provides the implementations; this file pins the
// public names so a user import like:
//
//   >>> taida-lang/terminal => @(terminalSize, readKey, KeyKind, isTerminal)
//
// always sees the same surface symbols, regardless of which native
// build of the addon is loaded.
//
// ── Current public naming surface ────────────────────────────────
//
// Current Taida package naming:
//   * 関数: PascalCase alias 廃止 — `BufferWrite <= bufferWrite` 等の
//     uppercase alias を削除し、camelCase の addon sentinel を直接 export。
//     ユーザコードは `bufferWrite(...)` で呼ぶ。
//   * variant: ぶちパック非関数値フィールドを snake_case 化。
//     例: `KeyKind.Char` → `KeyKind.char`、`MouseKind.Up` → `MouseKind.up`。
//     Rust 側 `src/key.rs` の Int tag 対応は不変。
//   * 型 (PascalCase 維持): `Cell` / `CellStyle` / `ScreenBuffer` /
//     `DiffOp` / `KeyKind` / `EventKind` / `MouseKind` / `WidthMode` /
//     `PromptMode` / `LineEditorAction` 等のぶちパック型自体。
//
// ── How the facade is loaded ──────────────────────────────────────
//
// When the interpreter sees an addon-backed package import, it:
//   1. Loads the cdylib + verifies the ABI handshake.
//   2. Executes this file as a module in a fresh environment with
//      every `native/addon.toml::[functions]` entry pre-bound as an
//      addon sentinel (`terminalSize`, `readKey`, `isTerminal`, ...).
//   3. Collects the facade's `<<<` exports and binds them into the
//      importer's scope.
//
// `KeyKind` is a pure-Taida pack; user code compares `key.kind` against
// `KeyKind.char`, `KeyKind.arrow_up`, etc. Renumbering, reordering, or
// adding `KeyKind` variants requires bumping the addon ABI.
//
// ── readKey return shape ──────────────────────────────────────────
//
//   readKey() => @(
//     kind  <= KeyKind:...   // Int tag, compare against KeyKind.*
//     text  <= ""            // Char's text, empty for non-Char
//     ctrl  <= false
//     alt   <= false
//     shift <= false
//   )
//
// ── terminalSize return shape ─────────────────────────────────────
//
//   terminalSize() => @(
//     cols  <= 1   // always >= 1 on success
//     rows  <= 1
//   )
//
// Field order is part of the v1 lock (cols then rows).

///@ Purpose: キー種別を表す列挙パック（28バリアント）
///@ Params:
///@   - char:        Int = 0  — 印字可能文字 (text フィールドに本体)
///@   - enter:       Int = 1  — Enter / Return
///@   - escape:      Int = 2  — Escape (ESC)
///@   - tab:         Int = 3  — Tab
///@   - backspace:   Int = 4  — Backspace
///@   - delete:      Int = 5  — Delete (forward)
///@   - arrow_up/down/left/right: Int = 6/7/8/9 — 矢印キー
///@   - home / end:  Int = 10 / 11 — 行頭 / 行末ジャンプ
///@   - page_up/down: Int = 12 / 13 — ページスクロール
///@   - insert:      Int = 14 — Insert
///@   - f1..f12:     Int = 15..26 — ファンクションキー
///@   - unknown:     Int = 27 — 未認識キー
///@ Returns: KeyKind ぶちパック (28 variant)
///@ AI-Context: readKey / readEvent.key の戻り値 `kind` フィールドと比較。
///@   タグ値（Int）は v1 ABI で凍結済み。追加・並び替えは ABI bump が必要。
///@   variant は snake_case (`char`, `enter`, `arrow_up` 等)。
///@   Rust 側 `src/key.rs` の Int tag 対応は不変。
///@ AI-Hint: ctrl/alt/shift 修飾キーは別フィールド (key.ctrl 等) で表現する。
///@   `Ctrl+C` は `key.kind == KeyKind.char && key.text == "c" && key.ctrl == true`。
///@ Example:
///@   key <= readKey()
///@   key.kind |== KeyKind.enter => stdout("Enter pressed")
///@   key.kind |== KeyKind.char  => stdout(key.text)
///@   key.kind |== KeyKind.f5    => stdout("F5 hit")
KeyKind <= @(
  char        <= 0
  enter       <= 1
  escape      <= 2
  tab         <= 3
  backspace   <= 4
  delete      <= 5
  arrow_up    <= 6
  arrow_down  <= 7
  arrow_left  <= 8
  arrow_right <= 9
  home        <= 10
  end         <= 11
  page_up     <= 12
  page_down   <= 13
  insert      <= 14
  f1          <= 15
  f2          <= 16
  f3          <= 17
  f4          <= 18
  f5          <= 19
  f6          <= 20
  f7          <= 21
  f8          <= 22
  f9          <= 23
  f10         <= 24
  f11         <= 25
  f12         <= 26
  unknown     <= 27
)

// Public-surface aliases for the Rust addon functions. The identifiers
// `terminalSize`, `readKey`, `isTerminal`, etc. are pre-injected into
// this facade's environment by the interpreter (see
// `src/interpreter/module.rs::load_addon_facade`) as addon
// sentinels. We export the camelCase sentinels directly.

// ── Screen / Cursor / Style facade imports ──────────────────────
//
// These are pure Taida facades defined in ansi.td and style.td.
// They return ANSI escape strings — no side effects.

>>> ./ansi.td => @(clearScreen, clearLine, altScreenEnter, altScreenLeave, cursorMoveTo, cursorHide, cursorShow, mouseTrackingEnter, mouseTrackingLeave)

>>> ./style.td => @(Color, resetStyle, stylize, Color256, ColorRgb, stylize256, stylizeRgb)

// ── Width / Renderer facade imports ─────────────────────────────
//
// Pure Taida renderer foundation: Unicode width, virtual buffer, diff engine.

>>> ./width.td => @(WidthMode)

// ── Width native dispatch ───────────────────────────────────────
//
// The 5 width helpers (`measureGrapheme` / `displayWidth` /
// `normalizeCellText` / `truncateWidth` / `padWidth`) are pre-injected
// as addon sentinels in this facade's env, so they are already bound
// at the package level — no PascalCase alias needed.
// Sub-modules (`widgets.td`, `prompt.td`) that `>>>` width.td directly
// still pick up the pure-Taida versions because they don't get addon
// sentinels injected; typical sub-module inputs are short (< 200 chars)
// so the O(N²) fallback is bounded by the frame budget.

>>> ./renderer.td => @(Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp)

// ── Renderer native dispatch ────────────────────────────────────
//
// The hot-path renderer entries are pre-injected as addon sentinels
// in this facade's env. The camelCase
// sentinels (`bufferPut` / `bufferWrite` / `bufferFillRect` /
// `bufferClear` / `bufferDiff` / `renderFull` / `renderFrame` /
// `renderOps` / `bufferBlit` / `bufferNew` / `bufferResize`) are
// exported directly without PascalCase aliases.

// ── Event Types ─────────────────────────────────────────────────

///@ Purpose: イベント種別を表す列挙パック（4バリアント）
///@ Params:
///@   - key:     Int = 0 — キーボードイベント (key フィールドが有効)
///@   - mouse:   Int = 1 — マウスイベント (mouse フィールドが有効)
///@   - resize:  Int = 2 — ターミナルリサイズ (resize フィールドが有効)
///@   - unknown: Int = 3 — 未認識 / 解釈不能
///@ Returns: EventKind ぶちパック (4 variant)
///@ AI-Context: readEvent の戻り値 `kind` フィールドと比較して使う。
///@   v1 ABI で凍結 (Int tag 不変)。
///@   variant は snake_case。
///@ AI-Hint: event.kind に応じて event.key / event.mouse / event.resize の
///@   どのフィールドを読むか分岐する。kind=unknown のときは何も読まない。
///@ Example:
///@   event <= readEvent()
///@   event.kind |== EventKind.key => stdout("Key event")
///@   event.kind |== EventKind.mouse => stdout("Mouse event")
///@   event.kind |== EventKind.resize => stdout("Resize event: " + event.resize.cols.toString())
EventKind <= @(
  key     <= 0
  mouse   <= 1
  resize  <= 2
  unknown <= 3
)

///@ Purpose: マウスイベント種別を表す列挙パック（6バリアント）
///@ Params:
///@   - down:        Int = 0 — ボタン押下
///@   - up:          Int = 1 — ボタン離上
///@   - move:        Int = 2 — マウス移動 (ボタン押下なし)
///@   - drag:        Int = 3 — ドラッグ (ボタン押下中の移動)
///@   - scroll_up:   Int = 4 — ホイール上
///@   - scroll_down: Int = 5 — ホイール下
///@ Returns: MouseKind ぶちパック (6 variant)
///@ AI-Context: readEvent の戻り値 `mouse.kind` フィールドと比較して使う。
///@   v1 ABI で凍結 (Int tag 不変)。
///@   variant は snake_case。
///@ AI-Hint: マウストラッキングを有効化していない (mouseTrackingEnter 未呼出) 場合、
///@   readEvent はマウスイベントを emit しない。
///@ Example:
///@   event <= readEvent()
///@   event.kind |== EventKind.mouse =>
///@     event.mouse.kind |== MouseKind.down => stdout("Click at " + event.mouse.col.toString())
///@     event.mouse.kind |== MouseKind.scroll_up => stdout("Scroll up")
MouseKind <= @(
  down        <= 0
  up          <= 1
  move        <= 2
  drag        <= 3
  scroll_up   <= 4
  scroll_down <= 5
)

// ── Prompt / Line Editor facade imports ─────────────────────────

>>> ./prompt.td => @(PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, lineEditorNew, lineEditorStep, lineEditorRender)

// ── UX Widgets facade imports ───────────────────────────────────

>>> ./widgets.td => @(SpinnerState, spinnerNext, spinnerRender, ProgressOptions, progressBar, statusLine)

// ── Explicit RustAddon[...] bindings ────────────────────────────
//
// Each addon-backed public function is declared in the Taida facade so
// documentation, LSP, graph extraction, and runtime dispatch observe the
// same surface. Names and arities must match native/addon.toml.

///@ Purpose: 現在の stdout ターミナルサイズをセル数で取得する
///@ Returns: @(cols: Int, rows: Int) — 成功時は両方 1 以上
///@ Throws:
///@   TerminalSizeNotATty: stdout が TTY ではない場合
///@   TerminalSizeIoctl: Unix ioctl または Windows console size query が失敗した場合
///@ Example:
///@   size <= terminalSize()
///@   cols <= size.cols
///@   rows <= size.rows
///@ AI-Context: レイアウト計算の入口。呼び出しごとに OS の現在値を読むため、resize 後は再取得する。
///@ AI-SideEffects: stdout の端末サイズを問い合わせるだけで、端末状態は変更しない。
terminalSize       <= RustAddon["terminalSize"](arity <= 0)

///@ Purpose: stdin から 1 キー分の入力を読み取り、KeyKind ベースのパックで返す
///@ Returns: @(kind: Int, text: Str, ctrl: Bool, alt: Bool, shift: Bool) — kind は KeyKind と比較する
///@ Throws:
///@   ReadKeyNotATty: stdin が TTY ではない場合
///@   ReadKeyRawMode: raw mode の開始または復元に失敗した場合
///@   ReadKeyEof: EOF を検出した場合
///@   ReadKeyInterrupted: シグナル割り込みが発生した場合
///@ Example:
///@   key <= readKey()
///@   key.kind |== KeyKind.escape => write("cancel")
///@   key.kind |== KeyKind.char => write(key.text)
///@ AI-Context: 単発入力向け。イベントループで mouse / resize も扱う場合は rawModeEnter 後に readEvent を使う。
///@ AI-SideEffects: 必要に応じて stdin を一時的に raw mode にし、読み取り後に復元する。
readKey            <= RustAddon["readKey"](arity <= 0)

///@ Purpose: 指定ストリームが TTY かどうかを判定する
///@ Params:
///@   - stream: 判定対象。値は "stdin" / "stdout" / "stderr" のいずれか
///@ Returns: Bool — 対象が TTY なら true
///@ Throws:
///@   IsTerminalInvalidStream: stream が許可値以外の場合
///@ Example:
///@   interactive <= isTerminal("stdin")
///@   canPaint <= isTerminal("stdout")
///@ AI-Context: CI や pipe 実行時の TUI fallback 判定に使う。未知 stream 名は false ではなく診断として扱う。
///@ AI-SideEffects: TTY 判定の OS API を呼ぶ。端末状態は変更しない。
isTerminal         <= RustAddon["isTerminal"](arity <= 1)

///@ Purpose: stdin を persistent raw mode に切り替え、後続の readEvent / readKey ループを可能にする
///@ Returns: @(active: Bool) — 成功後は active=true
///@ Throws:
///@   RawModeNotATty: stdin が TTY ではない場合
///@   RawModeAlreadyActive: 既に raw mode の場合
///@   RawModeEnterFailed: raw mode への切替に失敗した場合
///@ Example:
///@   entered <= rawModeEnter()
///@   event <= readEvent()
///@   left <= rawModeLeave()
///@ AI-Hint: rawModeEnter と rawModeLeave は必ず対にする。長時間 raw mode を保持する TUI では leave を終了処理に置く。
///@ AI-SideEffects: stdin の端末設定を変更する。rawModeLeave で復元する。
rawModeEnter       <= RustAddon["rawModeEnter"](arity <= 0)

///@ Purpose: rawModeEnter が保存した stdin の設定を復元する
///@ Returns: @(active: Bool) — 成功後は active=false
///@ Throws:
///@   RawModeNotActive: raw mode でない状態で呼んだ場合
///@   RawModeLeaveFailed: 端末設定の復元に失敗した場合
///@ Example:
///@   entered <= rawModeEnter()
///@   key <= readKey()
///@   left <= rawModeLeave()
///@ AI-Context: raw mode が有効でない場合は成功扱いにしない。呼び出し側は enter / leave の所有権を明確にする。
///@ AI-SideEffects: rawModeEnter が保存した stdin の端末設定を復元する。
rawModeLeave       <= RustAddon["rawModeLeave"](arity <= 0)

///@ Purpose: raw mode 中のキーボード・マウス・リサイズイベントを 1 件読み取る
///@ Returns: @(kind: Int, key: @(...), mouse: @(...), resize: @(cols: Int, rows: Int)) — kind は EventKind と比較する
///@ Throws:
///@   ReadEventNotInRawMode: raw mode ではない状態で呼んだ場合
///@   ReadEventNotATty: stdin が TTY ではない場合
///@   ReadEventReadFailed: 入力読み取りに失敗した場合
///@   ReadEventEof: EOF を検出した場合
///@   ReadEventInterrupted: シグナル割り込みが発生した場合
///@ Example:
///@   entered <= rawModeEnter()
///@   event <= readEvent()
///@   event.kind |== EventKind.key => write(event.key.text)
///@   left <= rawModeLeave()
///@ AI-Context: 同じ stdin stream は 1 つの専用 blocking thread から読む。pending byte queue は thread-local。
///@ AI-Hint: mouse event を受けたい場合は rawModeEnter 後に mouseTrackingEnter の文字列を write する。
///@ AI-SideEffects: stdin からブロッキング読み取りを行い、SIGWINCH / console resize 状態も観測する。
readEvent          <= RustAddon["readEvent"](arity <= 0)

///@ Purpose: stdout に文字列を改行なしで即時書き出す
///@ Params:
///@   - bytes: 書き出す UTF-8 文字列。ANSI escape sequence を含んでよい
///@ Returns: Int — 書き込んだ UTF-8 byte 数
///@ Throws:
///@   WriteFailed: stdout write / flush に失敗した場合
///@   WriteBuildValue: 戻り値 Int のホスト側確保に失敗した場合
///@   WritePanic: write path 内で panic が発生した場合
///@ Example:
///@   n <= write("hello")
///@   moved <= write(cursorMoveTo(1, 1))
///@ AI-Context: TUI の paint path では stdout ではなく write を使う。改行や flush policy を呼び出し側が制御できる。
///@ AI-SideEffects: stdout に書き込み、flush する。改行は自動追加しない。
write              <= RustAddon["write"](arity <= 1)

///@ Purpose: ScreenBuffer の指定セルを 1 つ置き換えた新しい buffer を返す
///@ Params:
///@   - buf: 更新元 ScreenBuffer
///@   - col: 1-based column。1 以上かつ buf.cols 以下
///@   - row: 1-based row。1 以上かつ buf.rows 以下
///@   - cell: 書き込む Cell
///@ Returns: ScreenBuffer — buf と同じ cols / rows を持つ新しい buffer
///@ Throws:
///@   RendererOutOfBounds: 座標が buffer 外の場合
///@   RendererInvalidArg: buf または cell の shape が不正な場合
///@ Example:
///@   next <= bufferPut(buf, 1, 1, cell)
///@ AI-Context: buffer は immutable に扱う。戻り値を次フレームの状態として保持する。
bufferPut          <= RustAddon["bufferPut"](arity <= 4)

///@ Purpose: ScreenBuffer の指定位置から文字列を書き込み、表示幅に応じてカーソルを進める
///@ Params:
///@   - buf: 更新元 ScreenBuffer
///@   - col: 1-based column。開始位置は buffer 内
///@   - row: 1-based row。開始位置は buffer 内
///@   - text: 書き込む文字列。wide character は 2 cell 幅として扱う
///@   - style: text 全体に適用する CellStyle
///@ Returns: ScreenBuffer — 書き込み範囲だけが変わった新しい buffer
///@ Throws:
///@   RendererInvalidArg: buffer / style shape が不正な場合
///@   RendererOutOfBounds: 開始座標が buffer 外の場合
///@ Example:
///@   next <= bufferWrite(buf, 2, 1, "title", style)
///@ AI-Hint: text が右端を超える部分は buffer 外に書かない。折り返しは呼び出し側で行う。
bufferWrite        <= RustAddon["bufferWrite"](arity <= 5)

///@ Purpose: ScreenBuffer の矩形範囲を同じ Cell で埋める
///@ Params:
///@   - buf: 更新元 ScreenBuffer
///@   - col: 矩形左上の 1-based column
///@   - row: 矩形左上の 1-based row
///@   - width: 矩形幅。1 未満なら no-op
///@   - height: 矩形高さ。1 未満なら no-op
///@   - cell: 埋め込みに使う Cell
///@ Returns: ScreenBuffer — 指定矩形だけが変わった新しい buffer
///@ Throws:
///@   RendererInvalidArg: buf または cell の shape が不正な場合
///@   RendererOutOfBounds: col / row が 1 未満の場合
///@ Example:
///@   panel <= bufferFillRect(buf, 1, 1, 20, 3, cell)
///@ AI-Hint: buffer 外にはみ出す右端・下端は切り詰めて塗る。
bufferFillRect     <= RustAddon["bufferFillRect"](arity <= 6)

///@ Purpose: ScreenBuffer 全体を指定 Cell で塗り直す
///@ Params:
///@   - buf: 更新元 ScreenBuffer
///@   - fill: 全セルに入れる Cell
///@ Returns: ScreenBuffer — cols / rows を保ち、cells が fill で埋まった buffer
///@ Throws:
///@   RendererInvalidArg: buf または fill の shape が不正な場合
///@ Example:
///@   cleared <= bufferClear(buf, blank)
///@ AI-Context: frame 開始時の full repaint 準備に使う。既存 buffer は変更しない。
bufferClear        <= RustAddon["bufferClear"](arity <= 2)

///@ Purpose: 2 つの ScreenBuffer の差分操作列を作る
///@ Params:
///@   - prev: 直前に描画した ScreenBuffer
///@   - next: 次に描画したい ScreenBuffer
///@ Returns: @(ops: @[DiffOp], requires_full: Bool) — サイズ差異がある場合は requires_full=true
///@ Throws:
///@   RendererInvalidArg: prev または next の shape が不正な場合
///@ Example:
///@   diff <= bufferDiff(prev, next)
///@   text <= renderOps(diff.ops)
///@ AI-Context: renderFrame の下位 API。自前で paint scheduling する場合だけ直接使う。
bufferDiff         <= RustAddon["bufferDiff"](arity <= 2)

///@ Purpose: ScreenBuffer 全体を ANSI 文字列として描画する
///@ Params:
///@   - buf: 描画対象 ScreenBuffer
///@ Returns: Str — full repaint 用 ANSI escape sequence
///@ Throws:
///@   RendererInvalidArg: buf の shape が不正な場合
///@ Example:
///@   text <= renderFull(buf)
///@   bytes <= write(text)
///@ AI-Context: サイズ変更後や初回描画では renderFull を使う。通常 frame は renderFrame が差分を選ぶ。
renderFull         <= RustAddon["renderFull"](arity <= 1)

///@ Purpose: prev から next への redraw frame を作り、次の基準 buffer を返す
///@ Params:
///@   - prev: 直前に描画した ScreenBuffer
///@   - next: 次に描画したい ScreenBuffer
///@ Returns: @(text: Str, next: ScreenBuffer) — text を write し、戻り値 next を次回 prev にする
///@ Throws:
///@   RendererInvalidArg: prev または next の shape が不正な場合
///@ Example:
///@   frame <= renderFrame(prev, next)
///@   bytes <= write(frame.text)
///@   prev <= frame.next
///@ AI-Hint: 同一サイズなら diff、サイズ差異があれば full repaint に fallback する。
renderFrame        <= RustAddon["renderFrame"](arity <= 2)

///@ Purpose: DiffOp リストを ANSI 文字列に変換する
///@ Params:
///@   - ops: bufferDiff が返した差分操作列
///@ Returns: Str — cursor movement と style reset を含む差分描画文字列
///@ Throws:
///@   RendererInvalidArg: ops の shape が不正な場合
///@ Example:
///@   diff <= bufferDiff(prev, next)
///@   text <= renderOps(diff.ops)
///@ AI-Context: bufferDiff と renderOps を分けて instrumentation したい場合に使う。
renderOps          <= RustAddon["renderOps"](arity <= 1)

///@ Purpose: sub buffer を main buffer の指定位置へ合成する
///@ Params:
///@   - main: 合成先 ScreenBuffer。戻り値はこのサイズを保つ
///@   - sub: 合成元 ScreenBuffer
///@   - col: main 内の左上 1-based column
///@   - row: main 内の左上 1-based row
///@ Returns: ScreenBuffer — sub が main 上に重なった新しい buffer
///@ Throws:
///@   RendererInvalidArg: main または sub の shape が不正な場合
///@   RendererOutOfBounds: col / row が 1 未満の場合
///@ Example:
///@   composed <= bufferBlit(main, dialog, 10, 4)
///@ AI-Hint: main 外にはみ出す右端・下端は切り詰める。透明セルの概念はない。
bufferBlit         <= RustAddon["bufferBlit"](arity <= 4)

///@ Purpose: 新しい ScreenBuffer を指定サイズで作る
///@ Params:
///@   - cols: column count。1 以上
///@   - rows: row count。1 以上
///@ Returns: ScreenBuffer — default Cell で初期化された row-major buffer
///@ Throws:
///@   RendererInvalidSize: cols または rows が 1 未満の場合
///@ Example:
///@   buf <= bufferNew(80, 24)
///@ AI-Context: terminalSize の戻り値を渡すのが通常の初期化 path。
bufferNew          <= RustAddon["bufferNew"](arity <= 2)

///@ Purpose: ScreenBuffer を指定サイズへ resize した新しい buffer を返す
///@ Params:
///@   - buf: resize 元 ScreenBuffer
///@   - cols: 新しい column count。1 以上
///@   - rows: 新しい row count。1 以上
///@   - fill: 新規領域に使う Cell
///@ Returns: ScreenBuffer — 指定サイズの buffer
///@ Throws:
///@   RendererInvalidSize: cols または rows が 1 未満の場合
///@   RendererInvalidArg: buf または fill の shape が不正な場合
///@ Example:
///@   resized <= bufferResize(prev, size.cols, size.rows, blank)
///@ AI-Context: resize event 後に renderFrame へ渡す next buffer を作る。既存内容の保持範囲は実装契約に従う。
bufferResize       <= RustAddon["bufferResize"](arity <= 4)

///@ Purpose: 文字列先頭の grapheme 表示幅と分類を測定する
///@ Params:
///@   - text: 測定対象文字列。先頭 grapheme だけを評価する
///@ Returns: @(width: Int, mode: Int) — mode は WidthMode.narrow / wide / zero と比較する
///@ Example:
///@   g <= measureGrapheme("漢A")
///@   w <= g.width
///@ AI-Context: renderer が wide character のセル消費を決めるための低レベル API。
measureGrapheme    <= RustAddon["measureGrapheme"](arity <= 1)

///@ Purpose: 文字列全体の表示幅を terminal cell 数で返す
///@ Params:
///@   - text: 測定対象文字列
///@ Returns: Int — 結合文字と制御文字は 0、wide / fullwidth は 2、それ以外は 1 として合計する
///@ Example:
///@   width <= displayWidth("A漢")
///@ AI-Context: column alignment / truncation / progress bar 幅計算に使う。
displayWidth       <= RustAddon["displayWidth"](arity <= 1)

///@ Purpose: renderer cell に入れる文字列を 1 cell 表示向けに正規化する
///@ Params:
///@   - text: 正規化対象文字列
///@ Returns: Str — 改行を除去し、空文字列は空白 1 文字へ正規化する
///@ Example:
///@   cellText <= normalizeCellText(input)
///@ AI-Context: Cell.text に入れる前の sanitize API。複数 cell 幅の処理は renderer が扱う。
normalizeCellText  <= RustAddon["normalizeCellText"](arity <= 1)

///@ Purpose: 表示幅が width 以内に収まる prefix へ切り詰める
///@ Params:
///@   - text: 対象文字列
///@   - width: 最大表示幅。1 未満なら空文字列を返す
///@ Returns: Str — wide character を途中で割らない prefix
///@ Example:
///@   clipped <= truncateWidth(label, 12)
///@ AI-Hint: 省略記号は追加しない。必要なら呼び出し側で幅を確保してから連結する。
truncateWidth      <= RustAddon["truncateWidth"](arity <= 2)

///@ Purpose: 表示幅が width になるまで右側に空白を追加する
///@ Params:
///@   - text: 対象文字列
///@   - width: 目標表示幅
///@ Returns: Str — 既に width 以上なら text をそのまま返す
///@ Example:
///@   padded <= padWidth(name, 20)
///@ AI-Context: table / status line の列揃えに使う。左 padding は提供しない。
padWidth           <= RustAddon["padWidth"](arity <= 2)

<<< @(terminalSize, readKey, KeyKind, isTerminal, rawModeEnter, rawModeLeave, clearScreen, clearLine, altScreenEnter, altScreenLeave, cursorMoveTo, cursorHide, cursorShow, stylize, Color, resetStyle, Color256, ColorRgb, stylize256, stylizeRgb, EventKind, MouseKind, readEvent, mouseTrackingEnter, mouseTrackingLeave, write, WidthMode, measureGrapheme, displayWidth, normalizeCellText, truncateWidth, padWidth, Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, bufferNew, bufferResize, bufferClear, bufferPut, bufferWrite, bufferFillRect, bufferBlit, renderFull, bufferDiff, renderOps, renderFrame, PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, lineEditorNew, lineEditorStep, lineEditorRender, SpinnerState, spinnerNext, spinnerRender, ProgressOptions, progressBar, statusLine)
