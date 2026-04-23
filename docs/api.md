# Module: ansi.td

## Exports

- `ClearScreen`
- `ClearLine`
- `AltScreenEnter`
- `AltScreenLeave`
- `CursorMoveTo`
- `CursorHide`
- `CursorShow`
- `MouseTrackingEnter`
- `MouseTrackingLeave`

## Functions

### ClearScreen

> Clear the entire screen and move cursor to (1,1)

**Returns**: `Str` - Str

**Example**:

```taida
stdout(ClearScreen())
```

**AI-SideEffects**:
- none

### ClearLine

> Clear current line and move cursor to beginning

**Returns**: `Str` - Str

**Example**:

```taida
stdout(ClearLine())
```

**AI-SideEffects**:
- none

### AltScreenEnter

> Switch to alternate screen buffer

**Returns**: `Str` - Str

**Example**:

```taida
stdout(AltScreenEnter())
```

**AI-SideEffects**:
- none

### AltScreenLeave

> Switch back to main screen buffer

**Returns**: `Str` - Str

**Example**:

```taida
stdout(AltScreenLeave())
```

**AI-SideEffects**:
- none

### CursorMoveTo

> Move cursor to (col, row) position (1-based)

| Parameter | Type | Description |
|-----------|------|-------------|
| `col` | `-` | Int -- 1-based column (< 1 throws CursorMoveInvalidPosition) |
| `row` | `-` | Int -- 1-based row (< 1 throws CursorMoveInvalidPosition) |

**Returns**: `Str` - Str

**Throws**:
- CursorMoveInvalidPosition: col < 1 or row < 1

**Example**:

```taida
stdout(CursorMoveTo(10, 5))
```

**AI-SideEffects**:
- none

### CursorHide

> Hide the cursor

**Returns**: `Str` - Str

**Example**:

```taida
stdout(CursorHide())
```

**AI-SideEffects**:
- none

### CursorShow

> Show the cursor

**Returns**: `Str` - Str

**Example**:

```taida
stdout(CursorShow())
```

**AI-SideEffects**:
- none

### MouseTrackingEnter

> Enable mouse tracking (SGR 1006 + button/motion)

**Returns**: `Str` - Str

**Example**:

```taida
stdout(MouseTrackingEnter())
```

**AI-SideEffects**:
- none

### MouseTrackingLeave

> Disable mouse tracking

**Returns**: `Str` - Str

**Example**:

```taida
stdout(MouseTrackingLeave())
```

**AI-SideEffects**:
- none

# Module: prompt.td

## Exports

- `PromptMode`
- `PromptOptions`
- `CompletionState`
- `LineEditorAction`
- `LineEditorState`
- `LineEditorNew`
- `LineEditorStep`
- `LineEditorRender`

## Functions

### LineEditorNew

> Create a new LineEditorState from options

| Parameter | Type | Description |
|-----------|------|-------------|
| `opts` | `-` | - |

**Returns**: `@()`

### _insertAt

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |
| `ch` | `-` | - |

**Returns**: `Str`

### _deleteAt

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |

**Returns**: `Str`

### _deleteAtInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |

**Returns**: `Str`

### _deleteAtDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |

**Returns**: `Str`

### _makeState

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `cursor` | `-` | - |
| `state` | `-` | - |
| `action` | `-` | - |

**Returns**: `@()`

### _makeStateHist

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `cursor` | `-` | - |
| `state` | `-` | - |
| `histIdx` | `-` | - |
| `histSaved` | `-` | - |
| `action` | `-` | - |

**Returns**: `@()`

### LineEditorStep

> Process one key event and return the next state

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `key` | `-` | - |

**Returns**: `@()`

### _stepEditing

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `key` | `-` | - |

**Returns**: `@()`

### _stepArrowLeft

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepArrowRight

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepBackspace

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepBackspaceDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepDelete

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepDeleteDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepInsertChar

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `key` | `-` | - |

**Returns**: `@()`

### _stepInsertCharDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `key` | `-` | - |

**Returns**: `@()`

### _stepHistoryPrev

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepHistoryPrevDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepHistoryNext

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepHistoryNextDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _stepHistoryNextLoad

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `newIdx` | `-` | - |

**Returns**: `@()`

