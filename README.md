# taida-lang/terminal

Taida Lang official terminal package — TTY detection, size query, key / event input, raw mode, screen / cursor control, ANSI styling (16 / 256 / RGB), Unicode width, virtual buffer with diff renderer, line editor, and UX widgets.

- **Release**: `@f.9`
- **Backend**: Native-only addon (Rust `cdylib` + Taida facade). The interpreter dispatches to the cdylib through addon ABI v1.
- **Exports**: 61 public symbols (see below).

## Install

Declare the dependency in your project's `packages.tdm` with the release
version and the SHA-256 of its source archive:

```toml
[packages."taida-lang/terminal"]
version = "f.9"
integrity = "sha256:af24f2966042e61397b90c7311b2edb0904e2abb1d303e36e4b6dda977358c24"
```

Then install:

```bash
taida ingot install
```

The `integrity` value is the SHA-256 of
`https://github.com/taida-lang/terminal/archive/refs/tags/f.9.tar.gz`
(compute it with `curl -sL <url> | sha256sum` to verify independently).
The native cdylib for your platform is fetched from the GitHub release
for the same tag and verified against the published SHA-256 digests
automatically.

## Usage

Import the subset you need from the facade:

```taida
>>> taida-lang/terminal => @(
  isTerminal, terminalSize,
  readKey, KeyKind,
  rawModeEnter, rawModeLeave,
  readEvent, EventKind, MouseKind,
  write,
  clearScreen, clearLine,
  altScreenEnter, altScreenLeave,
  cursorMoveTo, cursorHide, cursorShow,
  mouseTrackingEnter, mouseTrackingLeave,
  stylize, Color, resetStyle,
  stylize256, Color256, stylizeRgb, ColorRgb,
  displayWidth, measureGrapheme, padWidth, truncateWidth, normalizeCellText, WidthMode,
  Cell, CellStyle, ScreenBuffer,
  bufferNew, bufferResize, bufferClear, bufferPut, bufferWrite,
  bufferFillRect, bufferBlit, bufferDiff,
  renderFull, renderOps, renderFrame,
  DiffOpKind, DiffOp,
  PromptMode, PromptOptions, CompletionState,
  LineEditorAction, LineEditorState,
  lineEditorNew, lineEditorStep, lineEditorRender,
  SpinnerState, spinnerNext, spinnerRender,
  ProgressOptions, progressBar, statusLine
)
```

**Call convention.** Public functions use normal function-call syntax
`name(...)`. The old PascalCase mold-call aliases were removed in
`@f.8` to comply with the current Taida naming rules.

### TTY detection and terminal size

```taida
| isTerminal("stdout") |> stdout(terminalSize().cols.toString() + "x" + terminalSize().rows.toString())
```

`|` is the guard operator; `|>` separates the condition from a single
body expression. When you need a multi-arm conditional, wrap it in
`(...)` with a `| _ |> ...` fallback. See
`docs/guide/07_control_flow.md` for full grammar.

### Single key read

```taida
key <= readKey()

(
  | key.kind == KeyKind.escape |> stdout("Escaped!")
  | key.kind == KeyKind.enter  |> stdout("Submitted")
  | key.kind == KeyKind.char   |> stdout("typed: " + key.text)
  | _                          |> stdout("other: " + key.kind.toString())
)
```

### Persistent raw mode + unified events

```taida
rawModeEnter()
write(mouseTrackingEnter())

event <= readEvent()
(
  | event.kind == EventKind.key    |> stdout("key: " + event.key.text)
  | event.kind == EventKind.mouse  |> stdout("click at " + event.mouse.col.toString())
  | event.kind == EventKind.resize |> stdout("resize: " + event.resize.cols.toString())
  | _                              |> stdout("unknown event")
)

write(mouseTrackingLeave())
rawModeLeave()
```

### ANSI strings (pure helpers, no side effects)

```taida
write(clearScreen())
write(cursorHide())
write(cursorMoveTo(10, 5))
write("hello")
write(cursorShow())
```

`write(bytes)` writes to stdout without appending a newline — use it
when `stdout()`'s implicit `\n` would corrupt ANSI framing (cursor
moves, partial redraws, spinner ticks, progress updates).

### Styling (16 / 256 / RGB)

Style packs require all six fields (`fg`, `bg`, `bold`, `dim`,
`underline`, `italic`). Use `""` for unset color and `false` for unset
attributes. Call arguments must fit on a single line.

