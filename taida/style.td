// taida/style.td
//
// ANSI styling helpers -- stylize, color, resetStyle.
//
// This is the pure ANSI styling facade:
//   - No syscalls
//   - No hidden side effects
//   - Returns ANSI escape strings only
//
// Public names follow the current Taida package conventions: functions are
// camelCase and pack types are PascalCase.
//
// -- Color Palette --

///@ Purpose: Basic 16-color palette (name constants)
///@ AI-Context: Pass to stylize fg / bg arguments. 16 entries (8 base + 8 bright)。
///@   全フィールドの値は文字列定数 (e.g. "red", "bright_blue") で、
///@   `stylize` 内部で SGR コードに変換される。field 名は snake_case。
///@ AI-Hint: 直接 SGR 数値を扱いたい場合は Color256 / ColorRgb を使う。
///@ Example:
///@   stdout(stylize("hello", @(fg <= Color.red, bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)))
///@   stdout(stylize("warn", @(fg <= Color.bright_yellow, bg <= "", bold <= true, dim <= false, underline <= false, italic <= false)))
Color <= @(
  black          <= "black"
  red            <= "red"
  green          <= "green"
  yellow         <= "yellow"
  blue           <= "blue"
  magenta        <= "magenta"
  cyan           <= "cyan"
  white          <= "white"
  bright_black   <= "bright_black"
  bright_red     <= "bright_red"
  bright_green   <= "bright_green"
  bright_yellow  <= "bright_yellow"
  bright_blue    <= "bright_blue"
  bright_magenta <= "bright_magenta"
  bright_cyan    <= "bright_cyan"
  bright_white   <= "bright_white"
)

// -- Internal: color name to SGR code mapping --

fgCode name =
  | name == "black"          |> "30"
  | name == "red"            |> "31"
  | name == "green"          |> "32"
  | name == "yellow"         |> "33"
  | name == "blue"           |> "34"
  | name == "magenta"        |> "35"
  | name == "cyan"           |> "36"
  | name == "white"          |> "37"
  | name == "bright_black"   |> "90"
  | name == "bright_red"     |> "91"
  | name == "bright_green"   |> "92"
  | name == "bright_yellow"  |> "93"
  | name == "bright_blue"    |> "94"
  | name == "bright_magenta" |> "95"
  | name == "bright_cyan"    |> "96"
  | name == "bright_white"   |> "97"
  | _ |> Error(type <= "StylizeInvalidColor", message <= "unknown foreground color: " + name).throw()
=> :Str

bgCode name =
  | name == "black"          |> "40"
  | name == "red"            |> "41"
  | name == "green"          |> "42"
  | name == "yellow"         |> "43"
  | name == "blue"           |> "44"
  | name == "magenta"        |> "45"
  | name == "cyan"           |> "46"
  | name == "white"          |> "47"
  | name == "bright_black"   |> "100"
  | name == "bright_red"     |> "101"
  | name == "bright_green"   |> "102"
  | name == "bright_yellow"  |> "103"
  | name == "bright_blue"    |> "104"
  | name == "bright_magenta" |> "105"
  | name == "bright_cyan"    |> "106"
  | name == "bright_white"   |> "107"
  | _ |> Error(type <= "StylizeInvalidColor", message <= "unknown background color: " + name).throw()
=> :Str

// Safe wrappers that guard against empty-string calls (avoids
// triggering error-throwing paths when no color is specified).
safeFgCode name =
  If[name == "", "", fgCode(name)]()
=> :Str

safeBgCode name =
  If[name == "", "", bgCode(name)]()
=> :Str

// Helper: join a code to the accumulator with ";" separator
appendCode acc code =
  If[acc == "", code, acc + ";" + code]()
=> :Str

// -- resetStyle --

///@ Purpose: Return ANSI reset sequence
///@ Params: (none)
///@ Returns: Str — ANSI escape sequence `ESC[0m`
///@ Example:
///@   stdout(resetStyle())
///@ AI-Context: stylize / stylize256 / stylizeRgb の戻り値は既に reset suffix を
///@   含むため、通常はユーザコードで直接呼ぶ必要はない。生 SGR を組み立てる
///@   advanced ケースでのみ末尾に append する。
///@ AI-SideEffects: none
resetStyle =
  "\x1b[0m"
=> :Str

// -- stylize --

