# taida-lang/terminal

Taida Lang official terminal package -- TTY detection, size query, key/event input, raw mode, screen/cursor control, ANSI styling (16/256/RGB), Unicode width, virtual buffer with diff renderer, line editor, and UX widgets.

## Install

```bash
taida install taida-lang/terminal
```

## Usage

```taida
>>> taida-lang/terminal => @(
  IsTerminal, TerminalSize, ReadKey, KeyKind,
  RawModeEnter, RawModeLeave, ReadEvent, EventKind, MouseKind,
  ClearScreen, CursorMoveTo, CursorHide, CursorShow,
  AltScreenEnter, AltScreenLeave, MouseTrackingEnter, MouseTrackingLeave,
  Stylize, Color, Stylize256, Color256, StylizeRgb, ColorRgb, ResetStyle,
  DisplayWidth, BufferNew, RenderFull, RenderFrame,
  LineEditorNew, LineEditorStep, LineEditorRender, SpinnerState, SpinnerNext, SpinnerRender, ProgressBar
)
```

### Quick examples

```taida
// TTY detection + terminal size
IsTerminal[]("stdout") |== true => stdout(TerminalSize[]().cols.toString())

// Single key read
key <= ReadKey[]()
key.kind |== KeyKind.Escape => stdout("Escaped!")

// Persistent raw mode + unified events
RawModeEnter[]()
stdout(MouseTrackingEnter[]())
event <= ReadEvent[]()
event.kind |== EventKind.Key    => stdout(event.key.text)
event.kind |== EventKind.Mouse  => stdout("click at " + event.mouse.col.toString())
event.kind |== EventKind.Resize => stdout("new size: " + event.resize.cols.toString())
stdout(MouseTrackingLeave[]())
RawModeLeave[]()

// Screen control (returns ANSI strings, you decide when to write)
stdout(ClearScreen[]())
stdout(CursorMoveTo[](10, 5))

// Styled text (16 / 256 / RGB)
stdout(Stylize[]("hello", @(fg <= Color.red, bold <= true)))
stdout(Stylize256[]("256", @(fg <= Color256(index <= 208))))
stdout(StylizeRgb[]("rgb", @(fg <= ColorRgb(r <= 255, g <= 128, b <= 0))))
```

## Exports (59 symbols)

### Core (addon)

| Symbol | Description |
|--------|-------------|
| `IsTerminal` | `(stream) -> Bool` -- stdin/stdout/stderr TTY check |
| `TerminalSize` | `() -> @(cols, rows)` -- both >= 1 |
| `ReadKey` | `() -> @(kind, text, ctrl, alt, shift)` -- single key read |
| `KeyKind` | 28-variant enum: Char, Enter, Escape, Tab, ..., F1-F12, Unknown |
| `RawModeEnter` | `() -> @()` -- enter raw mode |
| `RawModeLeave` | `() -> @()` -- leave raw mode |
| `ReadEvent` | `() -> @(kind, key, mouse, resize)` -- unified event (raw mode required) |
| `EventKind` | Key / Mouse / Resize / Unknown |
| `MouseKind` | Down / Up / Move / Drag / ScrollUp / ScrollDown |

### ANSI Facade (pure Taida)

| Symbol | Returns |
|--------|---------|
| `ClearScreen` | `"\x1b[2J\x1b[H"` |
| `ClearLine` | `"\x1b[2K\r"` |
| `AltScreenEnter` | `"\x1b[?1049h"` |
| `AltScreenLeave` | `"\x1b[?1049l"` |
| `CursorMoveTo` | `"\x1b[{row};{col}H"` (1-based) |
| `CursorHide` | `"\x1b[?25l"` |
| `CursorShow` | `"\x1b[?25h"` |
| `MouseTrackingEnter` | SGR 1006 enable sequence |
| `MouseTrackingLeave` | SGR 1006 disable sequence |

### Styling (pure Taida)

| Symbol | Description |
|--------|-------------|
| `Stylize` | 16-color styling with bold/dim/italic/underline |
| `Color` | 16-color palette (black..white, bright_black..bright_white) |
| `Stylize256` | 256-color styling |
| `Color256` | `@(index)` -- 0-255 |
| `StylizeRgb` | RGB truecolor styling |
| `ColorRgb` | `@(r, g, b)` -- 0-255 each |
| `ResetStyle` | `"\x1b[0m"` |