```taida
red <= stylize("hello", @(fg <= Color.red, bg <= "", bold <= true, dim <= false, underline <= false, italic <= false))
stdout(red)

orange <= stylize256("256", @(fg <= Color256(index <= 208), bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
stdout(orange)

rgb <= stylizeRgb("rgb", @(fg <= ColorRgb(r <= 255, g <= 128, b <= 0), bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
stdout(rgb)

stdout(resetStyle())
```

### Unicode display width

```taida
stdout(displayWidth("hello").toString())     // 5
stdout(displayWidth("漢字").toString())       // 4
stdout(padWidth("hi", 5))                    // "hi   "
stdout(truncateWidth("abcdef", 3))           // "abc"

m <= measureGrapheme("漢")
stdout(m.width.toString() + " / mode=" + m.mode.toString())
```

### Virtual buffer + diff renderer

```taida
plain <= CellStyle(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)

prev <= bufferNew(20, 5)
next <= bufferWrite(prev, 1, 1, "Hello, world", plain)

frame <= renderFrame(prev, next)
write(frame.text)
// feed frame.next as the next `prev` to continue the diff chain
```

`renderFrame(prev, next)` returns `@(text, next)` with the minimal ANSI
diff; on size change it falls back to `renderFull(next)`. `bufferNew`
and `bufferResize` are native; allocation runs in Rust to avoid the
pure-Taida repeated-append hot path.

### Line editor (pure state machine)

Taida forbids rebinding an already-defined name in the same scope, so
each editor transition introduces a new name (`editor0`, `editor1`, …).

```taida
opts <= PromptOptions(prompt <= "> ", initial <= "", placeholder <= "type here", mode <= PromptMode.normal, history <= @[], completion <= CompletionState)

editor0 <= lineEditorNew(opts)

rawModeEnter()
editor1 <= lineEditorStep(editor0, readEvent())
rawModeLeave()

view <= lineEditorRender(editor1)
write(view.line)
```

`lineEditorStep` is pure — it takes the current state and one event, and
returns the next state. For a full event loop, write a recursive helper
that threads the editor state through each recursion step.

### UX widgets

```taida
stdout(progressBar(50, 100, ProgressOptions))
stdout(statusLine("left", "right", 40))

sp  <= SpinnerState
sp2 <= spinnerNext(sp)
stdout(spinnerRender(sp2))
```

## Exports (61 symbols)

### Native entries (Rust addon, call as `name(...)`)

| Symbol | Signature | Description |
|--------|-----------|-------------|
| `isTerminal` | `(stream: Str) -> Bool` | stdin / stdout / stderr TTY check |
| `terminalSize` | `() -> @(cols: Int, rows: Int)` | both fields >= 1 |
| `readKey` | `() -> @(kind, text, ctrl, alt, shift)` | single key read; manages raw mode for one call |
| `rawModeEnter` | `() -> @(active: Bool)` | enter raw mode; returns `active=true` on success |
| `rawModeLeave` | `() -> @(active: Bool)` | leave raw mode; returns `active=false` on success |
| `readEvent` | `() -> @(kind, key, mouse, resize)` | unified event — **raw mode required** |
| `write` | `(bytes: Str) -> Int` | unbuffered stdout write, returns byte count, no implicit `\n` |
| `bufferNew` | `(cols: Int, rows: Int) -> ScreenBuffer` | allocate a fresh buffer |
| `bufferResize` | `(buf, cols, rows, fill?) -> ScreenBuffer` | reallocate, clamp cursor to new bounds |
| `measureGrapheme` | `(text: Str) -> @(width, mode)` | single grapheme width + `WidthMode` tag |
| `displayWidth` | `(text: Str) -> Int` | total display width (cells) |
| `normalizeCellText` | `(text: Str) -> Str` | empty -> space, strip control chars, TAB -> 4 spaces |
| `truncateWidth` | `(text: Str, width: Int) -> Str` | right-edge truncation, wide-char aware |
| `padWidth` | `(text: Str, width: Int) -> Str` | right-pad with spaces |

### Pure-Taida facades (call as `Name(...)`)

#### ANSI control

| Symbol | Returns |
|--------|---------|
| `clearScreen` | `"\x1b[2J\x1b[H"` |
| `clearLine` | `"\x1b[2K\r"` |
| `altScreenEnter` | `"\x1b[?1049h"` |
| `altScreenLeave` | `"\x1b[?1049l"` |
| `cursorMoveTo` | `(col, row) -> "\x1b[{row};{col}H"` (1-based; throws `CursorMoveInvalidPosition` on `< 1`) |
| `cursorHide` | `"\x1b[?25l"` |
| `cursorShow` | `"\x1b[?25h"` |
| `mouseTrackingEnter` | SGR 1006 + button + motion enable |
| `mouseTrackingLeave` | SGR 1006 + button + motion disable |

