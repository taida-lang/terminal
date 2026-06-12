# Changelog

All notable changes to `taida-lang/terminal` are documented in this file.

Taida packages use a tag-based release scheme (`@a.1`, `@a.2`, ...). Rust
`Cargo.toml` version is intentionally held at `1.0.0`; the authoritative
release identity is the Taida package tag in `packages.tdm`.

## [@f.10] -- 2026-06-12

Renderer output efficiency, input decoding coverage, and signed
release assets.

### Fixed

- **Wide glyphs no longer shift the rest of the row.** Both the full
  render and diff ops used to emit the wide cell's trailing
  placeholder space as its own cell write; the glyph itself already
  advances the terminal cursor two columns, so everything after it
  in the row drifted one column to the right. The placeholder is now
  consumed with its lead cell everywhere.
- **`readKey` decodes Alt-prefixed sequences.** Terminals send
  Alt+Arrow as `ESC ESC [ A`; the pair used to decode as Unknown with
  the tail surfacing as garbage `Char('[')` / `Char('A')` events. The
  whole prefixed sequence now frames as one event with `alt` set.
- **`readKey` consumes X10 legacy mouse payloads.** A bare `ESC [ M`
  final carries three payload bytes that are not final-byte
  terminated; they used to surface as garbage `Char` events. The
  frame is consumed as one Unknown event (SGR mouse reporting is
  unaffected).
- **Oversized CSI sequences are drained to their final byte** instead
  of leaking their tail into later reads as garbage events.
- **`Ctrl+Space` and `Ctrl+\` `Ctrl+]` `Ctrl+^` `Ctrl+_` are
  detectable.** NUL and the C0 bytes above the letter range now decode
  as `Char` events with `ctrl` set, like every other Ctrl chord.
- **`readEvent` classifies hover motion as `Move`.** Under any-event
  tracking (mode 1003), motion without a button held (`Pb = 35`) used
  to come out as `Drag` with a phantom button 3.
- **Windows: orphan high surrogates surface as U+FFFD** instead of
  being dropped silently (doc parity with the orphan-low path), **a
  mouse event interrupting a surrogate pair no longer destroys the
  pending half** (the interrupting record is replayed on the next
  call), and **legacy conhost QuickEdit is cleared** on the
  mouse-enabled raw mode so mouse input reaches the application
  instead of the console's selection gesture.
- **Pack-construction failures no longer leak their children.** Every
  entry point now rolls back the child values when the host fails to
  assemble the return pack, matching the contract the Windows key path
  already followed.
- SIGWINCH handler reads of the self-pipe fd and the saved old-handler
  pointer upgraded from `Relaxed` to `Acquire`; the errno slot
  accessor covers the BSDs (`__error` / `__errno`).

### Changed

- **Styled output is run-length coalesced.** Contiguous same-style
  cells render as one `<SGR>text<reset>` run instead of per-cell SGR
  pairs — a fully styled screen costs roughly a ninth of the bytes,
  which matters over ssh and on slow terminals.
- **`bufferDiff` coalesces adjacent same-style changes into a single
  `Write` op**, and a wide lead + placeholder pair diffs as one unit
  whose op carries the lead glyph only. A single changed cell still
  produces a single op; code that assumed one op per changed cell for
  multi-cell edits should consume ops by their `col`/`row`/`text`
  fields rather than by count.

### Added

- **Release assets are signed.** Every asset (cdylibs, lockfile
  fragments, `SHA256SUMS`) ships with a Sigstore keyless
  `.cosign.bundle` produced by the release workflow via GitHub OIDC
  — `taida ingot install`'s signature verification now reports
  verified instead of warning about a missing bundle.

### Compatibility

- ABI unchanged (`abi = 1`). No source-breaking facade changes. The
  ANSI byte stream emitted for styled or wide-glyph content differs
  from `@f.9` as described above; tests that assert exact ANSI output
  may need their expectations refreshed.

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
