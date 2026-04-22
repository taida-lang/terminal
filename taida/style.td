// taida/style.td
//
// ANSI styling helpers -- Stylize, Color, ResetStyle.
//
// This is Layer B (ANSI facade) per TM_DESIGN.md:
//   - No syscalls
//   - No hidden side effects
//   - Returns ANSI escape strings only
//
// -- Color Palette (TM-2h) --

///@ Purpose: Basic 16-color palette (name constants)
///@ AI-Context: Pass to Stylize fg / bg arguments.
///@ Example:
///@   stdout(Stylize("hello", @(fg <= Color.red)))
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

_fgCode name =
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

_bgCode name =
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
_safeFgCode name =
  If[name == "", "", _fgCode(name)]()
=> :Str

_safeBgCode name =
  If[name == "", "", _bgCode(name)]()
=> :Str

// Helper: join a code to the accumulator with ";" separator
_appendCode acc code =
  If[acc == "", code, acc + ";" + code]()
=> :Str

// -- ResetStyle (TM-2h) --

///@ Purpose: Return ANSI reset sequence
///@ Returns: Str
///@ Example:
///@   stdout(ResetStyle())
///@ AI-SideEffects: none
ResetStyle =
  "\x1b[0m"
=> :Str

// -- Stylize (TM-2h) --

///@ Purpose: Apply color and decoration to text as ANSI string
///@ Params:
///@   text: Str
///@   opts: @(fg, bg, bold, dim, underline, italic)
///@ Returns: Str -- prefix + text + reset (or text as-is if no style)
///@ Throws:
///@   StylizeInvalidColor: unknown fg / bg color name
///@ Example:
///@   stdout(Stylize("hello", @(fg <= Color.red, bold <= true)))
///@ AI-SideEffects: none
Stylize text opts =
  c1 <= _safeFgCode(opts.fg)
  c2 <= If[opts.bg == "", c1, _appendCode(c1, _safeBgCode(opts.bg))]()
  c3 <= If[opts.bold == false, c2, _appendCode(c2, "1")]()
  c4 <= If[opts.dim == false, c3, _appendCode(c3, "2")]()
  c5 <= If[opts.italic == false, c4, _appendCode(c4, "3")]()
  c6 <= If[opts.underline == false, c5, _appendCode(c5, "4")]()
  If[c6 == "", text, "\x1b[" + c6 + "m" + text + "\x1b[0m"]()
=> :Str

// -- Color256 / ColorRgb (TM-6e) --

///@ Purpose: 256-color index (0-255)
///@ AI-Context: Pass to Stylize256 fg / bg. index -1 means no color.
Color256 <= @(index <= 0)

///@ Purpose: RGB color (each component 0-255)
///@ AI-Context: Pass to StylizeRgb fg / bg. All -1 means no color.
ColorRgb <= @(r <= 0, g <= 0, b <= 0)

// -- Internal: 256-color SGR code builder --

_validate256 index label =
  | index == -1 |> ""
  | index < 0   |> Error(type <= "StylizeInvalidColor", message <= label + " color index out of range (0-255): " + index.toString()).throw()
  | index > 255 |> Error(type <= "StylizeInvalidColor", message <= label + " color index out of range (0-255): " + index.toString()).throw()
  | _ |> "ok"
=> :Str

_fg256Code index =
  "38;5;" + index.toString()
=> :Str

_bg256Code index =
  "48;5;" + index.toString()
=> :Str

_safeFg256Code index =
  If[index == -1, "", _fg256Code(index)]()
=> :Str

_safeBg256Code index =
  If[index == -1, "", _bg256Code(index)]()
=> :Str

// -- Internal: RGB SGR code builder --

_validateRgbComponent value label component =
  | value == -1 |> ""
  | value < 0   |> Error(type <= "StylizeInvalidColor", message <= label + " " + component + " out of range (0-255): " + value.toString()).throw()
  | value > 255 |> Error(type <= "StylizeInvalidColor", message <= label + " " + component + " out of range (0-255): " + value.toString()).throw()
  | _ |> "ok"
=> :Str

