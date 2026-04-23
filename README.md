# taida-lang/terminal

Taida Lang official terminal package — TTY detection, size query, key / event input, raw mode, screen / cursor control, ANSI styling (16 / 256 / RGB), Unicode width, virtual buffer with diff renderer, line editor, and UX widgets.

- **Release**: `@a.7` (2026-04-24)
- **Backend**: Native-only addon (Rust `cdylib` + Taida facade). The interpreter dispatches to the cdylib through addon ABI v1.
- **Exports**: 61 public symbols (see below).

## Install

```bash
taida install taida-lang/terminal
```

## Usage

Import the subset you need from the facade:

```taida
>>> taida-lang/terminal => @(
  IsTerminal, TerminalSize,
  ReadKey, KeyKind,
  RawModeEnter, RawModeLeave,
  ReadEvent, EventKind, MouseKind,
  Write,
  ClearScreen, ClearLine,
  AltScreenEnter, AltScreenLeave,
  CursorMoveTo, CursorHide, CursorShow,
  MouseTrackingEnter, MouseTrackingLeave,
  Stylize, Color, ResetStyle,
  Stylize256, Color256, StylizeRgb, ColorRgb,
  DisplayWidth, MeasureGrapheme, PadWidth, TruncateWidth, NormalizeCellText, WidthMode,
  Cell, CellStyle, ScreenBuffer,
  BufferNew, BufferResize, BufferClear, BufferPut, BufferWrite,
  BufferFillRect, BufferBlit, BufferDiff,
  RenderFull, RenderOps, RenderFrame,
  DiffOpKind, DiffOp,
  PromptMode, PromptOptions, CompletionState,
  LineEditorAction, LineEditorState,
  LineEditorNew, LineEditorStep, LineEditorRender,
  SpinnerState, SpinnerNext, SpinnerRender,
  ProgressOptions, ProgressBar, StatusLine
)
```

**Call convention.** Native entries (TTY detection, raw mode, I/O,
renderer allocation, width) are invoked through the mold-call syntax
`Name[]()` — the empty `[]` routes the call through the addon sentinel.
Pure-Taida helpers (ANSI strings, styling, renderer mutation, line
editor, widgets) are plain functions called with `Name(...)`.

### TTY detection and terminal size

```taida
| IsTerminal[]("stdout") |> (
  size <= TerminalSize[]()
  stdout(size.cols.toString() + "x" + size.rows.toString())
)
```

`|` is the guard operator; `|>` separates the condition from the body.
Conditional branches are values, not statements — wrap them in `(...)`
when you need multiple arms, per `docs/guide/07_control_flow.md`.

### Single key read

```taida
key <= ReadKey[]()

(
  | key.kind == KeyKind.Escape |> stdout("Escaped!")
  | key.kind == KeyKind.Enter  |> stdout("Submitted")
  | key.kind == KeyKind.Char   |> stdout("typed: " + key.text)
  | _                          |> stdout("other: " + key.kind.toString())
)
```

### Persistent raw mode + unified events

```taida
RawModeEnter[]()
Write[](MouseTrackingEnter())

event <= ReadEvent[]()
(
  | event.kind == EventKind.Key    |> stdout("key: " + event.key.text)
  | event.kind == EventKind.Mouse  |> stdout("click at " + event.mouse.col.toString())
  | event.kind == EventKind.Resize |> stdout("resize: " + event.resize.cols.toString())
  | _                              |> stdout("unknown event")
)

Write[](MouseTrackingLeave())
RawModeLeave[]()
```

### ANSI strings (pure helpers, no side effects)

```taida
Write[](ClearScreen())
Write[](CursorHide())
Write[](CursorMoveTo(10, 5))
Write[]("hello")
Write[](CursorShow())
```

`Write[](bytes)` writes to stdout without appending a newline — use it
when `stdout()`'s implicit `\n` would corrupt ANSI framing (cursor
moves, partial redraws, spinner ticks, progress updates).

### Styling (16 / 256 / RGB)

```taida
red <= Stylize(
  "hello",
  @(fg <= Color.red, bg <= "", bold <= true, dim <= false, underline <= false, italic <= false)
)
stdout(red)

orange <= Stylize256(
  "256",
  @(fg <= Color256(index <= 208), bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
)
stdout(orange)

rgb <= StylizeRgb(
  "rgb",
  @(fg <= ColorRgb(r <= 255, g <= 128, b <= 0), bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
)
stdout(rgb)

stdout(ResetStyle())
```

### Unicode display width

```taida
stdout(DisplayWidth("hello").toString())     // 5
stdout(DisplayWidth("漢字").toString())       // 4
stdout(PadWidth("hi", 5))                    // "hi   "
stdout(TruncateWidth("abcdef", 3))           // "abc"

m <= MeasureGrapheme("漢")
stdout(m.width.toString() + " / mode=" + m.mode.toString())
```

### Virtual buffer + diff renderer