### LineEditorRender

> Generate display string from current state

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()` - @(line <= "", cursor_col <= 1)

### _getDisplayText

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### _cursorWidthCalc

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Int`

## Bindings

### PromptMode

> Prompt display mode

### PromptOptions

> Prompt configuration options

### CompletionState

> Completion candidates state (v1 minimal)

### LineEditorAction

> Result action from LineEditorStep

### LineEditorState

> Line editor internal state (pure, no side effects)

# Module: renderer.td

## Exports

- `Cell`
- `CellStyle`
- `ScreenBuffer`
- `DiffOpKind`
- `DiffOp`

## Bindings

### Cell

> One cell of data (character + style)

**AI-Context**:
Default text is " ". The renderer normalizes empty
text to " " for rendering. Wide-char placeholder cells also
carry " " as the second-half text.

### CellStyle

> Default style options for BufferWrite callers

**AI-Context**:
BufferWrite style arg must be this 6-field shape. Use
`CellStyle(fg <= "red", bg <= "", bold <= false, dim <= false,
underline <= false, italic <= false)` — every field must be present.

### ScreenBuffer

> Virtual screen buffer (row-major flat cells)

**AI-Context**:
Use `BufferNew[](cols, rows)` to allocate. Direct
construction is allowed but the cells list length must equal
cols*rows or the native renderer rejects the buffer with
`RendererInvalidArg`.

### DiffOpKind

> Diff operation kind

**AI-Context**:
Tag values are frozen — the Rust `renderer::diff`
matches against these literals. Renumbering breaks the addon ABI.

### DiffOp

> A single diff operation

# Module: style.td

## Exports

- `Color`
- `ResetStyle`
- `Stylize`
- `Color256`
- `ColorRgb`
- `Stylize256`
- `StylizeRgb`

## Functions

### _fgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### _bgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### _safeFgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### _safeBgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### _appendCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `acc` | `-` | - |
| `code` | `-` | - |

**Returns**: `Str`

### ResetStyle

> Return ANSI reset sequence

**Returns**: `Str` - Str

**Example**:

```taida
stdout(ResetStyle())
```

**AI-SideEffects**:
- none

### Stylize

> Apply color and decoration to text as ANSI string

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str |
| `opts` | `-` | @(fg, bg, bold, dim, underline, italic) |

**Returns**: `Str` - Str -- prefix + text + reset (or text as-is if no style)

**Throws**:
- StylizeInvalidColor: unknown fg / bg color name

**Example**:

```taida
stdout(Stylize("hello", @(fg <= Color.red, bold <= true)))
```

**AI-SideEffects**:
- none

### _validate256

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |
| `label` | `-` | - |

**Returns**: `Str`

### _fg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### _bg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### _safeFg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### _safeBg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### _validateRgbComponent

| Parameter | Type | Description |
|-----------|------|-------------|
| `value` | `-` | - |
| `label` | `-` | - |
| `component` | `-` | - |

**Returns**: `Str`

