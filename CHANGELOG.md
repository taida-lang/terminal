# Changelog

All notable changes to `taida-lang/terminal` are documented in this file.

Taida packages use a tag-based release scheme (`@a.1`, `@a.2`, ...). Rust
`Cargo.toml` version is intentionally held at `1.0.0`; the authoritative
release identity is the Taida package tag in `packages.tdm`.

## [@f.9] -- 2026-06-11

Compatibility release for current Taida, plus a hardening pass over the
renderer, input, and Windows console paths driven by a pre-release bug
hunt.

### Fixed

- **The renderer no longer aborts the host process on malformed
  buffers.** `bufferDiff` / `renderFrame` ran their diff + render
  computation outside the FFI panic barrier; a hand-built
  `ScreenBuffer` with negative dimensions panicked there and took the
  whole host process down. The computation now runs inside the
  barrier, and buffer parsing rejects negative dimensions up front as
  `RendererInvalidArg`.
- **Buffer allocation is bounded.** `bufferNew` / `bufferResize` /
  buffer parsing reject more than 500,000 cells with
  `RendererInvalidSize`. Oversized requests previously went straight
  to the allocator, and allocator exhaustion aborts the host process
  with no catchable error — the bound also covers the per-cell pack
  marshalling of buffer return values, so it sits close to real
  terminal sizes rather than merely below the absurd.
