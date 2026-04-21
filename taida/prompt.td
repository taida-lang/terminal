// taida/prompt.td
//
// Line editor state machine and prompt types.
//
// This is Layer C (Prompt/Widget) per TM_DESIGN.md:
//   - No syscalls
//   - No hidden side effects
//   - Pure state machine: event in, state out

// -- Imports --

>>> ./width.td => @(DisplayWidth)
>>> ./ansi.td => @(ClearLine, CursorMoveTo)

// -- PromptMode --

///@ Purpose: Prompt display mode
///@ Since: a.7
PromptMode <= @(
  Normal   <= 0
  Password <= 1
)

// -- PromptOptions --

///@ Purpose: Prompt configuration options
///@ Since: a.7
PromptOptions <= @(
  prompt      <= ""
  initial     <= ""
  placeholder <= ""
  mode        <= 0
  history     <= @[]
  completion  <= @()
)

// -- CompletionState --

///@ Purpose: Completion candidates state (v1 minimal)
///@ Since: a.7
CompletionState <= @(
  items    <= @[]
  selected <= 0
  visible  <= false
)

// -- LineEditorAction --

///@ Purpose: Result action from LineEditorStep
///@ Since: a.7
LineEditorAction <= @(
  Editing   <= 0
  Submitted <= 1
  Cancelled <= 2
)

// -- LineEditorState --

///@ Purpose: Line editor internal state (pure, no side effects)
///@ Since: a.7
LineEditorState <= @(
  text           <= ""
  cursor         <= 0
  prompt         <= ""
  mode           <= 0
  placeholder    <= ""
  history        <= @[]
  history_index  <= -1
  history_saved  <= ""
  completion     <= CompletionState
  action         <= 0
)

// -- LineEditorNew --

///@ Purpose: Create a new LineEditorState from options
///@ Since: a.7
LineEditorNew opts =
  LineEditorState(text <= opts.initial, cursor <= opts.initial.length(), prompt <= opts.prompt, mode <= opts.mode, placeholder <= opts.placeholder, history <= opts.history, history_index <= -1, history_saved <= "", completion <= CompletionState, action <= LineEditorAction.Editing)
=> :@()

// -- Internal: key kind constants --

_KK_Char       <= 0
_KK_Enter      <= 1
_KK_Escape     <= 2
_KK_Backspace  <= 4
_KK_Delete     <= 5
_KK_ArrowUp    <= 6
_KK_ArrowDown  <= 7
_KK_ArrowLeft  <= 8
_KK_ArrowRight <= 9
_KK_Home       <= 10
_KK_End        <= 11

// -- Internal: string manipulation helpers --

