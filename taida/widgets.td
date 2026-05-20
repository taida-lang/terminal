// taida/widgets.td
//
// UX widgets: spinner, progress bar, status line.
//
// This is the pure widget layer:
//   - No syscalls
//   - No hidden side effects
//   - Widgets return Str or pure state
//   - Redraw timing is NOT owned by widget
//   - Long-running animation loops are controlled externally
//
// Public names follow the current Taida package conventions: functions are
// camelCase, enum-like fields are snake_case, and pack types are PascalCase.

// -- Imports --

>>> ./width.td => @(displayWidth, truncateWidth, padWidth)

// -- Spinner --

spinner_frames <= @["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
spinner_frame_count <= 10

///@ Purpose: Spinner state ぶちパック
///@ Params:
///@   - frame: Int  — 現在のフレームインデックス (0..9 循環)
///@   - label: Str  — スピナー右側に表示するラベル ("" で非表示)
///@   - done:  Bool — true で停止状態 (✓ 表示)
///@ Returns: SpinnerState ぶちパック
///@ AI-Context: 純粋な不変状態。spinnerNext で次状態を返す関数型 spinner。
///@   描画タイミングは widget 側で持たない (caller がループで poll)。
///@ AI-Hint: 起動時は `SpinnerState(frame <= 0, label <= "loading", done <= false)`、
///@   完了時は `done <= true` をセットしたインスタンスに置き換える。
///@ Example:
///@   s <= SpinnerState(frame <= 0, label <= "Working", done <= false)
///@   s2 <= spinnerNext(s)              // frame 0 -> 1
///@   stdout(spinnerRender(s2))         // "⠙ Working"
SpinnerState <= @(
  frame <= 0
  label <= ""
  done  <= false
)

///@ Purpose: Advance the spinner to the next frame
///@ Params:
///@   - state: SpinnerState — 現在の状態
///@ Returns: SpinnerState — frame が 1 進んだ新状態 (done なら state そのまま)
///@ Example:
///@   s2 <= spinnerNext(s)
///@ AI-Context: state.done == true なら state を unchanged で返す (idempotent)。
///@   フレーム数は 10 で循環 (frame_count = 10、Braille pattern)。
///@ AI-Hint: 描画は外部ループで行う。本関数は **state transition のみ**。
///@   frame 進行のタイミング (60 fps / 100ms 等) は caller が決める。
spinnerNext state =
  If[state.done == true, state, spinnerNextInner(state)]()
=> :SpinnerState

spinnerNextInner state =
  inc <= state.frame + 1
  div_result <=< Div[inc, spinner_frame_count]()
  next_frame <= inc - div_result * spinner_frame_count
  SpinnerState(frame <= next_frame, label <= state.label, done <= state.done)
=> :SpinnerState

///@ Purpose: Render the spinner as a display string
///@ Params:
///@   - state: SpinnerState — 描画する状態
///@ Returns: Str — done なら "v" or "v <label>"、active なら "<frame_char>" or "<frame_char> <label>"
///@ Example:
///@   stdout(spinnerRender(s))   // "⠋ Loading" 等
///@ AI-Context: Braille pattern (⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏) を frame 0..9 に対応。
///@   done 状態は ASCII "v" (チェック相当)。terminal の文字幅問題を避けて
///@   ASCII にしている (将来 ✓/✔ への切替は config field で拡張可能)。
spinnerRender state =
  If[state.done == true, spinnerDoneText(state), spinnerActiveText(state)]()
=> :Str

spinnerDoneText state =
  If[state.label == "", "v", "v " + state.label]()
=> :Str

spinnerActiveText state =
  spinner_frames.get(state.frame) >=> frame_char
  If[state.label == "", frame_char, frame_char + " " + state.label]()
=> :Str

// -- Progress Bar --

///@ Purpose: Progress bar options ぶちパック
///@ Params:
///@   - width:           Int = 20 — バー全体の幅 (セル数)
///@   - complete_char:   Str = "#" — 完了部分の文字 (1 grapheme)
///@   - incomplete_char: Str = "-" — 未完了部分の文字 (1 grapheme)
///@   - left_label:      Str = "" — バー左側に表示するラベル
///@   - right_label:     Str = "" — バー右側に表示するラベル
///@ Returns: ProgressOptions ぶちパック
///@ AI-Context: progressBar に渡す設定。ラベルはバーの左右に空白区切りで append。
///@   width < 1 は内部で 1 にクランプ (描画破綻防止)。
///@ Example:
///@   opts <= ProgressOptions(width <= 30, complete_char <= "█", incomplete_char <= "░", left_label <= "Build", right_label <= "")
///@   stdout(progressBar(7, 10, opts))   // "Build ████████████████████░░░░░░░░░░"
ProgressOptions <= @(
  width           <= 20
  complete_char   <= "#"
  incomplete_char <= "-"
  left_label      <= ""
  right_label     <= ""
)

///@ Purpose: Render a progress bar string
///@ Params:
///@   - current: Int — 現在進捗 (0..total、total 超過は total にクランプ)
///@   - total:   Int — 全体 (>= 1)
///@   - opts:    ProgressOptions — 装飾オプション (5 フィールド全必須)
///@ Returns: Str — 1 行の表示文字列 (`<left_label> <bar> <right_label>` 形)
///@ Throws:
///@   - ProgressInvalidTotal: total < 1
///@   - ProgressInvalidCurrent: current < 0
///@ Example:
///@   stdout(progressBar(50, 100, ProgressOptions(width <= 20, complete_char <= "#", incomplete_char <= "-", left_label <= "", right_label <= "50%")))
///@   // "########## --------- 50%"
///@ AI-Context: 左右ラベルは空文字列で省略可能。改行は付与しないので、
///@   in-place 更新するなら caller が `\r` か cursorMoveTo で行頭に戻す。
///@ AI-Hint: 同じ行で進捗を更新するパターン:
///@   stdout("\r" + progressBar(i, n, opts))   // \r で行頭に戻して上書き
///@   または cursorMoveTo(1, row) で固定行に描画
progressBar current total opts =
  | total < 1   |> Error(type <= "ProgressInvalidTotal", message <= "total must be >= 1").throw()
  | current < 0 |> Error(type <= "ProgressInvalidCurrent", message <= "current must be >= 0").throw()
  | _ |> progressBarInner(current, total, opts)
=> :Str

progressBarInner current total opts =
  clamped <= If[current > total, total, current]()
  bar_width <= If[opts.width < 1, 1, opts.width]()
  filled <=< Div[clamped * bar_width, total]()
  empty <= bar_width - filled
  fill_str <= repeatStr(opts.complete_char, filled)
  empty_str <= repeatStr(opts.incomplete_char, empty)
  bar <= fill_str + empty_str
  with_left <= If[opts.left_label == "", bar, opts.left_label + " " + bar]()
  If[opts.right_label == "", with_left, with_left + " " + opts.right_label]()
=> :Str

// Repeat routes through Rust str::repeat, keeping repeated fill text linear.
repeatStr ch count =
  If[count < 1, "", Repeat[ch, count]()]()
=> :Str

// -- Status Line --

///@ Purpose: Generate a status line with left/right text
///@ Params:
///@   - left:  Str — 左側テキスト
///@   - right: Str — 右側テキスト (右寄せ)
///@   - width: Int — 行幅 (0 ならパディングなしで連結のみ)
///@ Returns: Str — width 幅の 1 行 (left + spaces + right、超過時は left を切詰め)
///@ Example:
///@   stdout(statusLine("file.txt", "[modified]", 60))
///@   // "file.txt                                              [modified]"
///@ AI-Context: 表示幅 (displayWidth) ベースで計算するので wide char (CJK) も
///@   正しくスペース調整される。left + right が width を超過した場合、
///@   left を truncateWidth(left, width - displayWidth(right)) で切り詰め、
///@   right はそのまま表示。avail < 1 なら left を完全に drop して
///@   `truncateWidth(right, width)` を返す。
///@ AI-Hint: TUI のフッターステータス行や tab タイトル行に使う基本 widget。
statusLine left right width =
  If[width == 0, left + right, statusLineInner(left, right, width)]()
=> :Str

statusLineInner left right width =
  left_w <= displayWidth(left)
  right_w <= displayWidth(right)
  gap <= width - left_w - right_w
  | gap > 0 |> left + repeatStr(" ", gap) + right
  | gap == 0 |> left + right
  | _ |> statusLineTruncate(left, right, width, right_w)
=> :Str

statusLineTruncate left right width right_w =
  avail <= width - right_w
  If[avail < 1, truncateWidth(right, width), truncateWidth(left, avail) + right]()
=> :Str

<<< @(SpinnerState, spinnerNext, spinnerRender, ProgressOptions, progressBar, statusLine)