_isNoColorRgb color =
  | color.r == -1 |>
    | color.g == -1 |>
      | color.b == -1 |> true
      | _ |> false
    | _ |> false
  | _ |> false
=> :Bool

_fgRgbCode color =
  "38;2;" + color.r.toString() + ";" + color.g.toString() + ";" + color.b.toString()
=> :Str

_bgRgbCode color =
  "48;2;" + color.r.toString() + ";" + color.g.toString() + ";" + color.b.toString()
=> :Str

_safeFgRgbCode color =
  If[_isNoColorRgb(color) == true, "", _fgRgbCode(color)]()
=> :Str

_safeBgRgbCode color =
  If[_isNoColorRgb(color) == true, "", _bgRgbCode(color)]()
=> :Str

// Validate all RGB components for a color (skip if all -1)
_validateRgbFull color label =
  If[_isNoColorRgb(color) == true, "", _validateRgbComponent(color.r, label, "r")]()
=> :Str

_validateRgbG color label prev =
  If[_isNoColorRgb(color) == true, "", _validateRgbComponent(color.g, label, "g")]()
=> :Str

_validateRgbB color label prev =
  If[_isNoColorRgb(color) == true, "", _validateRgbComponent(color.b, label, "b")]()
=> :Str

// -- Stylize256 (TM-6e) --

///@ Purpose: Apply 256-color styling to text
///@ Params:
///@   text: Str
///@   opts: @(fg <= Color256(index <= -1), bg <= Color256(index <= -1), bold, dim, underline, italic)
///@ Returns: Str
///@ Throws:
///@   StylizeInvalidColor: index out of 0-255 range
Stylize256 text opts =
  // Validate indices first (these throw on invalid values)
  vfg <= _validate256(opts.fg.index, "foreground")
  vbg <= _validate256(opts.bg.index, "background")
  c1 <= _safeFg256Code(opts.fg.index)
  c2 <= If[opts.bg.index == -1, c1, _appendCode(c1, _safeBg256Code(opts.bg.index))]()
  c3 <= If[opts.bold == false, c2, _appendCode(c2, "1")]()
  c4 <= If[opts.dim == false, c3, _appendCode(c3, "2")]()
  c5 <= If[opts.italic == false, c4, _appendCode(c4, "3")]()
  c6 <= If[opts.underline == false, c5, _appendCode(c5, "4")]()
  If[c6 == "", text, "\x1b[" + c6 + "m" + text + "\x1b[0m"]()
=> :Str

// -- StylizeRgb (TM-6e) --

///@ Purpose: Apply RGB color styling to text
///@ Params:
///@   text: Str
///@   opts: @(fg <= ColorRgb(...), bg <= ColorRgb(...), bold, dim, underline, italic)
///@ Returns: Str
///@ Throws:
///@   StylizeInvalidColor: r/g/b out of 0-255 range
StylizeRgb text opts =
  // Validate RGB components (these throw on invalid values)
  vfr <= _validateRgbFull(opts.fg, "foreground")
  vfg2 <= _validateRgbG(opts.fg, "foreground", vfr)
  vfb <= _validateRgbB(opts.fg, "foreground", vfg2)
  vbr <= _validateRgbFull(opts.bg, "background")
  vbg2 <= _validateRgbG(opts.bg, "background", vbr)
  vbb <= _validateRgbB(opts.bg, "background", vbg2)
  c1 <= _safeFgRgbCode(opts.fg)
  c2 <= If[_isNoColorRgb(opts.bg) == true, c1, _appendCode(c1, _safeBgRgbCode(opts.bg))]()
  c3 <= If[opts.bold == false, c2, _appendCode(c2, "1")]()
  c4 <= If[opts.dim == false, c3, _appendCode(c3, "2")]()
  c5 <= If[opts.italic == false, c4, _appendCode(c4, "3")]()
  c6 <= If[opts.underline == false, c5, _appendCode(c5, "4")]()
  If[c6 == "", text, "\x1b[" + c6 + "m" + text + "\x1b[0m"]()
=> :Str

<<< @(Color, ResetStyle, Stylize, Color256, ColorRgb, Stylize256, StylizeRgb)