_insertAt s pos ch =
  left <=[ Slice[s, 0, pos]()
  right <=[ Slice[s, pos, s.length()]()
  left + ch + right
=> :Str

_deleteAt s pos =
  If[pos < 0, s, _deleteAtInner(s, pos)]()
=> :Str

_deleteAtInner s pos =
  If[pos == s.length(), s, _deleteAtDo(s, pos)]()
=> :Str

_deleteAtDo s pos =
  left <=[ Slice[s, 0, pos]()
  right <=[ Slice[s, pos + 1, s.length()]()
  left + right
=> :Str

// -- Internal: make state with new values helper --

_makeState text cursor state action =
  LineEditorState(text <= text, cursor <= cursor, prompt <= state.prompt, mode <= state.mode, placeholder <= state.placeholder, history <= state.history, history_index <= state.history_index, history_saved <= state.history_saved, completion <= state.completion, action <= action)
=> :@()

_makeStateHist text cursor state histIdx histSaved action =
  LineEditorState(text <= text, cursor <= cursor, prompt <= state.prompt, mode <= state.mode, placeholder <= state.placeholder, history <= state.history, history_index <= histIdx, history_saved <= histSaved, completion <= state.completion, action <= action)
=> :@()

// -- LineEditorStep --

///@ Purpose: Process one key event and return the next state
///@ Since: a.7
LineEditorStep state key =
  If[state.action == LineEditorAction.Editing, _stepEditing(state, key), state]()
=> :@()

_stepEditing state key =
  | key.kind == _KK_Enter      |> _makeState(state.text, state.cursor, state, LineEditorAction.Submitted)
  | key.kind == _KK_Escape     |> _makeState(state.text, state.cursor, state, LineEditorAction.Cancelled)
  | key.kind == _KK_ArrowLeft  |> _stepArrowLeft(state)
  | key.kind == _KK_ArrowRight |> _stepArrowRight(state)
  | key.kind == _KK_Home       |> _makeState(state.text, 0, state, LineEditorAction.Editing)
  | key.kind == _KK_End        |> _makeState(state.text, state.text.length(), state, LineEditorAction.Editing)
  | key.kind == _KK_Backspace  |> _stepBackspace(state)
  | key.kind == _KK_Delete     |> _stepDelete(state)
  | key.kind == _KK_ArrowUp    |> _stepHistoryPrev(state)
  | key.kind == _KK_ArrowDown  |> _stepHistoryNext(state)
  | key.kind == _KK_Char       |> _stepInsertChar(state, key)
  | _ |> state
=> :@()

_stepArrowLeft state =
  nc <= If[state.cursor > 0, state.cursor - 1, 0]()
  _makeState(state.text, nc, state, LineEditorAction.Editing)
=> :@()

_stepArrowRight state =
  nc <= If[state.cursor < state.text.length(), state.cursor + 1, state.text.length()]()
  _makeState(state.text, nc, state, LineEditorAction.Editing)
=> :@()

_stepBackspace state =
  If[state.cursor == 0, state, _stepBackspaceDo(state)]()
=> :@()

_stepBackspaceDo state =
  newText <= _deleteAt(state.text, state.cursor - 1)
  _makeState(newText, state.cursor - 1, state, LineEditorAction.Editing)
=> :@()

_stepDelete state =
  If[state.cursor == state.text.length(), state, _stepDeleteDo(state)]()
=> :@()

_stepDeleteDo state =
  newText <= _deleteAt(state.text, state.cursor)
  _makeState(newText, state.cursor, state, LineEditorAction.Editing)
=> :@()

_stepInsertChar state key =
  If[key.text == "", state, _stepInsertCharDo(state, key)]()
=> :@()

_stepInsertCharDo state key =
  newText <= _insertAt(state.text, state.cursor, key.text)
  _makeState(newText, state.cursor + key.text.length(), state, LineEditorAction.Editing)
=> :@()

_stepHistoryPrev state =
  If[state.history.length() == 0, state, _stepHistoryPrevDo(state)]()
=> :@()

_stepHistoryPrevDo state =
  newIdx <= (
    | state.history_index == -1 |> state.history.length() - 1
    | state.history_index > 0   |> state.history_index - 1
    | _ |> 0
  )
  saved <= If[state.history_index == -1, state.text, state.history_saved]()
  state.history.get(newIdx) ]=> histText
  _makeStateHist(histText, histText.length(), state, newIdx, saved, LineEditorAction.Editing)
=> :@()

_stepHistoryNext state =
  If[state.history_index == -1, state, _stepHistoryNextDo(state)]()
=> :@()

_stepHistoryNextDo state =
  newIdx <= state.history_index + 1
  | newIdx == state.history.length() |> _makeStateHist(state.history_saved, state.history_saved.length(), state, -1, "", LineEditorAction.Editing)
  | newIdx > state.history.length() |> _makeStateHist(state.history_saved, state.history_saved.length(), state, -1, "", LineEditorAction.Editing)
  | _ |> _stepHistoryNextLoad(state, newIdx)
=> :@()

_stepHistoryNextLoad state newIdx =
  state.history.get(newIdx) ]=> histText
  _makeStateHist(histText, histText.length(), state, newIdx, state.history_saved, LineEditorAction.Editing)
=> :@()

// -- LineEditorRender --

///@ Purpose: Generate display string from current state
///@ Returns: @(line <= "", cursor_col <= 1)
///@ Since: a.7
LineEditorRender state =
  displayText <= _getDisplayText(state)
  promptWidth <= DisplayWidth(state.prompt)
  cursorDisplayPos <= If[state.mode == PromptMode.Password, state.cursor, _cursorWidthCalc(state)]()
  cursorCol <= promptWidth + cursorDisplayPos + 1
  @(line <= ClearLine() + state.prompt + displayText, cursor_col <= cursorCol)
=> :@()

_getDisplayText state =
  | state.mode == PromptMode.Password |> _maskLoop("", state.text.length())
  | state.text == "" |>
    | state.placeholder == "" |> ""
    | _ |> state.placeholder
  | _ |> state.text
=> :Str

_maskLoop acc remaining =
  | remaining < 1 |> acc
  | _ |> _maskLoop(acc + "*", remaining - 1)
=> :Str

_cursorWidthCalc state =
  prefix <=[ Slice[state.text, 0, state.cursor]()
  DisplayWidth(prefix)
=> :Int

<<< @(PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, LineEditorNew, LineEditorStep, LineEditorRender)