```taida
plain <= CellStyle(
  fg <= "", bg <= "",
  bold <= false, dim <= false, underline <= false, italic <= false
)

prev <= BufferNew[](20, 5)
next <= BufferWrite(prev, 1, 1, "Hello, world", plain)

frame <= RenderFrame(prev, next)
Write[](frame.text)
// feed frame.next as the next `prev` to continue the diff chain
```

`RenderFrame(prev, next)` returns `@(text, next)` with the minimal ANSI
diff; on size change it falls back to `RenderFull(next)`. `BufferNew[]`
and `BufferResize[]` are native (`@a.7` / TMB-024 — allocation moved off
the pure-Taida `Append` loop to eliminate the O(N²) hot path).

### Line editor (pure state machine)

```taida
opts <= PromptOptions(
  prompt      <= "> ",
  initial     <= "",
  placeholder <= "type here",
  mode        <= PromptMode.Normal,
  history     <= @[],
  completion  <= @()
)

editor <= LineEditorNew(opts)

RawModeEnter[]()
editor <= LineEditorStep(editor, ReadEvent[]())
RawModeLeave[]()

view <= LineEditorRender(editor)
Write[](view.text)
```

`LineEditorStep` is pure — it takes the current state and one event, and
returns the next state. Compose your own event loop (`ReadEvent[]()`,
kitty protocol, mocked events in tests) around it.

### UX widgets

```taida
stdout(ProgressBar(50, 100, ProgressOptions))
stdout(StatusLine("left", "right", 40))

sp  <= SpinnerState
sp2 <= SpinnerNext(sp)
stdout(SpinnerRender(sp2))
```

## Exports (61 symbols)

### Native entries (Rust addon, call as `Name[](...)`)

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `IsTerminal` | `(stream: Str) -> Bool` | stdin / stdout / stderr TTY check |
| `TerminalSize` | `() -> @(cols: Int, rows: Int)` | both fields >= 1 |
| `ReadKey` | `() -> @(kind, text, ctrl, alt, shift)` | single key read; manages raw mode for one call |
| `RawModeEnter` | `() -> @()` | enter raw mode (paired with `RawModeLeave`) |
| `RawModeLeave` | `() -> @()` | leave raw mode |
| `ReadEvent` | `() -> @(kind, key, mouse, resize)` | unified event — **raw mode required** |
| `Write` | `(bytes: Str) -> Int` | unbuffered stdout write, returns byte count, no implicit `\n` |
| `BufferNew` | `(cols: Int, rows: Int) -> ScreenBuffer` | allocate a fresh buffer |
| `BufferResize` | `(buf, cols, rows, fill?) -> ScreenBuffer` | reallocate, clamp cursor to new bounds |
| `MeasureGrapheme` | `(text: Str) -> @(width, mode)` | single grapheme width + `WidthMode` tag |
| `DisplayWidth` | `(text: Str) -> Int` | total display width (cells) |
| `NormalizeCellText` | `(text: Str) -> Str` | empty → space, strip control chars, TAB → 4 spaces |
| `TruncateWidth` | `(text: Str, width: Int) -> Str` | right-edge truncation, wide-char aware |
| `PadWidth` | `(text: Str, width: Int) -> Str` | right-pad with spaces |

### Pure-Taida facades (call as `Name(...)`)

#### ANSI control

| Symbol | Returns |
|--------|---------|
| `ClearScreen` | `"\x1b[2J\x1b[H"` |
| `ClearLine` | `"\x1b[2K\r"` |
| `AltScreenEnter` | `"\x1b[?1049h"` |
| `AltScreenLeave` | `"\x1b[?1049l"` |
| `CursorMoveTo` | `(col, row) -> "\x1b[{row};{col}H"` (1-based; throws `CursorMoveInvalidPosition` on `< 1`) |
| `CursorHide` | `"\x1b[?25l"` |
| `CursorShow` | `"\x1b[?25h"` |
| `MouseTrackingEnter` | SGR 1006 + button + motion enable |
| `MouseTrackingLeave` | SGR 1006 + button + motion disable |

#### Styling

| Symbol | Description |
|--------|-------------|
| `Color` | 16-color palette pack (`Color.red`, `Color.bright_white`, …) |
| `Stylize` | `(text, @(fg, bg, bold, dim, underline, italic)) -> Str` |
| `Color256` | `@(index: Int)` — 0–255 |
| `Stylize256` | 256-color variant |
| `ColorRgb` | `@(r, g, b: Int)` — each 0–255 |
| `StylizeRgb` | truecolor variant |
| `ResetStyle` | `"\x1b[0m"` |

Style packs require all six fields (`fg`, `bg`, `bold`, `dim`,
`underline`, `italic`). Use `""` for unset color and `false` for unset
attributes.

#### Width

| Symbol | Description |
|--------|-------------|
| `WidthMode` | enum pack — `Narrow` = 0, `Wide` = 1, `Zero` = 2, `Ambiguous` = 3 |

(The five width helpers — `MeasureGrapheme`, `DisplayWidth`,
`NormalizeCellText`, `TruncateWidth`, `PadWidth` — are dispatched to
native and listed above.)

