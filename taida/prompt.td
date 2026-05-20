// taida/prompt.td
//
// Line editor state machine and prompt types.
//
// This is the pure prompt/widget layer:
//   - No syscalls
//   - No hidden side effects
//   - Pure state machine: event in, state out
//
// Public names follow the current Taida package conventions: functions are
// camelCase, enum-like fields are snake_case, and pack types are PascalCase.

// -- Imports --

>>> ./width.td => @(displayWidth)
>>> ./ansi.td => @(clearLine, cursorMoveTo)

// -- KeyKind (re-imported as parameter to lineEditorStep, but the kind
//    field in the key pack is just an Int — we compare against literal
//    integers below to avoid coupling on KeyKind variant names.

// -- PromptMode --

///@ Purpose: Prompt display mode 列挙パック
///@ Params:
///@   - normal:   Int = 0 — 通常表示 (text を素のまま描画)
///@   - password: Int = 1 — パスワード表示 (text を "*" でマスク)
///@ Returns: PromptMode ぶちパック
///@ AI-Context: PromptOptions.mode / LineEditorState.mode フィールドの値として使う。
///@   タグ値は v1 で凍結 (再番号はマイグレーション必須)。
///@ AI-Hint: パスワード入力は Mode を切替えるだけで描画が "*" マスクされる。
///@   state.text 自体には平文が入っているので、submit 後は呼出側で zeroize 推奨。
///@ Example:
///@   opts <= PromptOptions(prompt <= "Password: ", initial <= "", placeholder <= "", mode <= PromptMode.password, history <= @[], completion <= CompletionState)
PromptMode <= @(
  normal   <= 0
  password <= 1
)

// -- PromptOptions --

///@ Purpose: Prompt configuration options
///@ Params:
///@   - prompt:      Str       — プロンプト文字列 (e.g., "> ")
///@   - initial:     Str       — 初期入力テキスト
///@   - placeholder: Str       — text="" のときに表示するヒント
///@   - mode:        Int       — PromptMode タグ (normal=0, password=1)
///@   - history:     @[Str]    — 履歴 (上下キーで navigate)
///@   - completion:  CompletionState — 補完設定 (v1 では minimal placeholder)
///@ Returns: PromptOptions ぶちパック
///@ AI-Context: lineEditorNew に渡して LineEditorState を構築する設定パック。
///@   全フィールドが必須 (ぶちパックの構造的部分型付け)。
///@ Example:
///@   opts <= PromptOptions(prompt <= "$ ", initial <= "", placeholder <= "type a command", mode <= PromptMode.normal, history <= @[], completion <= CompletionState)
PromptOptions <= @(
  prompt      <= ""
  initial     <= ""
  placeholder <= ""
  mode        <= 0
  history     <= @[]
  completion  <= @(items <= @[], selected <= 0, visible <= false)
)

// -- CompletionState --

///@ Purpose: Completion candidates state (v1 minimal)
///@ Params:
///@   - items:    @[Str] — 候補リスト
///@   - selected: Int    — 選択中インデックス (0-based)
///@   - visible:  Bool   — ポップアップ表示中フラグ
///@ Returns: CompletionState ぶちパック
///@ AI-Context: v1 では minimal placeholder 。フル補完 UI は将来拡張 (v2+)。
///@   現状は LineEditorState.completion フィールドの type-shape 維持のため存在。
CompletionState <= @(
  items    <= @[]
  selected <= 0
  visible  <= false
)

// -- LineEditorAction --

///@ Purpose: Result action from lineEditorStep 列挙パック
///@ Params:
///@   - editing:   Int = 0 — 編集中 (Enter 未入力)
///@   - submitted: Int = 1 — Enter で確定
///@   - cancelled: Int = 2 — Escape でキャンセル
///@ Returns: LineEditorAction ぶちパック
///@ AI-Context: LineEditorState.action フィールドと比較する。
///@   submitted / cancelled になったら caller はループを抜けて state.text を読む。
///@ AI-Hint: state.action |== LineEditorAction.editing の間 readKey + lineEditorStep の
///@   ループを continue する pattern が定番。
///@ Example:
///@   state.action |== LineEditorAction.submitted => stdout("input: " + state.text)
///@   state.action |== LineEditorAction.cancelled => stdout("aborted")
LineEditorAction <= @(
  editing   <= 0
  submitted <= 1
  cancelled <= 2
)

