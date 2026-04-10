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

**Since**: a.5

**AI-SideEffects**:
- none

### ClearLine

> Clear current line and move cursor to beginning

**Returns**: `Str` - Str

**Example**:

```taida
stdout(ClearLine())
```

**Since**: a.5

**AI-SideEffects**:
- none

### AltScreenEnter

> Switch to alternate screen buffer

**Returns**: `Str` - Str

**Example**:

```taida
stdout(AltScreenEnter())
```

**Since**: a.5

**AI-SideEffects**:
- none

### AltScreenLeave

> Switch back to main screen buffer

**Returns**: `Str` - Str

**Example**:

```taida
stdout(AltScreenLeave())
```

**Since**: a.5

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

**Since**: a.5

**AI-SideEffects**:
- none

### CursorHide

> Hide the cursor

**Returns**: `Str` - Str

**Example**:

```taida
stdout(CursorHide())
```

**Since**: a.5

**AI-SideEffects**:
- none

### CursorShow

> Show the cursor

**Returns**: `Str` - Str

**Example**:

```taida
stdout(CursorShow())
```

**Since**: a.5

**AI-SideEffects**:
- none

### MouseTrackingEnter

> Enable mouse tracking (SGR 1006 + button/motion)

**Returns**: `Str` - Str

**Example**:

```taida
stdout(MouseTrackingEnter())
```

**Since**: a.5

**AI-SideEffects**:
- none

### MouseTrackingLeave

> Disable mouse tracking

**Returns**: `Str` - Str

**Example**:

```taida
stdout(MouseTrackingLeave())
```

**Since**: a.5

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

**Since**: a.7

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

**Since**: a.7

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

**Since**: a.7

### _getDisplayText

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### _maskLoop

| Parameter | Type | Description |
|-----------|------|-------------|
| `acc` | `-` | - |
| `remaining` | `-` | - |

**Returns**: `Str`