#### Styling

| Symbol | Description |
|--------|-------------|
| `Color` | 16-color palette pack (`Color.red`, `Color.bright_white`, …) |
| `stylize` | `(text, @(fg, bg, bold, dim, underline, italic)) -> Str` |
| `Color256` | `@(index: Int)` — 0–255 |
| `stylize256` | 256-color variant |
| `ColorRgb` | `@(r, g, b: Int)` — each 0–255 |
| `stylizeRgb` | truecolor variant |
| `resetStyle` | `"\x1b[0m"` |

Style packs require all six fields (`fg`, `bg`, `bold`, `dim`,
`underline`, `italic`). Use `""` for unset color and `false` for unset
attributes.

#### Width

| Symbol | Description |
|--------|-------------|
| `WidthMode` | enum pack — `narrow` = 0, `wide` = 1, `zero` = 2, `ambiguous` = 3 |

(The five width helpers — `measureGrapheme`, `displayWidth`,
`normalizeCellText`, `truncateWidth`, `padWidth` — are dispatched to
native and listed above.)

#### Virtual buffer + renderer

| Symbol | Description |
|--------|-------------|
| `Cell` | `@(text, fg, bg, bold, dim, underline, italic)` |
| `CellStyle` | `@(fg, bg, bold, dim, underline, italic)` — helper pack for style args |
| `ScreenBuffer` | `@(cols, rows, cells, cursor_col, cursor_row, cursor_visible)` |
| `DiffOpKind` | enum pack — `move_to`, `write`, `clear_line`, `show_cursor`, `hide_cursor` |
| `DiffOp` | `@(kind, col, row, text, style)` |
| `bufferClear` | `(buf, fill?) -> ScreenBuffer` |
| `bufferPut` | `(buf, col, row, cell) -> ScreenBuffer` |
| `bufferWrite` | `(buf, col, row, text, style?) -> ScreenBuffer` — width-aware, right-edge truncation |
| `bufferFillRect` | `(buf, col, row, width, height, cell) -> ScreenBuffer` |
| `bufferBlit` | `(main, sub, col, row) -> ScreenBuffer` — composite `sub` at `(col, row)`, clips overflow, drops half wide-chars at right edge |
| `renderFull` | `(buf) -> Str` — full redraw |
| `bufferDiff` | `(prev, next) -> @(ops, requires_full)` |
| `renderOps` | `(ops) -> Str` — diff ops to ANSI string |
| `renderFrame` | `(prev, next) -> @(text, next)` — minimal diff or full fallback |

#### Line editor (pure state machine)

| Symbol | Description |
|--------|-------------|
| `PromptMode` | enum pack — `normal`, `password` |
| `PromptOptions` | `@(prompt, initial, placeholder, mode, history, completion)` |
| `CompletionState` | `@(items, selected, visible)` |
| `LineEditorAction` | enum pack — `editing`, `submitted`, `cancelled` |
| `LineEditorState` | full editor state pack |
| `lineEditorNew` | `(opts) -> LineEditorState` |
| `lineEditorStep` | `(state, event) -> LineEditorState` — pure transition |
| `lineEditorRender` | `(state) -> @(text, cursor_col)` |

#### UX widgets

| Symbol | Description |
|--------|-------------|
| `SpinnerState` | `@(frame, label, done)` |
| `spinnerNext` | `(state) -> SpinnerState` |
| `spinnerRender` | `(state) -> Str` |
| `ProgressOptions` | `@(width, complete_char, incomplete_char, left_label, right_label)` |
| `progressBar` | `(current, total, opts?) -> Str` |
| `statusLine` | `(left, right?, width?) -> Str` |

#### Enums (value packs)

| Symbol | Description |
|--------|-------------|
| `KeyKind` | 28 variants — `char`, `enter`, `escape`, `tab`, `backspace`, `delete`, `arrow_up/down/left/right`, `home`, `end`, `page_up/down`, `insert`, `f1`-`f12`, `unknown` |
| `EventKind` | `key`, `mouse`, `resize`, `unknown` |
| `MouseKind` | `down`, `up`, `move`, `drag`, `scroll_up`, `scroll_down` |

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
cargo test                 # Rust unit + integration tests
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