// -- LineEditorState --

///@ Purpose: Line editor internal state (pure, no side effects)
///@ Params:
///@   - text:          Str    — 現在の入力テキスト
///@   - cursor:        Int    — カーソル位置 (0..text.length() の char index)
///@   - prompt:        Str    — プロンプト文字列
///@   - mode:          Int    — PromptMode タグ (normal=0, password=1)
///@   - placeholder:   Str    — text="" 時のヒント
///@   - history:       @[Str] — 履歴
///@   - history_index: Int    — 現在の履歴位置 (-1 で履歴未参照中)
///@   - history_saved: Str    — 履歴 navigate 開始時の text 退避
///@   - completion:    CompletionState — 補完状態
///@   - action:        Int    — LineEditorAction タグ (editing=0, submitted=1, cancelled=2)
///@ Returns: LineEditorState ぶちパック
///@ AI-Context: 純粋な state machine の状態。lineEditorStep が key event を受けて
///@   新しい state を返す関数型編集器。副作用ゼロ (描画は lineEditorRender 経由)。
///@ AI-Hint: 直接構築せず lineEditorNew(opts) 経由で生成するのが安全。
///@   全 10 フィールドが必須。
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

// -- lineEditorNew --

///@ Purpose: Create a new LineEditorState from options
///@ Params:
///@   - opts: PromptOptions — 6 フィールド全必須の設定パック
///@ Returns: LineEditorState — text=opts.initial、cursor=opts.initial.length()、action=editing で初期化
///@ Example:
///@   opts <= PromptOptions(prompt <= "> ", initial <= "", placeholder <= "", mode <= PromptMode.normal, history <= @[], completion <= CompletionState)
///@   state <= lineEditorNew(opts)
///@ AI-Context: 初期 cursor 位置は text 末尾 (initial.length())。
///@   history_index = -1 で「履歴未参照中」を表す sentinel。
///@ AI-Hint: 新しいプロンプトを開始するたびにこの関数で fresh state を作る。
///@   既存 state を流用するとカーソル / 履歴位置の残存で UI が崩れる。
lineEditorNew opts =
  LineEditorState(text <= opts.initial, cursor <= opts.initial.length(), prompt <= opts.prompt, mode <= opts.mode, placeholder <= opts.placeholder, history <= opts.history, history_index <= -1, history_saved <= "", completion <= CompletionState, action <= LineEditorAction.editing)
=> :LineEditorState

// -- Internal: KeyKind tag literals --
//
// These mirror `KeyKind.<variant>` integer values pinned by the Rust
// addon (`src/key.rs`). Hard-coding the integers avoids importing the
// terminal facade circularly (prompt.td is a sub-module of terminal.td).
// The tag values are part of the v1 ABI and must not change.

kk_char       <= 0
kk_enter      <= 1
kk_escape     <= 2
kk_backspace  <= 4
kk_delete     <= 5
kk_arrow_up    <= 6
kk_arrow_down  <= 7
kk_arrow_left  <= 8
kk_arrow_right <= 9
kk_home       <= 10
kk_end        <= 11

// -- Internal: string manipulation helpers --

insertAt s pos ch =
  left <=< Slice[s, 0, pos]()
  right <=< Slice[s, pos, s.length()]()
  left + ch + right
=> :Str

deleteAt s pos =
  If[pos < 0, s, deleteAtInner(s, pos)]()
=> :Str

deleteAtInner s pos =
  If[pos == s.length(), s, deleteAtDo(s, pos)]()
=> :Str

deleteAtDo s pos =
  left <=< Slice[s, 0, pos]()
  right <=< Slice[s, pos + 1, s.length()]()
  left + right
=> :Str

// -- Internal: make state with new values helper --