- **Width tables now match the pure-Taida reference (UAX #11).**
  Hangul choseong U+1100..U+115F are Wide — they previously fell into
  a combining range and vanished from `bufferWrite` output, so
  NFD-normalised Korean text lost its leading consonants.
  Jungseong/jongseong U+1160..U+11FF are zero-width. The combining
  kana voicing marks are U+3099..U+309A; the stand-alone full-width
  forms U+309B/U+309C are Wide (this pair was off by one). Native and
  the pure `width.td` used by sub-module imports now agree on every
  codepoint.
- `truncateWidth` stops at budget exhaustion: trailing zero-width
  marks are no longer appended once the width budget reaches zero,
  matching the pure-Taida reference behaviour.
- **Wide-pair left-seam defence.** Writing over the trailing
  placeholder of an existing wide-char pair (via `bufferPut` /
  `bufferWrite` / `bufferFillRect` / `bufferBlit`) blanks the
  orphaned lead cell instead of leaving a row that renders one column
  wider than the buffer.
- **The SIGWINCH chain is SA_SIGINFO-safe.** The handler is installed
  with `SA_SIGINFO` and forwards the kernel's real signal info when
  chaining to a previously installed `SA_SIGINFO`-style handler.
  Before, such a predecessor (for example another runtime's handler)
  received a null pointer it is entitled to dereference, crashing the
  process on the first terminal resize. The handler also saves and
  restores `errno`.
- **`readKey` delivers every key under auto-repeat.** The escape
  drain used to read greedily until a 50 ms timeout, concatenating
  repeated arrow-key sequences and silently dropping all but the
  first — with garbage `Char('[')` events appearing at the buffer
  boundary. It now assembles exactly one escape sequence per call,
  using the same structural framing as `readEvent`.
- **Resizes are reported when stdout is redirected.** The resize size
  query now tries stdin → stdout → stderr. Under `app | tee log`, a
  window resize previously surfaced as a spurious
  `ReadEventInterrupted` error instead of a Resize event.
- **Windows console hardening.** The read loops run behind a panic
  barrier (`ReadKeyPanic` / `ReadEventPanic`) with RAII console-mode
  restore, so a panic cannot abort the process or leave the console
  raw. UTF-16 surrogate pairs (emoji and other supplementary-plane
  characters) are assembled into one event instead of being silently
  dropped. `rawModeEnter` now enables window and mouse input reports,
  making `readEvent`'s Resize and Mouse branches reachable on
  Windows. Pack-construction failures surface deterministic errors,
  matching the unix error contract. These fixes are compile-verified
  against the Windows target; a hardware console smoke pass is still
  outstanding.
- Generated API docs no longer attach the renderer functions' error
  tables to `EventKind`: orphaned doc blocks left over from the
  pre-binding facade leaked into the next definition's generated
  documentation and have been removed from the source.

### Changed

- `packages.tdm` now points at `<<<@f.9 taida-lang/terminal`.
- Built and verified against current Taida (the `@f.61` toolchain)
  with taida-addon v2.1.0.
- The facade smoke test now gates its PASS marker on hardening probes
  (negative dimensions, oversized allocation, both expected to raise
  catchable errors) and on UAX #11 width anchors, all executed
  through the real interpreter.

### Known limitations

- The renderer is codepoint-based. Combining marks measure as
  zero-width but are not composed into the preceding cell on write,
  and emoji outside the East Asian Wide ranges count as width 1. NFC
  input renders correctly; NFD accent sequences and ZWJ emoji may
  drop marks at the cell level. This matches the pure-Taida reference
  renderer; a grapheme-cluster-aware renderer would change public
  output and is deferred to a dedicated release.
- Windows virtual-terminal input versus virtual-key interpretation is
  unchanged in this release.

### Compatibility

- The native addon ABI remains ABI v1: `abi = 1`,
  `entry = "taida_addon_get_v1"`. The 23 native function table
  entries keep their names, order, and arities.
- Error names `ReadKeyPanic` / `ReadEventPanic` are now also raised
  on Windows (codes 1008 / 4006 there; unix codes are unchanged).
- Source-compatible with `@f.8` Taida callers.

## [@f.8] -- 2026-05-21

### Changed

- The public Taida facade now exports addon-backed functions directly as
  camelCase function bindings such as `terminalSize`, `readEvent`,
  `bufferWrite`, and `padWidth`. The old PascalCase mold-style function aliases
  are removed.
- Enum-like value packs now expose snake_case variants while keeping the same
  integer tag values. Examples: `KeyKind.enter`, `EventKind.resize`,
  `MouseKind.scroll_down`, `WidthMode.wide`, and `DiffOpKind.move_to`.
- `rawModeEnter()` and `rawModeLeave()` now return a meaningful status pack
  instead of an empty pack: `@(active: Bool)`.
- `PromptOptions.completion` now defaults to `CompletionState`, a concrete
  `@(items, selected, visible)` pack, instead of an empty placeholder pack.
- `packages.tdm` now points at `<<<@f.8 taida-lang/terminal`.

### Added

- Added explicit `RustAddon[...]` facade bindings for every native entry so
  Taida-side parsing, linting, graph extraction, runtime dispatch, and API
  extraction all observe the same public surface.
- Added source `///@` documentation comments to the checked-in `taida/*.td`
  facade sources so generated API docs carry parameter names, return shapes,
  errors, examples, and AI context from the facade itself.

### Fixed

- `readEvent()` pending byte storage is now per OS thread. Concurrent callers no
  longer share surplus bytes from partially decoded escape sequences.
- The Taida facade syntax has been updated to the current chain operators
  (`>=>` / `<=<`) so current Taida can parse, lint, and document the package.

### Compatibility

- The native addon ABI remains ABI v1: `abi = 1`,
  `entry = "taida_addon_get_v1"`.
- The 23 native function table entries keep their existing names, order, and
  arities.
- Existing Rust FFI entry points and integer enum tag values are unchanged.
- The release is source-breaking for Taida callers that still import the old
  PascalCase aliases or old PascalCase enum variants.

### Migration

1. Change `packages.tdm` to `<<<@f.8 taida-lang/terminal`.
2. Replace PascalCase function calls with camelCase calls, for example
   `BufferWrite[buf, 1, 1, "ABC", style]()` becomes
   `bufferWrite(buf, 1, 1, "ABC", style)`.
3. Replace enum variants with snake_case variants, for example
   `KeyKind.Enter` becomes `KeyKind.enter`.
4. Replace empty completion placeholders with `CompletionState`.
5. Use the returned raw-mode status pack if you need to observe raw-mode
   transitions: `rawModeEnter().active == true`,
   `rawModeLeave().active == false`.

## Prior Releases

Earlier `@a.*` releases introduced the terminal package, raw terminal I/O,
event parsing, ANSI helpers, Unicode width helpers, virtual screen buffers, diff
rendering, line-editor state, widgets, and renderer performance improvements.
The public history is preserved in Git tags and release artifacts.
