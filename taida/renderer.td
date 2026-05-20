// taida/renderer.td
//
// Virtual screen buffer + diff engine + renderer.
//
// This file is a **pure-Taida types module** for the renderer. All
// runtime dispatch aliases (`BufferPut`, `BufferWrite`, `BufferNew`,
// `BufferResize`, ...) are defined in `terminal.td` because addon
// sentinels are pre-injected only into the top-level facade's env.
//
// This module defines the pure public pack types. Native implementations live
// behind the top-level addon facade.
//
// This file exports **only** pure-Taida type packs.
// The runtime dispatch is the responsibility of `terminal.td`.

// -- Cell --

///@ Purpose: One cell of data (character + style)
///@ Params:
///@   - text:      Str  — 1 grapheme 相当のテキスト (default " ")
///@   - fg:        Str  — 前景色名 ("" = 色なし)
///@   - bg:        Str  — 背景色名 ("" = 色なし)
///@   - bold:      Bool — 太字
///@   - dim:       Bool — 薄字
///@   - underline: Bool — 下線
///@   - italic:    Bool — イタリック
///@ Returns: Cell ぶちパック
///@ AI-Context: Default text is " ". The renderer normalizes empty
///@   text to " " for rendering. Wide-char placeholder cells also
///@   carry " " as the second-half text. fg/bg は文字列名 (Color パレット
///@   と同様)。256 色 / RGB は v1 の Cell では扱わない (style 引数で stylize* 済み
///@   文字列を bufferWrite に渡す形を推奨)。
///@ AI-Hint: Cell を直接構築するより bufferPut / bufferWrite を使うのが安全。
///@ Example:
///@   c <= Cell(text <= "X", fg <= "red", bg <= "", bold <= true, dim <= false, underline <= false, italic <= false)
Cell <= @(
  text      <= " "
  fg        <= ""
  bg        <= ""
  bold      <= false
  dim       <= false
  underline <= false
  italic    <= false
)

///@ Purpose: Default style options for bufferWrite callers
///@ Params:
///@   - fg:        Str  — 前景色名 ("" = 色なし)
///@   - bg:        Str  — 背景色名 ("" = 色なし)
///@   - bold:      Bool — 太字
///@   - dim:       Bool — 薄字
///@   - underline: Bool — 下線
///@   - italic:    Bool — イタリック
///@ Returns: CellStyle ぶちパック (6 フィールドの subset、Cell とは違って text を含まない)
///@ AI-Context: bufferWrite style arg must be this 6-field shape. Use
///@   `CellStyle(fg <= "red", bg <= "", bold <= false, dim <= false,
///@   underline <= false, italic <= false)` — every field must be present.
///@   1 つでも欠けるとぶちパックの構造的部分型付けで undefined になる。
///@ AI-Hint: bufferWrite の style 引数を組み立てるショートカットとして使う。
///@ Example:
///@   s <= CellStyle(fg <= "cyan", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
///@   buf2 <= bufferWrite(buf, 1, 1, "Title", s)
CellStyle <= @(
  fg        <= ""
  bg        <= ""
  bold      <= false
  dim       <= false
  underline <= false
  italic    <= false
)

// -- ScreenBuffer --

///@ Purpose: Virtual screen buffer (row-major flat cells)
///@ Params:
///@   - cols:           Int      — 列数 (>= 1)
///@   - rows:           Int      — 行数 (>= 1)
///@   - cells:          @[Cell]  — row-major flat 配列 (length = cols*rows)
///@   - cursor_col:     Int      — カーソル列 (1-based)
///@   - cursor_row:     Int      — カーソル行 (1-based)
///@   - cursor_visible: Bool     — カーソル表示
///@ Returns: ScreenBuffer ぶちパック
///@ AI-Context: Use `bufferNew(cols, rows)` to allocate. Direct
///@   construction is allowed but the cells list length must equal
///@   cols*rows or the native renderer rejects the buffer with
///@   `RendererInvalidArg`. Package-level buffer operations use native
///@   primitives such as bufferPut / bufferWrite for hot paths.
///@ AI-Hint: TUI の典型 frame loop:
///@   - prev <= bufferNew(cols, rows)
///@   - loop { next <= compose(prev); frame <= renderFrame(prev, next); write(frame.text); prev <= frame.next }
///@ Example:
///@   buf <= bufferNew(80, 24)
///@   buf2 <= bufferWrite(buf, 1, 1, "Hello", CellStyle)
ScreenBuffer <= @(
  cols           <= 0
  rows           <= 0
  cells          <= @[" "]
  cursor_col     <= 1
  cursor_row     <= 1
  cursor_visible <= true
)

// -- DiffOpKind --

///@ Purpose: Diff operation kind
///@ Params:
///@   - move_to:     Int = 0 — カーソル移動
///@   - write:       Int = 1 — テキスト書き込み
///@   - clear_line:  Int = 2 — 行クリア
///@   - show_cursor: Int = 3 — カーソル表示
///@   - hide_cursor: Int = 4 — カーソル非表示
///@ Returns: DiffOpKind ぶちパック (5 variant 列挙)
///@ AI-Context: Tag values are frozen — the Rust `renderer::diff`
///@   matches against these literals. Renumbering breaks the addon ABI.
///@   variant は snake_case。Int tag は不変。
///@ AI-Hint: bufferDiff の戻り値 ops list の各要素の `kind` フィールドと比較。
///@   直接 0..4 を書かず必ず `DiffOpKind.write` 等を経由する。
///@ Example:
///@   diff <= bufferDiff(prev, next)
///@   diff.ops |== first => first.kind |== DiffOpKind.write => stdout("write op")
DiffOpKind <= @(
  move_to     <= 0
  write       <= 1
  clear_line  <= 2
  show_cursor <= 3
  hide_cursor <= 4
)

// -- DiffOp --

///@ Purpose: A single diff operation
///@ Params:
///@   - kind:  Int       — DiffOpKind タグ
///@   - col:   Int       — 1-based column
///@   - row:   Int       — 1-based row
///@   - text:  Str       — テキスト (write op のみ意味あり)
///@   - style: CellStyle — 装飾 (write op のみ意味あり)
///@ Returns: DiffOp ぶちパック
///@ AI-Context: bufferDiff の出力要素。renderOps / renderFrame で ANSI 文字列に
///@   展開される。ユーザコードは通常直接 DiffOp を構築しない (renderer 内部表現)。
DiffOp <= @(
  kind  <= DiffOpKind.write
  col   <= 1
  row   <= 1
  text  <= ""
  style <= @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
)

// -- Native dispatch --
//
// The native renderer entries (`bufferPut`, `bufferWrite`,
// `bufferFillRect`, `bufferClear`, `bufferDiff`, `renderFull`,
// `renderFrame`, `renderOps`, `bufferBlit`, `bufferNew`,
// `bufferResize`) are pre-injected as addon sentinels **only into the
// package facade env** (`terminal.td`), not into sub-imports like this
// file. Aliasing them under uppercase names happens in `terminal.td`;
// this module exports only the **pure Taida types** (`Cell` /
// `CellStyle` / `ScreenBuffer` / `DiffOpKind` / `DiffOp`) so it can be
// re-imported safely.

<<< @(Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp)
