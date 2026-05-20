// taida/width.td
//
// Unicode width calculation for terminal rendering.
//
// This is the pure renderer foundation:
//   - No syscalls
//   - No hidden side effects
//   - Pure computation on strings
//
// Width policy:
//   1. ASCII printable (U+0020..U+007E) -> width 1
//   2. Combining mark -> width 0
//   3. East Asian Wide / Fullwidth -> width 2
//   4. Ambiguous -> width 1 (v1 decision)
//   5. TAB -> renderer layer forbids; pre-expand to spaces
//   6. Newline -> forbidden in cell text
//
// History:
//   - Initial pure-Taida implementation on top of CharAt /
//     CodePoint. Functionally correct but O(N²) because CharAt is
//     O(idx) in the interpreter and `acc + ch` re-allocates per step.
//   - Native implementation behind the same facade
//     names. The addon entries `measureGrapheme` / `displayWidth` /
//     `normalizeCellText` / `truncateWidth` / `padWidth` are
//     pre-injected as sentinels *only* into `terminal.td`'s env, so
//     the package-level public surface uses the native fast path. The
//     pure-Taida implementations below are kept so that **sub-module
//     sub-imports** (`widgets.td`, `prompt.td`) continue to type-check
//     and run without requiring a sentinel. Downstream code that
//     imports `taida-lang/terminal` goes through `terminal.td` and
//     therefore picks up the native alias.
//
//   - F generation release: variant fields are snake_case, functions are
//     camelCase, and private helper names avoid leading underscores.
//
// Sub-module callers (`widgets.td`, `prompt.td`) still see the
// pure-Taida fallback — typical inputs are short (< 200 chars) so the
// O(N²) cost is bounded and does not regress Hachikuma's frame budget.

// -- Width Mode Enum --

///@ Purpose: Unicode width category enum pack
///@ AI-Context: Compare with measureGrapheme result `mode` field.
///@   Tag values are frozen — the Rust `width.rs` matches against these
///@   literals (narrow=0, wide=1, zero=2, ambiguous=3). 並び替え禁止 (ABI lock)。
///@   variant は snake_case。Int tag は不変。
///@ AI-Hint: `m.mode == WidthMode.wide` のように比較する。直接 0/1/2/3 を
///@   書くのは禁止 (lock 後の変更を吸収しないため)。
///@ Example:
///@   m <= measureGrapheme("あ")
///@   m.mode |== WidthMode.wide => stdout("wide char (width 2)")
WidthMode <= @(
  narrow    <= 0
  wide      <= 1
  zero      <= 2
  ambiguous <= 3
)

// -- Internal helpers --

// Range check: lo < cp and cp < hi+1 (inclusive range)
inRange cp lo hi =
  | cp < lo |> false
  | cp > hi |> false
  | _ |> true
=> :Bool

// -- Internal: codepoint range checks --

// Combining marks: width 0
isCombining cp =
  | inRange(cp, 768, 879) == true   |> true
  | inRange(cp, 4448, 4607) == true |> true
  | inRange(cp, 6832, 6911) == true |> true
  | inRange(cp, 7616, 7679) == true |> true
  | inRange(cp, 8400, 8447) == true |> true
  | inRange(cp, 65056, 65071) == true |> true
  | inRange(cp, 65024, 65039) == true |> true
  | cp == 65279 |> true
  | cp == 8203  |> true
  | cp == 8204  |> true
  | cp == 8205  |> true
  | inRange(cp, 12441, 12442) == true  |> true
  | inRange(cp, 917760, 917999) == true |> true
  | _ |> false
=> :Bool

// East Asian Wide / Fullwidth: width 2
isWide cp =
  | inRange(cp, 11904, 42191) == true  |> true
  | inRange(cp, 44032, 55203) == true  |> true
  | inRange(cp, 63744, 64255) == true  |> true
  | inRange(cp, 65072, 65103) == true  |> true
  | inRange(cp, 65281, 65376) == true  |> true
  | inRange(cp, 65504, 65510) == true  |> true
  | inRange(cp, 4352, 4447) == true    |> true
  | inRange(cp, 131072, 262141) == true |> true
  | _ |> false
=> :Bool

