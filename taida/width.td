// taida/width.td
//
// Unicode width calculation for terminal rendering.
//
// This is Layer C (Renderer foundation) per TM_DESIGN.md:
//   - No syscalls
//   - No hidden side effects
//   - Pure computation on strings
//
// Width Policy (Phase 4 lock):
//   1. ASCII printable (U+0020..U+007E) -> width 1
//   2. Combining mark -> width 0
//   3. East Asian Wide / Fullwidth -> width 2
//   4. Ambiguous -> width 1 (v1 decision)
//   5. TAB -> renderer layer forbids; pre-expand to spaces
//   6. Newline -> forbidden in cell text
//
// History:
//   - Phase 4 (TM-4b/4c): pure-Taida implementation on top of CharAt /
//     CodePoint. Functionally correct but O(N²) because CharAt is
//     O(idx) in the interpreter and `acc + ch` re-allocates per step.
//   - Phase 10 (TMB-025): native implementation behind the same facade
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
// Sub-module callers (`widgets.td`, `prompt.td`) still see the
// pure-Taida fallback — typical inputs are short (< 200 chars) so the
// O(N²) cost is bounded and does not regress Hachikuma's frame budget.
// See TMB-025 note for the scaling envelope.

// -- Width Mode Enum --

///@ Purpose: Unicode width category enum pack
///@ AI-Context: Compare with MeasureGrapheme result `mode` field.
///@   Tag values are frozen — the Rust `width.rs` matches against these
///@   literals (Narrow=0, Wide=1, Zero=2, Ambiguous=3).
WidthMode <= @(
  Narrow    <= 0
  Wide      <= 1
  Zero      <= 2
  Ambiguous <= 3
)

// -- Internal helpers --

// Range check: lo < cp and cp < hi+1 (inclusive range)
_inRange cp lo hi =
  | cp < lo |> false
  | cp > hi |> false
  | _ |> true
=> :Bool

// -- Internal: codepoint range checks --

// Combining marks: width 0
_isCombining cp =
  | _inRange(cp, 768, 879) == true   |> true
  | _inRange(cp, 4448, 4607) == true |> true
  | _inRange(cp, 6832, 6911) == true |> true
  | _inRange(cp, 7616, 7679) == true |> true
  | _inRange(cp, 8400, 8447) == true |> true
  | _inRange(cp, 65056, 65071) == true |> true
  | _inRange(cp, 65024, 65039) == true |> true
  | cp == 65279 |> true
  | cp == 8203  |> true
  | cp == 8204  |> true
  | cp == 8205  |> true
  | _inRange(cp, 12441, 12442) == true  |> true
  | _inRange(cp, 917760, 917999) == true |> true
  | _ |> false
=> :Bool

// East Asian Wide / Fullwidth: width 2
_isWide cp =
  | _inRange(cp, 11904, 42191) == true  |> true
  | _inRange(cp, 44032, 55203) == true  |> true
  | _inRange(cp, 63744, 64255) == true  |> true
  | _inRange(cp, 65072, 65103) == true  |> true
  | _inRange(cp, 65281, 65376) == true  |> true
  | _inRange(cp, 65504, 65510) == true  |> true
  | _inRange(cp, 4352, 4447) == true    |> true
  | _inRange(cp, 131072, 262141) == true |> true
  | _ |> false
=> :Bool

// Control characters: width 0
_isControl cp =
  | cp < 32   |> true
  | cp == 127 |> true
  | _inRange(cp, 128, 159) == true |> true
  | _ |> false
=> :Bool

// -- MeasureGrapheme --

///@ Purpose: Measure the display width and category of a single grapheme
///@ Params:
///@   text: Str -- the text to measure (only the first grapheme is measured)
///@ Returns: @(width <= 0, mode <= WidthMode.Narrow)
MeasureGrapheme text =
  If[text == "", @(width <= 0, mode <= WidthMode.Zero), _measureGraphemeInner(text)]()
=> :@(width: Int, mode: Int)

_measureGraphemeInner text =
  ch <=[ CharAt[text, 0]()
  cp <=[ CodePoint[ch]()
  | _isControl(cp) == true  |> @(width <= 0, mode <= WidthMode.Zero)
  | _isCombining(cp) == true |> @(width <= 0, mode <= WidthMode.Zero)
  | _isWide(cp) == true     |> @(width <= 2, mode <= WidthMode.Wide)
  | _ |> @(width <= 1, mode <= WidthMode.Narrow)
=> :@(width: Int, mode: Int)

// -- DisplayWidth --

///@ Purpose: Calculate the total display width (cell count) of a string
///@ Params:
///@   text: Str -- the text to measure
///@ Returns: Int -- display width in cells
// Combined loop + step in single function to avoid forward references
// in the facade module loader (which evaluates top-to-bottom without hoisting).
_dwCalc src idx acc len =
  | idx == len |> acc
  | _ |>
    CharAt[src, idx]() ]=> ch
    CodePoint[ch]() ]=> cp
    step <= If[cp > 65535, 2, 1]()
    m <= MeasureGrapheme(ch)
    _dwCalc(src, idx + step, acc + m.width, len)
=> :Int

DisplayWidth text =
  If[text == "", 0, _dwCalc(text, 0, 0, text.length())]()
=> :Int

// -- NormalizeCellText --

///@ Purpose: Normalize cell text (TAB -> spaces, newline -> strip, empty -> " ")
///@ Params:
///@   text: Str -- the text to normalize
///@ Returns: Str -- normalized text
_normLoop src idx acc len =
  | idx == len |> acc
  | _ |>
    CharAt[src, idx]() ]=> ch
    nextAcc <= (
      | ch == "\t" |> acc + "    "
      | ch == "\n" |> acc
      | ch == "\r" |> acc
      | _ |> acc + ch
    )
    _normLoop(src, idx + 1, nextAcc, len)
=> :Str

_normFinish result =
  If[result == "", " ", result]()
=> :Str

NormalizeCellText text =
  If[text == "", " ", _normFinish(_normLoop(text, 0, "", text.length()))]()
=> :Str

// -- TruncateWidth --

///@ Purpose: Truncate text to fit within a given display width
///@ Params:
///@   text: Str -- the text to truncate
///@   width: Int -- maximum display width
///@ Returns: Str -- truncated text
_twLoop src idx acc remaining len =
  | idx == len |> acc
  | remaining < 1 |> acc
  | _ |>
    CharAt[src, idx]() ]=> ch
    CodePoint[ch]() ]=> cp
    step <= If[cp > 65535, 2, 1]()
    m <= MeasureGrapheme(ch)
    | m.width > remaining |> acc
    | _ |> _twLoop(src, idx + step, acc + ch, remaining - m.width, len)
=> :Str

TruncateWidth text width =
  | width < 1 |> ""
  | text == "" |> ""
  | _ |> _twLoop(text, 0, "", width, text.length())
=> :Str

// -- PadWidth --

///@ Purpose: Pad text with spaces on the right to reach a target display width
///@ Params:
///@   text: Str -- the text to pad
///@   width: Int -- target display width
///@ Returns: Str -- padded text
// TMB-026 FIXED: the prior `_padLoop` used `s + " "` concat N times
// which was O(N²). Replaced with `Repeat[" ", n]()` which lowers to
// Rust `str::repeat` (O(N)).
PadWidth text width =
  current <= DisplayWidth(text)
  | current > width |> text
  | current == width |> text
  | _ |> text + Repeat[" ", width - current]()
=> :Str

<<< @(WidthMode, MeasureGrapheme, DisplayWidth, NormalizeCellText, TruncateWidth, PadWidth)
