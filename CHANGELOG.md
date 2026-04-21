# Changelog

All notable changes to `taida-lang/terminal` are documented in this file.

Taida packages use a tag-based release scheme (`@a.1`, `@a.2`, ...). Rust
`Cargo.toml` version is intentionally held at `1.0.0` — the authoritative
release identity is the Taida package tag in `packages.tdm`.

## [@a.3] — 2026-04-21

### Added
- **`Write[](bytes: Str) -> Int`** — TUI 用の改行なし即時 write path
  (TMB-016). `io::stdout().write_all + flush` で 1 バイトも蓄積せず端末に
  書き出し、書き込んだバイト数 (`Int`) を返す。non-tty (pipe / redirect)
  でも成功経路を維持し、`catch_unwind` で FFI unwind を封止する。ANSI
  エスケープを連続送信する描画用途で使う契約。addon function table は
  append-only で 6 → 7 entries に拡張（既存 4 entry `terminalSize` /
  `readKey` / `isTerminal` / `readEvent` / `rawModeEnter` / `rawModeLeave`
  の ABI 不変）。
- 5xxx エラー帯: `WriteFailed` / `WriteBuildValue` / `WritePanic`。
- 公開ドキュメント整備 (TMB-018): `README.md` に Write の usage / Exports
  59 → 60 / error variants / test count 340 → 360 を反映、`docs/api.md` の
  `terminal.td` export list と binding 節に `Write` エントリ追加
  (signature / throws / UTF-8 byte-count contract / non-tty 成功経路 /
  since `@a.3`)。

### Fixed
- **SIGWINCH handler install 順序 race** (TMB-017, TMB-007 follow-up) —
  `ensure_sigwinch_pipe()` の install 順序を再構成。旧 `sigaction(SIGWINCH,
  &sa, &mut old_sa)` → `Box::new(old_sa)` → `OLD_SIGWINCH.store(...)` の順
  では、ステップ 1 直後〜3 直前の race window で SIGWINCH が到達すると
  自前 `sigwinch_handler` が `OLD_SIGWINCH.load()` で null を得て、
  他ライブラリの old handler へのチェーンを silently skip していた。
  新順序: `sigaction(SIGWINCH, NULL, &mut old_sa)` で先に現 handler を
  取得 → `OLD_SIGWINCH.store(..., Release)` → `sigaction(SIGWINCH, &sa,
  NULL)` で新 handler を install → `SIGWINCH_INSTALLED.store(true,
  Release)`。race window を物理的に消去しつつ TMB-007 の chain 契約
  (SIG_DFL / SIG_IGN はスキップ) を維持。

### Tests
- `tests/sigwinch_install_order.rs`: 順序 pin + 再入冪等 + pure-probe
  idempotence (3 tests).
- `tests/sigwinch_external_chain.rs` (新規、strong-path 専用バイナリ):
  external handler を先に install → addon install → 実 SIGWINCH self-
  delivery で self-pipe byte と external counter 双方が +1 されることを
  assertion で pin。フレッシュプロセスで実行されるため probe の副作用で
  強い経路が短絡する問題を回避。
- `tests/write_returns_byte_count.rs` / `write_non_tty.rs` /
  `write_arity_mismatch.rs` および `src/write.rs` 内 unit tests。
- `cargo test`: **366 PASS / 0 failed** (pre-release `@a.2`: 340 PASS)。
- `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo
  check --target x86_64-pc-windows-msvc`: all clean。

### Internal
- `src/write.rs` 新設。`src/lib.rs` dispatcher に Write entry を append。
- `src/event.rs` に install 順序 reorder および `__test_only_sigwinch_pure_probe()`
  (副作用ゼロ probe) を追加。`src/lib.rs` で `__test_only::sigwinch_pure_probe()`
  として test-only 再 export。`TERMINAL_FUNCTIONS` / 公開 ABI には影響なし。

### Upgrade notes
- 既存利用者への破壊的変更はなし。`Write` は新規 export。
- TUI を実装する際は `stdout()` (taida-lang 本体、`\n` 暗黙付与の行指向
  I/O) ではなく `Write()` (本 package) を使うこと。contract は
  `docs/api.md` 参照。

## [@a.2] — 2026-04-16

- facade 5 ファイルの discard binding リネーム (TMB-015)
- `addon.toml` の `[library.prebuild.targets]` を撤去し `addon.lock.toml`
  fallback へ (C14B-012 経由で taida 本体が対応)
- a.1 release asset から正しい SHA-256 を handback (TMB-014)
- CI を C14 release.yml ワークフローへ移行

## [@a.1] — 2026-04-10

初回リリース。`terminalSize` / `readKey` / `isTerminal` / `readEvent` の
4 entry と raw mode / ANSI style / unicode width / prompt / renderer /
widgets の Taida facade を含む。