### Unicode Width (pure Taida)

| Symbol | Description |
|--------|-------------|
| `WidthMode` | Narrow / Wide / Zero / Ambiguous |
| `MeasureGrapheme` | `(text) -> @(width, mode)` |
| `DisplayWidth` | `(text) -> Int` |
| `NormalizeCellText` | `(text) -> Str` -- empty -> space, strip control chars |
| `TruncateWidth` | `(text, width) -> Str` |
| `PadWidth` | `(text, width) -> Str` |

### Virtual Buffer / Renderer (pure Taida)

| Symbol | Description |
|--------|-------------|
| `Cell` | `@(text, fg, bg, bold, dim, underline, italic)` |
| `ScreenBuffer` | `@(cols, rows, cells, cursor_col, cursor_row, cursor_visible)` |
| `DiffOpKind` | MoveTo / Write / ClearLine / ShowCursor / HideCursor |
| `DiffOp` | `@(kind, col, row, text, style)` |
| `BufferNew` | `(cols, rows) -> ScreenBuffer` |
| `BufferResize` | `(buf, cols, rows, fill?) -> ScreenBuffer` |
| `BufferClear` | `(buf, fill?) -> ScreenBuffer` |
| `BufferPut` | `(buf, col, row, cell) -> ScreenBuffer` |
| `BufferWrite` | `(buf, col, row, text, style?) -> ScreenBuffer` |
| `BufferFillRect` | `(buf, col, row, width, height, cell) -> ScreenBuffer` |
| `RenderFull` | `(buf) -> Str` -- full screen render |
| `BufferDiff` | `(prev, next) -> @(ops, requires_full)` |
| `RenderOps` | `(ops) -> Str` -- diff ops to ANSI string |
| `RenderFrame` | `(prev, next) -> @(text, next)` -- smart render |

### Line Editor (pure Taida state machine)

| Symbol | Description |
|--------|-------------|
| `PromptMode` | Normal / Password |
| `PromptOptions` | `@(prompt, initial, placeholder, mode, history, completion)` |
| `CompletionState` | `@(items, selected, visible)` |
| `LineEditorAction` | Editing / Submitted / Cancelled |
| `LineEditorState` | Full editor state pack |
| `LineEditorNew` | `(opts) -> LineEditorState` |
| `LineEditorStep` | `(state, event) -> LineEditorState` -- pure state transition |
| `LineEditorRender` | `(state) -> @(text, cursor_col)` |

### UX Widgets (pure Taida)

| Symbol | Description |
|--------|-------------|
| `SpinnerState` | `@(frame, label, done)` |
| `SpinnerNext` | `(state) -> SpinnerState` |
| `SpinnerRender` | `(state) -> Str` |
| `ProgressOptions` | `@(width, complete_char, incomplete_char, left_label, right_label)` |
| `ProgressBar` | `(current, total, opts?) -> Str` |
| `StatusLine` | `(left, right?, width?) -> Str` |

### Error variants

Functions throw deterministic errors (no silent fallbacks):

- `IsTerminalInvalidStream`
- `ReadKeyNotATty` / `ReadKeyRawMode` / `ReadKeyEof` / `ReadKeyInterrupted` / `ReadKeyUnsupported`
- `TerminalSizeNotATty` / `TerminalSizeIoctl`
- `RawModeNotATty` / `RawModeAlreadyActive` / `RawModeNotActive` / `RawModeEnterFailed` / `RawModeLeaveFailed`
- `ReadEventNotInRawMode` / `ReadEventNotATty` / `ReadEventReadFailed` / `ReadEventEof` / `ReadEventInterrupted` / `ReadEventPanic` / `ReadEventResizeInitFailed`
- `CursorMoveInvalidPosition`
- `StylizeInvalidColor`

## Development

### Prerequisites

- Rust toolchain (edition 2024)
- [taida](https://github.com/taida-lang/taida) CLI (for facade smoke test)

### Build

```bash
cargo build
```

### Test

```bash
cargo test                    # Rust unit + integration tests (340)
./scripts/smoke-test.sh       # Taida facade smoke test
```

### Local taida-addon override

Create `.cargo/config.toml`:

```toml
[patch."https://github.com/taida-lang/taida.git"]
taida-addon = { path = "../taida/crates/addon-rs" }
```

## License

MIT