### _isNoColorRgb

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Bool`

### _fgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### _bgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### _safeFgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### _safeBgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### _validateRgbFull

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |
| `label` | `-` | - |

**Returns**: `Str`

### _validateRgbG

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |
| `label` | `-` | - |
| `prev` | `-` | - |

**Returns**: `Str`

### _validateRgbB

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |
| `label` | `-` | - |
| `prev` | `-` | - |

**Returns**: `Str`

### Stylize256

> Apply 256-color styling to text

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str |
| `opts` | `-` | @(fg <= Color256(index <= -1), bg <= Color256(index <= -1), bold, dim, underline, italic) |

**Returns**: `Str` - Str

**Throws**:
- StylizeInvalidColor: index out of 0-255 range

### StylizeRgb

> Apply RGB color styling to text

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str |
| `opts` | `-` | @(fg <= ColorRgb(...), bg <= ColorRgb(...), bold, dim, underline, italic) |

**Returns**: `Str` - Str

**Throws**:
- StylizeInvalidColor: r/g/b out of 0-255 range

## Bindings

### Color

> Basic 16-color palette (name constants)

**Example**:

```taida
stdout(Stylize("hello", @(fg <= Color.red)))
```

**AI-Context**:
Pass to Stylize fg / bg arguments.

### Color256

> 256-color index (0-255)

**AI-Context**:
Pass to Stylize256 fg / bg. index -1 means no color.

### ColorRgb

> RGB color (each component 0-255)

**AI-Context**:
Pass to StylizeRgb fg / bg. All -1 means no color.

# Module: terminal.td

## Exports

- `TerminalSize`
- `ReadKey`
- `KeyKind`
- `IsTerminal`
- `RawModeEnter`
- `RawModeLeave`
- `ClearScreen`
- `ClearLine`
- `AltScreenEnter`
- `AltScreenLeave`
- `CursorMoveTo`
- `CursorHide`
- `CursorShow`
- `Stylize`
- `Color`
- `ResetStyle`
- `Color256`
- `ColorRgb`
- `Stylize256`
- `StylizeRgb`
- `EventKind`
- `MouseKind`
- `ReadEvent`
- `MouseTrackingEnter`
- `MouseTrackingLeave`
- `Write`
- `WidthMode`
- `MeasureGrapheme`
- `DisplayWidth`
- `NormalizeCellText`
- `TruncateWidth`
- `PadWidth`
- `Cell`
- `CellStyle`
- `ScreenBuffer`
- `DiffOpKind`
- `DiffOp`
- `BufferNew`
- `BufferResize`
- `BufferClear`
- `BufferPut`
- `BufferWrite`
- `BufferFillRect`
- `BufferBlit`
- `RenderFull`
- `BufferDiff`
- `RenderOps`
- `RenderFrame`
- `PromptMode`
- `PromptOptions`
- `CompletionState`
- `LineEditorAction`
- `LineEditorState`
- `LineEditorNew`
- `LineEditorStep`
- `LineEditorRender`
- `SpinnerState`
- `SpinnerNext`
- `SpinnerRender`
- `ProgressOptions`
- `ProgressBar`
- `StatusLine`

## Bindings

### KeyKind

> キー種別を表す列挙パック（28バリアント）

**Example**:

```taida
key <= ReadKey[]()
key.kind |== KeyKind.Enter => stdout("Enter pressed")
key.kind |== KeyKind.Char  => stdout(key.text)
```

**AI-Context**:
ReadKey の戻り値 `kind` フィールドと比較して使う。
タグ値（Int）は v1 ABI で凍結済み。追加・並び替えは ABI bump が必要。

### TerminalSize

> ターミナルのカラム数・行数を取得する

**Returns**: @(cols: Int, rows: Int) — 両方 >= 1

**Throws**:
- TerminalSizeNotATty: stdout が TTY でない場合
- TerminalSizeIoctl: ioctl(TIOCGWINSZ) が失敗した場合

**Example**:

```taida
size <= TerminalSize[]()
stdout(size.cols)
stdout(size.rows)
```

**AI-SideEffects**:
- ioctl システムコールを発行する（読み取り専用、副作用なし）

### ReadKey

> キーボードから1キー分の入力を読み取る（raw モード）

**Returns**: @(kind: KeyKind, text: Str, ctrl: Bool, alt: Bool, shift: Bool)

**Throws**:
- ReadKeyNotATty: stdin が TTY でない場合
- ReadKeyRawMode: raw モードの開始/終了に失敗した場合
- ReadKeyEof: EOF を検出した場合
- ReadKeyInterrupted: シグナル割り込みが発生した場合

**Example**:

```taida
key <= ReadKey[]()
key.kind |== KeyKind.Escape => stdout("Escaped!")
```

**AI-Context**:
ブロッキング呼び出し。1キー読み取り後に raw モードを解除して返る。

**AI-SideEffects**:
- stdin を一時的に raw モードに変更し、RAII で自動復元する

### IsTerminal

> 指定ストリームが TTY かどうかを判定する

**Returns**: Bool

**Throws**:
- IsTerminalInvalidStream: stream が "stdin" / "stdout" / "stderr" 以外の場合

**Example**:

```taida
interactive <= IsTerminal[]("stdin")
stdout(interactive.toString())
```

**AI-SideEffects**:
- `isatty` システムコールを発行する（読み取り専用、副作用なし）

### RawModeEnter

> stdin を raw モードに切り替える

**Returns**: @() — 空パック

**Throws**:
- RawModeNotATty: stdin が TTY でない場合
- RawModeAlreadyActive: 既に raw モードの場合（二重 enter 禁止）
- RawModeEnterFailed: termios 操作に失敗した場合

**Example**:

```taida
RawModeEnter[]()
key <= ReadKey[]()
RawModeLeave[]()
```

**AI-Context**:
TUI アプリで RawModeEnter → ReadKey xN → RawModeLeave の
パターンに使う。raw モード中の ReadKey は自身の enter/leave をスキップする。

**AI-SideEffects**:
- stdin の termios を変更する。RawModeLeave で復元必須。

### RawModeLeave

> stdin を raw モードから復元する

**Returns**: @() — 空パック

**Throws**:
- RawModeNotActive: raw モードでない状態で呼んだ場合
- RawModeLeaveFailed: termios 復元に失敗した場合

**Example**:

```taida
RawModeEnter[]()
key <= ReadKey[]()
RawModeLeave[]()
```

**AI-SideEffects**:
- stdin の termios を復元する

### MeasureGrapheme

> 単一グラフィムの表示幅と分類を測定する

**Returns**: Notes: 空文字列は @(width <= 0, mode <= WidthMode.Zero) を返す

### DisplayWidth

> 文字列の合計表示幅をセル数で返す

**Returns**: Notes: 結合文字 / 制御文字は 0、East Asian Wide / Fullwidth は 2、それ以外は 1

### NormalizeCellText

> セルテキストを正規化する (TAB -> 4 spaces, \n/\r 除去, 空文字列 -> " ")

**Returns**: Str

### TruncateWidth

> 文字列を指定表示幅で切り詰める (右端で打ち切り、wide char 境界で余剰は drop)

**Returns**: Notes: width < 1 は "" を返す

### PadWidth

> 右側を空白で埋めて指定表示幅に揃える

**Returns**: Notes: 既に width 以上の場合は text をそのまま返す

### BufferNew

> 空の ScreenBuffer を指定サイズで確保する

**Returns**: ScreenBuffer — cells は row-major で cols*rows 個の default Cell

**Throws**:
- RendererInvalidSize — cols < 1 または rows < 1

**AI-Context**:
native 実装では `vec![default; cols*rows]` で一括確保。
`@a.6` までは pure Taida の `Append` ループで O(N²) だったため、
120×40 で 3.3 秒を消費する hot path だった (TMB-024 で解消)。

### BufferResize

> ScreenBuffer を新しいサイズで再確保する

**Returns**: ScreenBuffer

**Throws**:
- RendererInvalidSize — cols < 1 または rows < 1
- Notes:
- - cells は default Cell で seed される（fill 引数は v1 互換のため残置、無効化）
- - cursor_col / cursor_row は新 bounds 内に clamp される
- - cursor_visible は prev から継承される

### BufferPut

> 単一セルを (col, row) に書き込む

**Returns**: ScreenBuffer — 同じサイズの新パック

**Throws**:
- RendererOutOfBounds — col<1 / row<1 / col>cols / row>rows

**AI-Context**:
内部表現は Vec<Cell> に直接書き込むため O(1)。

### BufferWrite

> テキストを (col, row) から書き、表示幅で進める

**Returns**: ScreenBuffer

**Throws**:
- RendererOutOfBounds — 開始位置が範囲外
- Notes: 右端で truncate；wide char は 2 セル使用、2 セル目はスペース placeholder；
- width 0 grapheme (combining mark / control) はスキップ。

### BufferFillRect

> 矩形領域を cell で塗りつぶす

**Throws**:
- RendererOutOfBounds — col<1 / row<1。width<1 / height<1 は no-op。

### BufferClear

> バッファ全体を fill cell で塗りつぶす

### BufferDiff

> 2 つのバッファ間の最小 diff 操作リストを生成する

**Returns**: @(ops <= @[DiffOp...], requires_full <= Bool)

**AI-Context**:
requires_full=true は cols/rows が異なる場合。
呼び出し側は RenderFull(next) にフォールバックする。

### RenderFull

> バッファ全体を ANSI 文字列としてレンダリングする

**Returns**: Str — CursorHide + 行毎 CursorMoveTo + cell text + ResetStyle + CursorMoveTo(cursor) + (visible なら CursorShow)

### RenderOps

> DiffOp リストを ANSI 文字列に変換する

**Returns**: Str

### RenderFrame

> prev / next の差分を最小 ANSI 出力として生成する

**Returns**: @(text <= Str, next <= ScreenBuffer)

**AI-Context**:
requires_full なら RenderFull(next)、それ以外は RenderOps(diff.ops)。

### BufferBlit

> sub バッファを main バッファの (col, row) 位置に合成する

**Returns**: ScreenBuffer — main と同じ cols/rows の新パック

**Throws**:
- RendererOutOfBounds — col<1 / row<1
- Notes:
- - main からはみ出す sub のセルは silently clip（BufferFillRect と同じ規約）。
- - (col, row) が main の範囲外（右/下）を指すなら no-op（main をそのまま返す）。
- - wide char placeholder cell（text=" " の 2 セル目）は sub からそのまま運ばれる。
- - style 属性（fg/bg/bold/dim/underline/italic）はセル毎に保持される。

**AI-Context**:
TUI の pane 合成用プリミティブ。pure-Taida ループで sub を main に
重ねると Taida の list index O(n) が効いて O(N²) になる（TMB-022）。
この native 実装は Vec<Cell> 上の線形 copy で O(N) に抑える。

### EventKind

> イベント種別を表す列挙パック（4バリアント）

**Example**:

```taida
event <= ReadEvent[]()
event.kind |== EventKind.Key => stdout("Key event")
event.kind |== EventKind.Mouse => stdout("Mouse event")
event.kind |== EventKind.Resize => stdout("Resize event")
```

**AI-Context**:
ReadEvent の戻り値 `kind` フィールドと比較して使う。

### MouseKind

> マウスイベント種別を表す列挙パック（6バリアント）

**Example**:

```taida
event <= ReadEvent[]()
event.kind |== EventKind.Mouse =>
event.mouse.kind |== MouseKind.Down => stdout("Click!")
```

**AI-Context**:
ReadEvent の戻り値 `mouse.kind` フィールドと比較して使う。

### ReadEvent

> 統合イベントを1つ読み取る（キー / マウス / リサイズ）

**Returns**: @(kind: EventKind, key: @(...), mouse: @(...), resize: @(...))

**Throws**:
- ReadEventNotInRawMode: raw モードでない場合
- ReadEventNotATty: stdin が TTY でない場合
- ReadEventReadFailed: read(2) が失敗した場合
- ReadEventEof: stdin が閉じた場合
- ReadEventInterrupted: シグナル割り込みが発生した場合

**Example**:

```taida
RawModeEnter[]()
stdout(MouseTrackingEnter[]())
event <= ReadEvent[]()
event.kind |== EventKind.Key => stdout(event.key.text)
event.kind |== EventKind.Mouse => stdout("mouse at " + event.mouse.col.toString())
event.kind |== EventKind.Resize => stdout("new size: " + event.resize.cols.toString())
stdout(MouseTrackingLeave[]())
RawModeLeave[]()
```

**AI-Context**:
raw モード必須。ReadKey の上位互換。

**AI-SideEffects**:
- ブロッキング呼び出し。stdin + SIGWINCH を poll で多重化。

### Write

> stdout に改行なしで即時書き出す（TUI 用）

**Returns**: Int — 書き込んだバイト数

**Throws**:
- WriteFailed: write_all / flush が I/O エラーで失敗した場合 (EPIPE 等)
- WriteBuildValue: 戻り値 Int のホスト側確保に失敗した場合
- WritePanic: write path 内で panic が発生した場合（FFI 境界で捕捉）

**Example**:

```taida
Write[]("\x1b[2J\x1b[H")          // clear + home cursor
Write[](CursorMoveTo[](10, 5))    // カーソル移動（改行なし）
n <= Write[]("hello")             // n == 5
```

**AI-Context**:
`stdout()` builtin は push 単位で `\n` を暗黙追加する行指向 I/O のため、
ANSI エスケープを連続送信する TUI 用途にはこの Write[]() を使う。
non-TTY (pipe / redirect) でも panic せず動作する（成功経路）。

**AI-SideEffects**:
- stdout に即時書き出す（flush 付き）。改行の暗黙追加は行わない。

# Module: widgets.td

## Exports

- `SpinnerState`
- `SpinnerNext`
- `SpinnerRender`
- `ProgressOptions`
- `ProgressBar`
- `StatusLine`

## Functions

### SpinnerNext

> Advance the spinner to the next frame

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### _spinnerNextInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `@()`

### SpinnerRender

> Render the spinner as a display string

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### _spinnerDoneText

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### _spinnerActiveText

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### ProgressBar

> Render a progress bar string

| Parameter | Type | Description |
|-----------|------|-------------|
| `current` | `-` | - |
| `total` | `-` | - |
| `opts` | `-` | - |

**Returns**: `Str`

**Throws**:
- ProgressInvalidTotal if total < 1, ProgressInvalidCurrent if current < 0

### _progressBarInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `current` | `-` | - |
| `total` | `-` | - |
| `opts` | `-` | - |

**Returns**: `Str`

### _repeatStr

| Parameter | Type | Description |
|-----------|------|-------------|
| `ch` | `-` | - |
| `count` | `-` | - |

**Returns**: `Str`

### StatusLine

> Generate a status line with left/right text

| Parameter | Type | Description |
|-----------|------|-------------|
| `left` | `-` | - |
| `right` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

### _statusLineInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `left` | `-` | - |
| `right` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

### _statusLineTruncate

| Parameter | Type | Description |
|-----------|------|-------------|
| `left` | `-` | - |
| `right` | `-` | - |
| `width` | `-` | - |
| `rightW` | `-` | - |

**Returns**: `Str`

## Bindings

### SpinnerState

> Spinner state

### ProgressOptions

> Progress bar options

# Module: width.td

## Exports

- `WidthMode`
- `MeasureGrapheme`
- `DisplayWidth`
- `NormalizeCellText`
- `TruncateWidth`
- `PadWidth`

## Functions

### _inRange

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |
| `lo` | `-` | - |
| `hi` | `-` | - |

**Returns**: `Bool`

### _isCombining

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |

**Returns**: `Bool`

### _isWide

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |

**Returns**: `Bool`

### _isControl

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |

**Returns**: `Bool`

### MeasureGrapheme

> Measure the display width and category of a single grapheme

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str -- the text to measure (only the first grapheme is measured) |

**Returns**: `@(width: Int, mode: Int)` - @(width <= 0, mode <= WidthMode.Narrow)

### _measureGraphemeInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |

**Returns**: `@(width: Int, mode: Int)`

### _dwCalc

> Calculate the total display width (cell count) of a string

| Parameter | Type | Description |
|-----------|------|-------------|
| `src` | `-` | - |
| `idx` | `-` | - |
| `acc` | `-` | - |
| `len` | `-` | - |

**Returns**: `Int` - Int -- display width in cells

### DisplayWidth

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |

**Returns**: `Int`

### _normLoop

> Normalize cell text (TAB -> spaces, newline -> strip, empty -> " ")

| Parameter | Type | Description |
|-----------|------|-------------|
| `src` | `-` | - |
| `idx` | `-` | - |
| `acc` | `-` | - |
| `len` | `-` | - |

**Returns**: `Str` - Str -- normalized text

### _normFinish

| Parameter | Type | Description |
|-----------|------|-------------|
| `result` | `-` | - |

**Returns**: `Str`

### NormalizeCellText

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |

**Returns**: `Str`

### _twLoop

> Truncate text to fit within a given display width

| Parameter | Type | Description |
|-----------|------|-------------|
| `src` | `-` | - |
| `idx` | `-` | - |
| `acc` | `-` | - |
| `remaining` | `-` | - |
| `len` | `-` | - |

**Returns**: `Str` - Str -- truncated text

### TruncateWidth

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

### PadWidth

> Pad text with spaces on the right to reach a target display width

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str -- the text to pad |
| `width` | `-` | Int -- target display width |

**Returns**: `Str` - Str -- padded text

## Bindings

### WidthMode

> Unicode width category enum pack

**AI-Context**:
Compare with MeasureGrapheme result `mode` field.
Tag values are frozen — the Rust `width.rs` matches against these
literals (Narrow=0, Wide=1, Zero=2, Ambiguous=3).