#### Virtual buffer + renderer

| Symbol | Description |
|--------|-------------|
| `Cell` | `@(text, fg, bg, bold, dim, underline, italic)` |
| `CellStyle` | `@(fg, bg, bold, dim, underline, italic)` — helper pack for style args |
| `ScreenBuffer` | `@(cols, rows, cells, cursor_col, cursor_row, cursor_visible)` |
| `DiffOpKind` | enum pack — `MoveTo`, `Write`, `ClearLine`, `ShowCursor`, `HideCursor` |
| `DiffOp` | `@(kind, col, row, text, style)` |
| `BufferClear` | `(buf, fill?) -> ScreenBuffer` |
| `BufferPut` | `(buf, col, row, cell) -> ScreenBuffer` |
| `BufferWrite` | `(buf, col, row, text, style?) -> ScreenBuffer` — width-aware, right-edge truncation |
| `BufferFillRect` | `(buf, col, row, width, height, cell) -> ScreenBuffer` |
| `BufferBlit` | `(main, sub, col, row) -> ScreenBuffer` — composite `sub` at `(col, row)`, clips overflow, drops half wide-chars at right edge |
| `RenderFull` | `(buf) -> Str` — full redraw |
| `BufferDiff` | `(prev, next) -> @(ops, requires_full)` |
| `RenderOps` | `(ops) -> Str` — diff ops to ANSI string |
| `RenderFrame` | `(prev, next) -> @(text, next)` — minimal diff or full fallback |

#### Line editor (pure state machine)

| Symbol | Description |
|--------|-------------|
| `PromptMode` | enum pack — `Normal`, `Password` |
| `PromptOptions` | `@(prompt, initial, placeholder, mode, history, completion)` |
| `CompletionState` | `@(items, selected, visible)` |
| `LineEditorAction` | enum pack — `Editing`, `Submitted`, `Cancelled` |
| `LineEditorState` | full editor state pack |
| `LineEditorNew` | `(opts) -> LineEditorState` |
| `LineEditorStep` | `(state, event) -> LineEditorState` — pure transition |
| `LineEditorRender` | `(state) -> @(text, cursor_col)` |

#### UX widgets

| Symbol | Description |
|--------|-------------|
| `SpinnerState` | `@(frame, label, done)` |
| `SpinnerNext` | `(state) -> SpinnerState` |
| `SpinnerRender` | `(state) -> Str` |
| `ProgressOptions` | `@(width, complete_char, incomplete_char, left_label, right_label)` |
| `ProgressBar` | `(current, total, opts?) -> Str` |
| `StatusLine` | `(left, right?, width?) -> Str` |

#### Enums (value packs)

| Symbol | Description |
|--------|-------------|
| `KeyKind` | 28 variants — `Char`, `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `ArrowUp/Down/Left/Right`, `Home`, `End`, `PageUp/Down`, `Insert`, `F1`–`F12`, `Unknown` |
| `EventKind` | `Key`, `Mouse`, `Resize`, `Unknown` |
| `MouseKind` | `Down`, `Up`, `Move`, `Drag`, `ScrollUp`, `ScrollDown` |

## Error variants

Functions throw deterministic errors (no silent fallbacks). Catch with
the `|==` ceiling at the top of your handler.

- `IsTerminalInvalidStream`
- `ReadKeyNotATty` / `ReadKeyRawMode` / `ReadKeyEof` / `ReadKeyInterrupted` / `ReadKeyUnsupported`
- `TerminalSizeNotATty` / `TerminalSizeIoctl`
- `RawModeNotATty` / `RawModeAlreadyActive` / `RawModeNotActive` / `RawModeEnterFailed` / `RawModeLeaveFailed`
- `ReadEventNotInRawMode` / `ReadEventNotATty` / `ReadEventReadFailed` / `ReadEventEof` / `ReadEventInterrupted` / `ReadEventPanic` / `ReadEventResizeInitFailed`
- `WriteFailed` / `WriteBuildValue` / `WritePanic` (5xxx band)
- `RendererInvalidSize` / `RendererOutOfBounds` (6xxx band)
- `WidthInvalidInput` (6101–6103)
- `CursorMoveInvalidPosition`
- `StylizeInvalidColor`

## Development

### Prerequisites

- Rust toolchain (edition 2024)
- [`taida`](https://github.com/taida-lang/taida) CLI (for the facade smoke test)

### Build

```bash
cargo build
```

### Test

```bash
cargo test                 # Rust unit + integration (486 tests as of @a.7)
./scripts/smoke-test.sh    # Taida facade smoke test
```

### Bench

```bash
cargo bench --bench renderer_perf
./scripts/check-bench-budget.sh   # enforces benches/baseline.json budgets
```

### Local `taida-addon` override

Create `.cargo/config.toml`:

```toml
[patch."https://github.com/taida-lang/taida.git"]
taida-addon = { path = "../taida/crates/addon-rs" }
```

## License

MIT