///@ Purpose: Apply color and decoration to text as ANSI string
///@ Params:
///@   - text: Str — 装飾対象テキスト
///@   - opts: @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
///@     - fg: 前景色名 ("red" / "bright_blue" 等、空文字列で指定なし)
///@     - bg: 背景色名 (同上)
///@     - bold / dim / italic / underline: Bool (デコレーション)
///@ Returns: Str — `ESC[<sgr>m` + text + `ESC[0m`、装飾なしなら text そのまま
///@ Throws:
///@   - StylizeInvalidColor: 未知の fg / bg 色名 (Color パレット外)
///@ Example:
///@   stdout(stylize("hello", @(fg <= Color.red, bg <= "", bold <= true, dim <= false, underline <= false, italic <= false)))
///@ AI-Context: opts は **6 フィールド全てが必須**。1 つでも欠けると
///@   ぶちパックの構造的部分型付けで undefined フィールド参照になる。
///@   無装飾を表現するには空文字 / false を明示的に渡す。
///@ AI-Hint: 256 色 / RGB を使いたい場合は stylize256 / stylizeRgb を使う。
///@ AI-SideEffects: none
stylize text opts =
  c1 <= safeFgCode(opts.fg)
  c2 <= If[opts.bg == "", c1, appendCode(c1, safeBgCode(opts.bg))]()
  c3 <= If[opts.bold == false, c2, appendCode(c2, "1")]()
  c4 <= If[opts.dim == false, c3, appendCode(c3, "2")]()
  c5 <= If[opts.italic == false, c4, appendCode(c4, "3")]()
  c6 <= If[opts.underline == false, c5, appendCode(c5, "4")]()
  If[c6 == "", text, "\x1b[" + c6 + "m" + text + "\x1b[0m"]()
=> :Str

// -- Color256 / ColorRgb --

///@ Purpose: 256-color index (0-255) ぶちパック型
///@ AI-Context: Pass to stylize256 fg / bg。`index <= -1` で「色指定なし」を
///@   表現する sentinel 値。index 範囲外 (< -1 or > 255) は throw。
///@ AI-Hint: terminal が 256 色非対応の場合 (e.g. CI 上の `TERM=dumb`) でも
///@   ANSI 文字列は出力されるが端末側で無視される。
///@ Example:
///@   red256 <= Color256(index <= 196)
///@   stdout(stylize256("alert", @(fg <= red256, bg <= Color256(index <= -1), bold <= true, dim <= false, underline <= false, italic <= false)))
Color256 <= @(index <= 0)

///@ Purpose: RGB color (each component 0-255) ぶちパック型
///@ AI-Context: Pass to stylizeRgb fg / bg。3 component 全 `-1` で「色指定なし」。
///@   いずれかの component が範囲外 (< -1 or > 255) は throw。
///@ AI-Hint: 24-bit color 非対応の端末では fallback されないため、フォールバック
///@   が必要なら呼び出し側で env 変数 (`COLORTERM=truecolor`) を見て分岐する。
///@ Example:
///@   purple <= ColorRgb(r <= 128, g <= 0, b <= 128)
///@   stdout(stylizeRgb("title", @(fg <= purple, bg <= ColorRgb(r <= -1, g <= -1, b <= -1), bold <= false, dim <= false, underline <= false, italic <= false)))
ColorRgb <= @(r <= 0, g <= 0, b <= 0)

// -- Internal: 256-color SGR code builder --

validate256 index label =
  | index == -1 |> ""
  | index < 0   |> Error(type <= "StylizeInvalidColor", message <= label + " color index out of range (0-255): " + index.toString()).throw()
  | index > 255 |> Error(type <= "StylizeInvalidColor", message <= label + " color index out of range (0-255): " + index.toString()).throw()
  | _ |> "ok"
=> :Str

fg256Code index =
  "38;5;" + index.toString()
=> :Str

bg256Code index =
  "48;5;" + index.toString()
=> :Str

safeFg256Code index =
  If[index == -1, "", fg256Code(index)]()
=> :Str

safeBg256Code index =
  If[index == -1, "", bg256Code(index)]()
=> :Str

// -- Internal: RGB SGR code builder --

validateRgbComponent value label component =
  | value == -1 |> ""
  | value < 0   |> Error(type <= "StylizeInvalidColor", message <= label + " " + component + " out of range (0-255): " + value.toString()).throw()
  | value > 255 |> Error(type <= "StylizeInvalidColor", message <= label + " " + component + " out of range (0-255): " + value.toString()).throw()
  | _ |> "ok"
=> :Str

isNoColorRgb color =
  | color.r == -1 |>
    | color.g == -1 |>
      | color.b == -1 |> true
      | _ |> false
    | _ |> false
  | _ |> false
=> :Bool

fgRgbCode color =
  "38;2;" + color.r.toString() + ";" + color.g.toString() + ";" + color.b.toString()
=> :Str

bgRgbCode color =
  "48;2;" + color.r.toString() + ";" + color.g.toString() + ";" + color.b.toString()
=> :Str

safeFgRgbCode color =
  If[isNoColorRgb(color) == true, "", fgRgbCode(color)]()
=> :Str

safeBgRgbCode color =
  If[isNoColorRgb(color) == true, "", bgRgbCode(color)]()
=> :Str

// Validate all RGB components for a color (skip if all -1)
validateRgbFull color label =
  If[isNoColorRgb(color) == true, "", validateRgbComponent(color.r, label, "r")]()