### _cursorWidthCalc

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Int`

## Bindings

### PromptMode

> Prompt display mode

**Since**: a.7

### PromptOptions

> Prompt configuration options

**Since**: a.7

### CompletionState

> Completion candidates state (v1 minimal)

**Since**: a.7

### LineEditorAction

> Result action from LineEditorStep

**Since**: a.7

### LineEditorState

> Line editor internal state (pure, no side effects)

**Since**: a.7

# Module: renderer.td

## Exports

- `Cell`
- `ScreenBuffer`
- `DiffOpKind`
- `DiffOp`
- `BufferNew`
- `BufferResize`
- `BufferClear`
- `BufferPut`
- `BufferWrite`
- `BufferFillRect`
- `RenderFull`
- `BufferDiff`
- `RenderOps`
- `RenderFrame`

## Functions

### _cellIndex

| Parameter | Type | Description |
|-----------|------|-------------|
| `cols` | `-` | - |
| `col` | `-` | - |
| `row` | `-` | - |

**Returns**: `Int`

### _hasStyle

| Parameter | Type | Description |
|-----------|------|-------------|
| `cell` | `-` | - |

**Returns**: `Bool`

### _cellStyleOpts

| Parameter | Type | Description |
|-----------|------|-------------|
| `cell` | `-` | - |

**Returns**: `@()`

### _cellEq

| Parameter | Type | Description |
|-----------|------|-------------|
| `a` | `-` | - |
| `b` | `-` | - |

**Returns**: `Bool`

### _makeCellsLoop

| Parameter | Type | Description |
|-----------|------|-------------|
| `acc` | `-` | - |
| `remaining` | `-` | - |
| `fill` | `-` | - |

**Returns**: `@[Str]`

### _makeCellsAppend

| Parameter | Type | Description |
|-----------|------|-------------|
| `acc` | `-` | - |
| `remaining` | `-` | - |
| `fill` | `-` | - |

**Returns**: `@[Str]`

### BufferNew

> Create an empty buffer of the specified size

| Parameter | Type | Description |
|-----------|------|-------------|
| `cols` | `-` | - |
| `rows` | `-` | - |

**Returns**: `@()`

**Throws**:
- RendererInvalidSize if cols < 1 or rows < 1

**Since**: a.6

### _bufferNewInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `cols` | `-` | - |
| `rows` | `-` | - |

**Returns**: `@()`

### BufferResize

> Resize a buffer, preserving existing content where possible

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |
| `cols` | `-` | - |
| `rows` | `-` | - |
| `fill` | `-` | - |

**Returns**: `@()`

**Since**: a.6

### _bufferResizeInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |
| `cols` | `-` | - |
| `rows` | `-` | - |
| `fill` | `-` | - |

**Returns**: `@()`

### BufferClear

> Clear the entire buffer with a fill cell

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |
| `fill` | `-` | - |

**Returns**: `@()`

**Since**: a.6

### BufferPut

> Write a single cell at a given position

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |
| `col` | `-` | - |
| `row` | `-` | - |
| `cell` | `-` | - |

**Returns**: `@()`

**Throws**:
- RendererOutOfBounds

**Since**: a.6

### BufferWrite

> Write text with style at a given position

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |
| `col` | `-` | - |
| `row` | `-` | - |
| `text` | `-` | - |
| `style` | `-` | - |

**Returns**: `@()`

**Throws**:
- RendererOutOfBounds

**Since**: a.6

### BufferFillRect

> Fill a rectangular region with a cell

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |
| `col` | `-` | - |
| `row` | `-` | - |
| `width` | `-` | - |
| `height` | `-` | - |
| `cell` | `-` | - |

**Returns**: `@()`

**Since**: a.6

### RenderFull

> Render the entire buffer as an ANSI string

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |

**Returns**: `Str`

**Since**: a.6

### _renderFullInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | - |

**Returns**: `Str`

### BufferDiff

> Generate a minimal list of diff operations between two buffers

| Parameter | Type | Description |
|-----------|------|-------------|
| `prev` | `-` | - |
| `next` | `-` | - |

**Returns**: `@()`

**Since**: a.6

### RenderOps

> Convert a list of DiffOps to an ANSI string

| Parameter | Type | Description |
|-----------|------|-------------|
| `ops` | `-` | - |

**Returns**: `Str`

**Since**: a.6

### RenderFrame

> Compare prev and next buffers, produce minimal ANSI output

| Parameter | Type | Description |
|-----------|------|-------------|
| `prev` | `-` | - |
| `next` | `-` | - |

**Returns**: `@()`

**Since**: a.6

## Bindings

### Cell

> One cell of data (character + style)

**Since**: a.6

### ScreenBuffer

> Virtual screen buffer (row-major flat cells)

**Since**: a.6

### DiffOpKind

> Diff operation kind

**Since**: a.6

### DiffOp

> A single diff operation

**Since**: a.6

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

**Since**: a.5

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

**Since**: a.5

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

**Since**: a.6

### StylizeRgb

> Apply RGB color styling to text

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str |
| `opts` | `-` | @(fg <= ColorRgb(...), bg <= ColorRgb(...), bold, dim, underline, italic) |

**Returns**: `Str` - Str

**Throws**:
- StylizeInvalidColor: r/g/b out of 0-255 range

**Since**: a.6

## Bindings

### Color

> Basic 16-color palette (name constants)

**Example**:

```taida
stdout(Stylize("hello", @(fg <= Color.red)))
```

**Since**: a.5

**AI-Context**:
Pass to Stylize fg / bg arguments.

### Color256

> 256-color index (0-255)

**Since**: a.6

**AI-Context**:
Pass to Stylize256 fg / bg. index -1 means no color.

### ColorRgb

> RGB color (each component 0-255)

**Since**: a.6

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
- `WidthMode`
- `MeasureGrapheme`
- `DisplayWidth`
- `NormalizeCellText`
- `TruncateWidth`
- `PadWidth`
- `Cell`
- `ScreenBuffer`
- `DiffOpKind`
- `DiffOp`
- `BufferNew`
- `BufferResize`
- `BufferClear`
- `BufferPut`
- `BufferWrite`
- `BufferFillRect`
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

**Since**: a.1

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

**Since**: a.1

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

**Since**: a.4

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

**Since**: a.5

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

**Since**: a.5

**AI-SideEffects**:
- stdin の termios を復元する

### EventKind

> イベント種別を表す列挙パック（4バリアント）

**Example**:

```taida
event <= ReadEvent[]()
event.kind |== EventKind.Key => stdout("Key event")
event.kind |== EventKind.Mouse => stdout("Mouse event")
event.kind |== EventKind.Resize => stdout("Resize event")
```

**Since**: a.5

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

**Since**: a.5

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

**Since**: a.5

**AI-Context**:
raw モード必須。ReadKey の上位互換。

**AI-SideEffects**:
- ブロッキング呼び出し。stdin + SIGWINCH を poll で多重化。

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

**Since**: a.7

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

**Since**: a.7

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

**Since**: a.7

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

### _repeatLoop

| Parameter | Type | Description |
|-----------|------|-------------|
| `acc` | `-` | - |
| `ch` | `-` | - |
| `remaining` | `-` | - |

**Returns**: `Str`

### StatusLine

> Generate a status line with left/right text

| Parameter | Type | Description |
|-----------|------|-------------|
| `left` | `-` | - |
| `right` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

**Since**: a.7

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

**Since**: a.7

### ProgressOptions

> Progress bar options

**Since**: a.7

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

**Since**: a.6

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

**Since**: a.6

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

**Since**: a.6

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

**Since**: a.6

### TruncateWidth

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

### _padLoop

> Pad text with spaces on the right to reach a target display width

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `remaining` | `-` | - |

**Returns**: `Str` - Str -- padded text

**Since**: a.6

### PadWidth

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

## Bindings

### WidthMode

> Unicode width category enum pack

**Since**: a.6

**AI-Context**:
Compare with MeasureGrapheme result `mode` field.