// Control characters: width 0
isControl cp =
  | cp < 32   |> true
  | cp == 127 |> true
  | inRange(cp, 128, 159) == true |> true
  | _ |> false
=> :Bool

// -- measureGrapheme --

///@ Purpose: Measure the display width and category of a single grapheme
///@ Params:
///@   - text: Str — the text to measure (only the first grapheme is measured)
///@ Returns: @(width: Int, mode: Int) — width はセル数 (0/1/2)、mode は WidthMode タグ
///@ Example:
///@   m <= measureGrapheme("a")    // @(width <= 1, mode <= WidthMode.narrow)
///@   m <= measureGrapheme("あ")   // @(width <= 2, mode <= WidthMode.wide)
///@   m <= measureGrapheme("")     // @(width <= 0, mode <= WidthMode.zero)
///@ AI-Context: width policy:
///@   - ASCII printable (U+0020..U+007E) → width 1 / narrow
///@   - 結合マーク / 制御文字 → width 0 / zero
///@   - East Asian Wide / Fullwidth → width 2 / wide
///@   - Ambiguous → width 1 / narrow (v1 決定、ja/zh ロケールでも narrow 扱い)
///@   package-level import では native fast-path に切り替わり、本 facade は
///@   sub-import 用 fallback。
///@ AI-Hint: 文字列全体の幅は displayWidth を使う。本関数は **first grapheme のみ**。
measureGrapheme text =
  If[text == "", @(width <= 0, mode <= WidthMode.zero), measureGraphemeInner(text)]()
=> :@(width: Int, mode: Int)

measureGraphemeInner text =
  ch <=< CharAt[text, 0]()
  cp <=< CodePoint[ch]()
  | isControl(cp) == true  |> @(width <= 0, mode <= WidthMode.zero)
  | isCombining(cp) == true |> @(width <= 0, mode <= WidthMode.zero)
  | isWide(cp) == true     |> @(width <= 2, mode <= WidthMode.wide)
  | _ |> @(width <= 1, mode <= WidthMode.narrow)
=> :@(width: Int, mode: Int)

// -- displayWidth --

///@ Purpose: Calculate the total display width (cell count) of a string
///@ Params:
///@   - text: Str — measure 対象テキスト (multi-byte / wide char 混在 OK)
///@ Returns: Int — display width in cells (0 以上)
///@ Example:
///@   displayWidth("hello")        // 5
///@   displayWidth("あいう")        // 6 (wide × 3)
///@   displayWidth("aあb")         // 4 (1 + 2 + 1)
///@   displayWidth("")             // 0
///@ AI-Context: Hot path. package-level import では native に切替済、本 facade は
///@   `widgets.td` / `prompt.td` の sub-import 経由で pure-Taida fallback として
///@   利用される。typical inputs は短い (< 200 chars) ため O(N²) でも frame
///@   budget を超えない。
///@ AI-Hint: 表示幅 = ターミナルセル数。byte 数や char 数とは異なる。
///@   切り詰めには truncateWidth、右側パディングには padWidth を使う。
// Combined loop + step in single function to avoid forward references
// in the facade module loader (which evaluates top-to-bottom without hoisting).
dwCalc src idx acc len =
  | idx == len |> acc
  | _ |>
    CharAt[src, idx]() >=> ch
    CodePoint[ch]() >=> cp
    step <= If[cp > 65535, 2, 1]()
    m <= measureGrapheme(ch)
    dwCalc(src, idx + step, acc + m.width, len)
=> :Int

displayWidth text =
  If[text == "", 0, dwCalc(text, 0, 0, text.length())]()
=> :Int

// -- normalizeCellText --