=> :Str

validateRgbG color label prev =
  If[isNoColorRgb(color) == true, "", validateRgbComponent(color.g, label, "g")]()
=> :Str

validateRgbB color label prev =
  If[isNoColorRgb(color) == true, "", validateRgbComponent(color.b, label, "b")]()
=> :Str

// -- stylize256 --

///@ Purpose: Apply 256-color styling to text
///@ Params:
///@   - text: Str — 装飾対象テキスト
///@   - opts: @(fg, bg, bold, dim, underline, italic)
///@     - fg: Color256 — `Color256(index <= -1)` で前景色なし
///@     - bg: Color256 — 同上 (背景色)
///@     - bold / dim / italic / underline: Bool
///@ Returns: Str — `ESC[<sgr>m` + text + `ESC[0m`、無装飾なら text そのまま
///@ Throws:
///@   - StylizeInvalidColor: index < -1 or index > 255
///@ Example:
///@   stdout(stylize256("ok", @(fg <= Color256(index <= 46), bg <= Color256(index <= -1), bold <= true, dim <= false, underline <= false, italic <= false)))
///@ AI-Context: 256 色は 0-15 が basic 16 色、16-231 が 6×6×6 RGB cube、
///@   232-255 が grayscale ramp。Color256(index <= -1) で「色なし」を表現する。
///@ AI-SideEffects: none
stylize256 text opts =
  // Validate indices first (these throw on invalid values)
  vfg <= validate256(opts.fg.index, "foreground")
  vbg <= validate256(opts.bg.index, "background")
  c1 <= safeFg256Code(opts.fg.index)
  c2 <= If[opts.bg.index == -1, c1, appendCode(c1, safeBg256Code(opts.bg.index))]()
  c3 <= If[opts.bold == false, c2, appendCode(c2, "1")]()
  c4 <= If[opts.dim == false, c3, appendCode(c3, "2")]()
  c5 <= If[opts.italic == false, c4, appendCode(c4, "3")]()
  c6 <= If[opts.underline == false, c5, appendCode(c5, "4")]()
  If[c6 == "", text, "\x1b[" + c6 + "m" + text + "\x1b[0m"]()
=> :Str

// -- stylizeRgb --

///@ Purpose: Apply RGB color styling to text
///@ Params:
///@   - text: Str — 装飾対象テキスト
///@   - opts: @(fg, bg, bold, dim, underline, italic)
///@     - fg: ColorRgb — 全 component -1 で前景色なし
///@     - bg: ColorRgb — 同上 (背景色)
///@     - bold / dim / italic / underline: Bool
///@ Returns: Str — `ESC[38;2;<r>;<g>;<b>m` + text + `ESC[0m` 形
///@ Throws:
///@   - StylizeInvalidColor: r / g / b いずれかが < -1 または > 255
///@ Example:
///@   blue <= ColorRgb(r <= 30, g <= 144, b <= 255)
///@   stdout(stylizeRgb("info", @(fg <= blue, bg <= ColorRgb(r <= -1, g <= -1, b <= -1), bold <= false, dim <= false, underline <= false, italic <= false)))
///@ AI-Context: 24-bit truecolor (16M 色) を SGR 38;2;r;g;b / 48;2;r;g;b で
///@   出力。端末側 (e.g., GNOME Terminal, iTerm2) が COLORTERM=truecolor を
///@   advertise している環境で有効。
///@ AI-Hint: 256 色までで十分なら stylize256 のほうが端末互換性が高い。
///@ AI-SideEffects: none
stylizeRgb text opts =
  // Validate RGB components (these throw on invalid values)
  vfr <= validateRgbFull(opts.fg, "foreground")
  vfg2 <= validateRgbG(opts.fg, "foreground", vfr)
  vfb <= validateRgbB(opts.fg, "foreground", vfg2)
  vbr <= validateRgbFull(opts.bg, "background")
  vbg2 <= validateRgbG(opts.bg, "background", vbr)
  vbb <= validateRgbB(opts.bg, "background", vbg2)
  c1 <= safeFgRgbCode(opts.fg)
  c2 <= If[isNoColorRgb(opts.bg) == true, c1, appendCode(c1, safeBgRgbCode(opts.bg))]()
  c3 <= If[opts.bold == false, c2, appendCode(c2, "1")]()
  c4 <= If[opts.dim == false, c3, appendCode(c3, "2")]()
  c5 <= If[opts.italic == false, c4, appendCode(c4, "3")]()
  c6 <= If[opts.underline == false, c5, appendCode(c5, "4")]()
  If[c6 == "", text, "\x1b[" + c6 + "m" + text + "\x1b[0m"]()
=> :Str

<<< @(Color, resetStyle, stylize, Color256, ColorRgb, stylize256, stylizeRgb)
