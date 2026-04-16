// taida/widgets.td
//
// UX widgets: spinner, progress bar, status line.
//
// This is Layer C (Widget) per TM_DESIGN.md:
//   - No syscalls
//   - No hidden side effects
//   - Widgets return Str or pure state
//   - Redraw timing is NOT owned by widget
//   - Long-running animation loops are controlled externally

// -- Imports --

>>> ./width.td => @(DisplayWidth, TruncateWidth, PadWidth)

// -- Spinner --

_SPINNER_FRAMES <= @["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
_SPINNER_FRAME_COUNT <= 10

///@ Purpose: Spinner state
///@ Since: a.7
SpinnerState <= @(
  frame <= 0
  label <= ""
  done  <= false
)

///@ Purpose: Advance the spinner to the next frame
///@ Since: a.7
SpinnerNext state =
  If[state.done == true, state, _spinnerNextInner(state)]()
=> :@()

_spinnerNextInner state =
  inc <= state.frame + 1
  divResult <=[ Div[inc, _SPINNER_FRAME_COUNT]()
  nextFrame <= inc - divResult * _SPINNER_FRAME_COUNT
  SpinnerState(frame <= nextFrame, label <= state.label, done <= state.done)
=> :@()

///@ Purpose: Render the spinner as a display string
///@ Since: a.7
SpinnerRender state =
  If[state.done == true, _spinnerDoneText(state), _spinnerActiveText(state)]()
=> :Str

_spinnerDoneText state =
  If[state.label == "", "v", "v " + state.label]()
=> :Str

_spinnerActiveText state =
  _SPINNER_FRAMES.get(state.frame) ]=> frameChar
  If[state.label == "", frameChar, frameChar + " " + state.label]()
=> :Str

// -- Progress Bar --

///@ Purpose: Progress bar options
///@ Since: a.7
ProgressOptions <= @(
  width           <= 20
  complete_char   <= "#"
  incomplete_char <= "-"
  left_label      <= ""
  right_label     <= ""
)

///@ Purpose: Render a progress bar string
///@ Throws: ProgressInvalidTotal if total < 1, ProgressInvalidCurrent if current < 0
///@ Since: a.7
ProgressBar current total opts =
  | total < 1   |> Error(type <= "ProgressInvalidTotal", message <= "total must be >= 1").throw()
  | current < 0 |> Error(type <= "ProgressInvalidCurrent", message <= "current must be >= 0").throw()
  | _ |> _progressBarInner(current, total, opts)
=> :Str

_progressBarInner current total opts =
  clamped <= If[current > total, total, current]()
  barWidth <= If[opts.width < 1, 1, opts.width]()
  filled <=[ Div[clamped * barWidth, total]()
  empty <= barWidth - filled
  fillStr <= _repeatStr(opts.complete_char, filled)
  emptyStr <= _repeatStr(opts.incomplete_char, empty)
  bar <= fillStr + emptyStr
  withLeft <= If[opts.left_label == "", bar, opts.left_label + " " + bar]()
  If[opts.right_label == "", withLeft, withLeft + " " + opts.right_label]()
=> :Str

_repeatStr ch count =
  If[count < 1, "", _repeatLoop("", ch, count)]()
=> :Str

_repeatLoop acc ch remaining =
  | remaining < 1 |> acc
  | _ |> _repeatLoop(acc + ch, ch, remaining - 1)
=> :Str

// -- Status Line --

///@ Purpose: Generate a status line with left/right text
///@ Since: a.7
StatusLine left right width =
  If[width == 0, left + right, _statusLineInner(left, right, width)]()
=> :Str

_statusLineInner left right width =
  leftW <= DisplayWidth(left)
  rightW <= DisplayWidth(right)
  gap <= width - leftW - rightW
  | gap > 0 |> left + _repeatStr(" ", gap) + right
  | gap == 0 |> left + right
  | _ |> _statusLineTruncate(left, right, width, rightW)
=> :Str

_statusLineTruncate left right width rightW =
  avail <= width - rightW
  If[avail < 1, TruncateWidth(right, width), TruncateWidth(left, avail) + right]()
=> :Str

<<< @(SpinnerState, SpinnerNext, SpinnerRender, ProgressOptions, ProgressBar, StatusLine)