makeState text cursor state action =
  LineEditorState(text <= text, cursor <= cursor, prompt <= state.prompt, mode <= state.mode, placeholder <= state.placeholder, history <= state.history, history_index <= state.history_index, history_saved <= state.history_saved, completion <= state.completion, action <= action)
=> :LineEditorState

makeStateHist text cursor state histIdx histSaved action =
  LineEditorState(text <= text, cursor <= cursor, prompt <= state.prompt, mode <= state.mode, placeholder <= state.placeholder, history <= state.history, history_index <= histIdx, history_saved <= histSaved, completion <= state.completion, action <= action)
=> :LineEditorState

// -- lineEditorStep --

///@ Purpose: Process one key event and return the next state
///@ Params:
///@   - state: LineEditorState — 現在の状態
///@   - key:   @(kind: Int, text: Str, ctrl: Bool, alt: Bool, shift: Bool) — readKey の戻り値
///@ Returns: LineEditorState — key の transition を反映した新状態 (action 含む)
///@ Example:
///@   key <= readKey()
///@   state <= lineEditorStep(state, key)
///@   state.action |== LineEditorAction.submitted => stdout("got: " + state.text)
///@ AI-Context: **副作用ゼロの pure transition**。state in / state out の関数型。
///@   action != editing (submitted / cancelled) の state には何もせず unchanged で
///@   返す (idempotent)。
///@   サポート操作:
///@     - char 入力: 現在 cursor 位置に挿入
///@     - Backspace: cursor 直前の char を削除
///@     - Delete:    cursor 位置の char を削除
///@     - Arrow Left/Right: cursor 移動
///@     - Home/End:  行頭 / 末尾へジャンプ
///@     - Arrow Up/Down: 履歴 navigate
///@     - Enter:     action <= submitted
///@     - Escape:    action <= cancelled
///@ AI-Hint: 不明な key は state を unchanged で返す (silent ignore)。
///@   readKey ループと組み合わせて使う:
///@     loop { key <= readKey(); state <= lineEditorStep(state, key); ... }
///@   長い入力では Slice + concat の累積コストが見える可能性があるため、
///@   大きなプロンプト履歴を扱う場合は呼び出し側で入力長を制限する。
lineEditorStep state key =
  If[state.action == LineEditorAction.editing, stepEditing(state, key), state]()
=> :LineEditorState

stepEditing state key =
  | key.kind == kk_enter      |> makeState(state.text, state.cursor, state, LineEditorAction.submitted)
  | key.kind == kk_escape     |> makeState(state.text, state.cursor, state, LineEditorAction.cancelled)
  | key.kind == kk_arrow_left  |> stepArrowLeft(state)
  | key.kind == kk_arrow_right |> stepArrowRight(state)
  | key.kind == kk_home       |> makeState(state.text, 0, state, LineEditorAction.editing)
  | key.kind == kk_end        |> makeState(state.text, state.text.length(), state, LineEditorAction.editing)
  | key.kind == kk_backspace  |> stepBackspace(state)
  | key.kind == kk_delete     |> stepDelete(state)
  | key.kind == kk_arrow_up    |> stepHistoryPrev(state)
  | key.kind == kk_arrow_down  |> stepHistoryNext(state)
  | key.kind == kk_char       |> stepInsertChar(state, key)
  | _ |> state
=> :LineEditorState

stepArrowLeft state =
  nc <= If[state.cursor > 0, state.cursor - 1, 0]()
  makeState(state.text, nc, state, LineEditorAction.editing)
=> :LineEditorState

stepArrowRight state =
  nc <= If[state.cursor < state.text.length(), state.cursor + 1, state.text.length()]()
  makeState(state.text, nc, state, LineEditorAction.editing)
=> :LineEditorState

stepBackspace state =
  If[state.cursor == 0, state, stepBackspaceDo(state)]()
=> :LineEditorState

stepBackspaceDo state =
  newText <= deleteAt(state.text, state.cursor - 1)
  makeState(newText, state.cursor - 1, state, LineEditorAction.editing)
=> :LineEditorState

stepDelete state =
  If[state.cursor == state.text.length(), state, stepDeleteDo(state)]()
