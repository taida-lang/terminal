# Changelog

All notable changes to `taida-lang/terminal` are documented in this file.

Taida packages use a tag-based release scheme (`@a.1`, `@a.2`, ...). Rust
`Cargo.toml` version is intentionally held at `1.0.0`; the authoritative
release identity is the Taida package tag in `packages.tdm`.

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