///@ Purpose: Normalize cell text (TAB -> 4 spaces, newline -> strip, empty -> " ")
///@ Params:
///@   - text: Str — 正規化対象テキスト
///@ Returns: Str — 正規化済みテキスト (renderer Cell.text 制約を満たす)
///@ Example:
///@   normalizeCellText("a\tb")    // "a    b"  (TAB → 4 spaces)
///@   normalizeCellText("a\nb")    // "ab"      (newline 除去)
///@   normalizeCellText("")        // " "       (空文字 → 単一空白)
///@ AI-Context: ScreenBuffer の Cell.text は 1 grapheme 相当の文字列を期待し、
///@   newline / TAB / 空文字を含むと renderer がカーソル位置を誤計算する。
///@   bufferWrite が呼び出す前段でこの正規化を通すと安全。
///@ AI-Hint: 通常 bufferWrite 内部で呼ばれるため、ユーザコードで直接呼ぶ
///@   必要はあまりない。 raw 描画 (bufferPut) で wide char placeholder を
///@   構築するときに使う。
normLoop src idx acc len =
  | idx == len |> acc
  | _ |>
    CharAt[src, idx]() >=> ch
    nextAcc <= (
      | ch == "\t" |> acc + "    "
      | ch == "\n" |> acc
      | ch == "\r" |> acc
      | _ |> acc + ch
    )
    normLoop(src, idx + 1, nextAcc, len)
=> :Str

normFinish result =
  If[result == "", " ", result]()
=> :Str

normalizeCellText text =
  If[text == "", " ", normFinish(normLoop(text, 0, "", text.length()))]()
=> :Str

// -- truncateWidth --

///@ Purpose: Truncate text to fit within a given display width
///@ Params:
///@   - text: Str — 切り詰め対象テキスト
///@   - width: Int — 最大表示幅 (1-based セル数)
///@ Returns: Str — width 以内に収まる prefix。width<1 や text="" なら ""
///@ Example:
///@   truncateWidth("hello", 3)     // "hel"
///@   truncateWidth("あいう", 3)     // "あ"   (wide=2、3 では 1 グリフのみ)
///@   truncateWidth("hello", 10)    // "hello"
///@   truncateWidth("hello", 0)     // ""
///@ AI-Context: wide char の境界を尊重 — 切り詰め後の last char が wide で
///@   残幅 1 しかない場合、その wide char は drop される (中途半端な半角扱い
///@   になるのを防ぐ)。
///@ AI-Hint: ステータスラインや tab title など、固定幅の slot に文字列を
///@   詰める際の基本 primitive。ペアで padWidth と組み合わせて使う。
twLoop src idx acc remaining len =
  | idx == len |> acc
  | remaining < 1 |> acc
  | _ |>
    CharAt[src, idx]() >=> ch
    CodePoint[ch]() >=> cp
    step <= If[cp > 65535, 2, 1]()
    m <= measureGrapheme(ch)
    | m.width > remaining |> acc
    | _ |> twLoop(src, idx + step, acc + ch, remaining - m.width, len)
=> :Str

truncateWidth text width =
  | width < 1 |> ""
  | text == "" |> ""
  | _ |> twLoop(text, 0, "", width, text.length())
=> :Str

// -- padWidth --

///@ Purpose: Pad text with spaces on the right to reach a target display width
///@ Params:
///@   - text: Str — パディング対象テキスト
///@   - width: Int — 目標表示幅 (セル数)
///@ Returns: Str — width に達するまで右側に空白を付加。既に width 以上なら text そのまま
///@ Example:
///@   padWidth("ok", 5)         // "ok   "
///@   padWidth("hello", 3)      // "hello" (no truncation; 既に超過)
///@   padWidth("あ", 4)          // "あ  "  (wide=2 + 2 spaces = 4)
///@ AI-Context: `Repeat[" ", n]()` (Rust str::repeat 経由)
///@ で O(N) の linear pad。statusLine / progressBar の右寄せに使う。
///@ AI-Hint: 切り詰めは行わないため、`width < displayWidth(text)` の場合は
///@   text がそのまま返る。固定幅の slot に詰めるときは
///@   `padWidth(truncateWidth(text, width), width)` の合成パターンが定番。
// The prior `padLoop` used `s + " "` concat N times
// which was O(N²). Replaced with `Repeat[" ", n]()` which lowers to
// Rust `str::repeat` (O(N)).
padWidth text width =
  current <= displayWidth(text)
  | current > width |> text
  | current == width |> text
  | _ |> text + Repeat[" ", width - current]()
=> :Str

<<< @(WidthMode, measureGrapheme, displayWidth, normalizeCellText, truncateWidth, padWidth)