=> :LineEditorState

stepDeleteDo state =
  newText <= deleteAt(state.text, state.cursor)
  makeState(newText, state.cursor, state, LineEditorAction.editing)
=> :LineEditorState

stepInsertChar state key =
  If[key.text == "", state, stepInsertCharDo(state, key)]()
=> :LineEditorState

stepInsertCharDo state key =
  newText <= insertAt(state.text, state.cursor, key.text)
  makeState(newText, state.cursor + key.text.length(), state, LineEditorAction.editing)
=> :LineEditorState

stepHistoryPrev state =
  If[state.history.length() == 0, state, stepHistoryPrevDo(state)]()
=> :LineEditorState

stepHistoryPrevDo state =
  new_idx <= (
    | state.history_index == -1 |> state.history.length() - 1
    | state.history_index > 0   |> state.history_index - 1
    | _ |> 0
  )
  saved <= If[state.history_index == -1, state.text, state.history_saved]()
  state.history.get(new_idx) >=> histText
  makeStateHist(histText, histText.length(), state, new_idx, saved, LineEditorAction.editing)
=> :LineEditorState

stepHistoryNext state =
  If[state.history_index == -1, state, stepHistoryNextDo(state)]()
=> :LineEditorState

stepHistoryNextDo state =
  new_idx <= state.history_index + 1
  | new_idx == state.history.length() |> makeStateHist(state.history_saved, state.history_saved.length(), state, -1, "", LineEditorAction.editing)
  | new_idx > state.history.length() |> makeStateHist(state.history_saved, state.history_saved.length(), state, -1, "", LineEditorAction.editing)
  | _ |> stepHistoryNextLoad(state, new_idx)
=> :LineEditorState

stepHistoryNextLoad state new_idx =
  state.history.get(new_idx) >=> histText
  makeStateHist(histText, histText.length(), state, new_idx, state.history_saved, LineEditorAction.editing)
=> :LineEditorState

// -- lineEditorRender --

///@ Purpose: Generate display string from current state
///@ Params:
///@   - state: LineEditorState — 描画する状態
///@ Returns: @(line: Str, cursor_col: Int)
///@   - line:       描画文字列 (clearLine prefix + prompt + display_text)
///@   - cursor_col: カーソル列 (1-based、prompt + cursor までの表示幅 + 1)
///@ Example:
///@   r <= lineEditorRender(state)
///@   write(r.line)
///@   write(cursorMoveTo(r.cursor_col, 1))
///@ AI-Context: line は `clearLine()` 出力で行頭から書き直すため、現在行を
///@   消去して再描画する。Password mode では text を "*" でマスク。
///@   text="" かつ placeholder!="" なら placeholder を表示 (cursor_col は prompt の直後)。
///@ AI-Hint: cursor_col は **行内の column** (1 始まり)、絶対画面位置ではない。
///@   現在の行が画面の何行目かは caller が `cursorMoveTo(r.cursor_col, current_row)` で組み合わせる。
///@   cursorWidthCalc が cursor 位置までの prefix を毎回再 slice するため、
///@   長い入力では描画コストが増える。
lineEditorRender state =
  display_text <= getDisplayText(state)
  prompt_width <= displayWidth(state.prompt)
  cursor_display_pos <= If[state.mode == PromptMode.password, state.cursor, cursorWidthCalc(state)]()
  cursor_col <= prompt_width + cursor_display_pos + 1
  @(line <= clearLine() + state.prompt + display_text, cursor_col <= cursor_col)
=> :@(line: Str, cursor_col: Int)

// Password-mode rendering uses Repeat so an N-character line stays linear.
getDisplayText state =
  | state.mode == PromptMode.password |> Repeat["*", state.text.length()]()
  | state.text == "" |>
    | state.placeholder == "" |> ""
    | _ |> state.placeholder
  | _ |> state.text
=> :Str

cursorWidthCalc state =
  prefix <=< Slice[state.text, 0, state.cursor]()
  displayWidth(prefix)
=> :Int

<<< @(PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, lineEditorNew, lineEditorStep, lineEditorRender)
