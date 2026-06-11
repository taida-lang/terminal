# Module: ansi.td

## Exports

- `clearScreen`
- `clearLine`
- `altScreenEnter`
- `altScreenLeave`
- `cursorMoveTo`
- `cursorHide`
- `cursorShow`
- `mouseTrackingEnter`
- `mouseTrackingLeave`

## Functions

### clearScreen

> Clear the entire screen and move cursor to (1,1)

**Returns**: `Str` - Str — ANSI escape sequence `ESC[2J ESC[H`

**Example**:

```taida
stdout(clearScreen())
```

**AI-Context**:
純粋関数。ANSI 文字列を返すだけで、stdout への書き出しは
呼び出し側が行う。TUI フレーム描画ではなく、CLI 起動時のクリアに使う。

**AI-SideEffects**:
- none

### clearLine

> Clear current line and move cursor to beginning

**Returns**: `Str` - Str — ANSI escape sequence `ESC[2K \r`

**Example**:

```taida
stdout(clearLine())
```

**AI-Context**:
行頭に戻る `\r` を含む。プロンプト再描画パターンで多用。
`lineEditorRender` 戻り値の line フィールドは既にこの prefix を含む。

**AI-SideEffects**:
- none

### altScreenEnter

> Switch to alternate screen buffer

**Returns**: `Str` - Str — ANSI escape sequence `ESC[?1049h`

**Example**:

```taida
stdout(altScreenEnter())
// ... TUI rendering ...
stdout(altScreenLeave())
```

**AI-Context**:
TUI の標準パターンの 1 つ。enter / leave は対で呼ぶ。
leave なしで終了するとターミナルにゴミが残る。

**AI-SideEffects**:
- none (ANSI string 返却のみ; 副作用は stdout 書き出し時)

### altScreenLeave

> Switch back to main screen buffer

**Returns**: `Str` - Str — ANSI escape sequence `ESC[?1049l`

**Example**:

```taida
stdout(altScreenLeave())
```

**AI-Context**:
`altScreenEnter` と対で必ず呼ぶ。プログラム終了時の
cleanup hook (e.g. signal handler) で出力するのが安全。

**AI-SideEffects**:
- none

### cursorMoveTo

> Move cursor to (col, row) position (1-based)

| Parameter | Type | Description |
|-----------|------|-------------|
| `col` | `-` | Int -- 1-based column (>= 1; col < 1 throws CursorMoveInvalidPosition) |
| `row` | `-` | Int -- 1-based row (>= 1; row < 1 throws CursorMoveInvalidPosition) |

**Returns**: `Str` - Str — ANSI escape sequence `ESC[<row>;<col>H`

**Throws**:
- - CursorMoveInvalidPosition: col < 1 or row < 1

**Example**:

```taida
stdout(cursorMoveTo(10, 5))   // col=10, row=5
```

**AI-Context**:
ANSI は (row;col) 順だが、本 facade は (col, row) 順で受ける
(renderer の cursor_col / cursor_row 順序と一致)。1-based に注意。

**AI-Hint**:
TUI 描画ループでは renderFrame が cursor 移動 ANSI を内部で
生成するので、この関数を frame 描画中に直接呼ぶ必要はない。

**AI-SideEffects**:
- none

### cursorHide

> Hide the cursor

**Returns**: `Str` - Str — ANSI escape sequence `ESC[?25l`

**Example**:

```taida
stdout(cursorHide())
```

**AI-Context**:
TUI 描画中のカーソル点滅を抑制したいときに使う。
プログラム終了時には cursorShow を必ず呼んで復元する。

**AI-SideEffects**:
- none

### cursorShow

> Show the cursor

**Returns**: `Str` - Str — ANSI escape sequence `ESC[?25h`

**Example**:

```taida
stdout(cursorShow())
```

**AI-Context**:
cursorHide と対で呼ぶ。終了 cleanup hook での明示が望ましい。

**AI-SideEffects**:
- none

### mouseTrackingEnter

> Enable mouse tracking (SGR 1006 + button/motion)

**Returns**: `Str` - Str — ANSI escape sequence `ESC[?1000h ESC[?1002h ESC[?1006h`

**Example**:

```taida
rawModeEnter()
stdout(mouseTrackingEnter())
event <= readEvent()  // event.kind == EventKind.mouse on click
stdout(mouseTrackingLeave())
rawModeLeave()
```

**AI-Context**:
3 つの ANSI シーケンスを連結: ?1000h (button report),
?1002h (button + motion report), ?1006h (SGR extended pixel coords)。
`readEvent` でマウスイベントを受け取るために raw mode と併用する。

**AI-Hint**:
enter / leave は必ず対で。leave 忘れるとシェルにマウス escape が
ダダ漏れになる。

**AI-SideEffects**:
- none

### mouseTrackingLeave

> Disable mouse tracking

**Returns**: `Str` - Str — ANSI escape sequence (mouseTrackingEnter の逆順 leave)

**Example**:

```taida
stdout(mouseTrackingLeave())
```

**AI-Context**:
enter で有効化した 3 モードをすべて解除。順序は逆。

**AI-SideEffects**:
- none

# Module: prompt.td

## Exports

- `PromptMode`
- `PromptOptions`
- `CompletionState`
- `LineEditorAction`
- `LineEditorState`
- `lineEditorNew`
- `lineEditorStep`
- `lineEditorRender`

## Functions

### lineEditorNew

> Create a new LineEditorState from options

| Parameter | Type | Description |
|-----------|------|-------------|
| `opts` | `-` | PromptOptions — 6 フィールド全必須の設定パック |

**Returns**: `LineEditorState` - LineEditorState — text=opts.initial、cursor=opts.initial.length()、action=editing で初期化

**Example**:

```taida
opts <= PromptOptions(prompt <= "> ", initial <= "", placeholder <= "", mode <= PromptMode.normal, history <= @[], completion <= CompletionState)
state <= lineEditorNew(opts)
```

**AI-Context**:
初期 cursor 位置は text 末尾 (initial.length())。
history_index = -1 で「履歴未参照中」を表す sentinel。

**AI-Hint**:
新しいプロンプトを開始するたびにこの関数で fresh state を作る。
既存 state を流用するとカーソル / 履歴位置の残存で UI が崩れる。

### insertAt

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |
| `ch` | `-` | - |

**Returns**: `Str`

### deleteAt

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |

**Returns**: `Str`

### deleteAtInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |

**Returns**: `Str`

### deleteAtDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `s` | `-` | - |
| `pos` | `-` | - |

**Returns**: `Str`

### makeState

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `cursor` | `-` | - |
| `state` | `-` | - |
| `action` | `-` | - |

**Returns**: `LineEditorState`

### makeStateHist

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `cursor` | `-` | - |
| `state` | `-` | - |
| `histIdx` | `-` | - |
| `histSaved` | `-` | - |
| `action` | `-` | - |

**Returns**: `LineEditorState`

### lineEditorStep

> Process one key event and return the next state

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | LineEditorState — 現在の状態 |
| `key` | `-` | @(kind: Int, text: Str, ctrl: Bool, alt: Bool, shift: Bool) — readKey の戻り値 |

**Returns**: `LineEditorState` - LineEditorState — key の transition を反映した新状態 (action 含む)

**Example**:

```taida
key <= readKey()
state <= lineEditorStep(state, key)
state.action |== LineEditorAction.submitted => stdout("got: " + state.text)
```

**AI-Context**:
**副作用ゼロの pure transition**。state in / state out の関数型。
action != editing (submitted / cancelled) の state には何もせず unchanged で
返す (idempotent)。
サポート操作:
- char 入力: 現在 cursor 位置に挿入
- Backspace: cursor 直前の char を削除
- Delete:    cursor 位置の char を削除
- Arrow Left/Right: cursor 移動
- Home/End:  行頭 / 末尾へジャンプ
- Arrow Up/Down: 履歴 navigate
- Enter:     action <= submitted
- Escape:    action <= cancelled

**AI-Hint**:
不明な key は state を unchanged で返す (silent ignore)。
readKey ループと組み合わせて使う:
loop { key <= readKey(); state <= lineEditorStep(state, key); ... }
長い入力では Slice + concat の累積コストが見える可能性があるため、
大きなプロンプト履歴を扱う場合は呼び出し側で入力長を制限する。

### stepEditing

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `key` | `-` | - |

**Returns**: `LineEditorState`

### stepArrowLeft

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepArrowRight

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepBackspace

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepBackspaceDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepDelete

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepDeleteDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepInsertChar

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `key` | `-` | - |

**Returns**: `LineEditorState`

### stepInsertCharDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `key` | `-` | - |

**Returns**: `LineEditorState`

### stepHistoryPrev

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepHistoryPrevDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepHistoryNext

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepHistoryNextDo

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `LineEditorState`

### stepHistoryNextLoad

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |
| `new_idx` | `-` | - |

**Returns**: `LineEditorState`

### lineEditorRender

> Generate display string from current state

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | LineEditorState — 描画する状態 |

**Returns**: `@(line: Str, cursor_col: Int)` - - cursor_col: カーソル列 (1-based、prompt + cursor までの表示幅 + 1)

**Example**:

```taida
r <= lineEditorRender(state)
write(r.line)
write(cursorMoveTo(r.cursor_col, 1))
```

**AI-Context**:
line は `clearLine()` 出力で行頭から書き直すため、現在行を
消去して再描画する。Password mode では text を "*" でマスク。
text="" かつ placeholder!="" なら placeholder を表示 (cursor_col は prompt の直後)。

**AI-Hint**:
cursor_col は **行内の column** (1 始まり)、絶対画面位置ではない。
現在の行が画面の何行目かは caller が `cursorMoveTo(r.cursor_col, current_row)` で組み合わせる。
cursorWidthCalc が cursor 位置までの prefix を毎回再 slice するため、
長い入力では描画コストが増える。

### getDisplayText

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### cursorWidthCalc

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Int`

## Bindings

### PromptMode

> Prompt display mode 列挙パック

**Returns**: PromptMode ぶちパック

**Example**:

```taida
opts <= PromptOptions(prompt <= "Password: ", initial <= "", placeholder <= "", mode <= PromptMode.password, history <= @[], completion <= CompletionState)
```

**AI-Context**:
PromptOptions.mode / LineEditorState.mode フィールドの値として使う。
タグ値は v1 で凍結 (再番号はマイグレーション必須)。

**AI-Hint**:
パスワード入力は Mode を切替えるだけで描画が "*" マスクされる。
state.text 自体には平文が入っているので、submit 後は呼出側で zeroize 推奨。

### PromptOptions

> Prompt configuration options

**Returns**: PromptOptions ぶちパック

**Example**:

```taida
opts <= PromptOptions(prompt <= "$ ", initial <= "", placeholder <= "type a command", mode <= PromptMode.normal, history <= @[], completion <= CompletionState)
```

**AI-Context**:
lineEditorNew に渡して LineEditorState を構築する設定パック。
全フィールドが必須 (ぶちパックの構造的部分型付け)。

### CompletionState

> Completion candidates state (v1 minimal)

**Returns**: CompletionState ぶちパック

**AI-Context**:
v1 では minimal placeholder 。フル補完 UI は将来拡張 (v2+)。
現状は LineEditorState.completion フィールドの type-shape 維持のため存在。

### LineEditorAction

> Result action from lineEditorStep 列挙パック

**Returns**: LineEditorAction ぶちパック

**Example**:

```taida
state.action |== LineEditorAction.submitted => stdout("input: " + state.text)
state.action |== LineEditorAction.cancelled => stdout("aborted")
```

**AI-Context**:
LineEditorState.action フィールドと比較する。
submitted / cancelled になったら caller はループを抜けて state.text を読む。

**AI-Hint**:
state.action |== LineEditorAction.editing の間 readKey + lineEditorStep の
ループを continue する pattern が定番。

### LineEditorState

> Line editor internal state (pure, no side effects)

**Returns**: LineEditorState ぶちパック

**AI-Context**:
純粋な state machine の状態。lineEditorStep が key event を受けて
新しい state を返す関数型編集器。副作用ゼロ (描画は lineEditorRender 経由)。

**AI-Hint**:
直接構築せず lineEditorNew(opts) 経由で生成するのが安全。
全 10 フィールドが必須。

# Module: renderer.td

## Exports

- `Cell`
- `CellStyle`
- `ScreenBuffer`
- `DiffOpKind`
- `DiffOp`

## Bindings

### Cell

> One cell of data (character + style)

**Returns**: Cell ぶちパック

**Example**:

```taida
c <= Cell(text <= "X", fg <= "red", bg <= "", bold <= true, dim <= false, underline <= false, italic <= false)
```

**AI-Context**:
Default text is " ". The renderer normalizes empty
text to " " for rendering. Wide-char placeholder cells also
carry " " as the second-half text. fg/bg は文字列名 (Color パレット
と同様)。256 色 / RGB は v1 の Cell では扱わない (style 引数で stylize* 済み
文字列を bufferWrite に渡す形を推奨)。

**AI-Hint**:
Cell を直接構築するより bufferPut / bufferWrite を使うのが安全。

### CellStyle

> Default style options for bufferWrite callers

**Returns**: CellStyle ぶちパック (6 フィールドの subset、Cell とは違って text を含まない)

**Example**:

```taida
s <= CellStyle(fg <= "cyan", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf2 <= bufferWrite(buf, 1, 1, "Title", s)
```

**AI-Context**:
bufferWrite style arg must be this 6-field shape. Use
`CellStyle(fg <= "red", bg <= "", bold <= false, dim <= false,
underline <= false, italic <= false)` — every field must be present.
1 つでも欠けるとぶちパックの構造的部分型付けで undefined になる。

**AI-Hint**:
bufferWrite の style 引数を組み立てるショートカットとして使う。

### ScreenBuffer

> Virtual screen buffer (row-major flat cells)

**Returns**: ScreenBuffer ぶちパック

**Example**:

```taida
buf <= bufferNew(80, 24)
buf2 <= bufferWrite(buf, 1, 1, "Hello", CellStyle)
```

**AI-Context**:
Use `bufferNew(cols, rows)` to allocate. Direct
construction is allowed but the cells list length must equal
cols*rows or the native renderer rejects the buffer with
`RendererInvalidArg`. Package-level buffer operations use native
primitives such as bufferPut / bufferWrite for hot paths.

**AI-Hint**:
TUI の典型 frame loop:
- prev <= bufferNew(cols, rows)
- loop { next <= compose(prev); frame <= renderFrame(prev, next); write(frame.text); prev <= frame.next }

### DiffOpKind

> Diff operation kind

**Returns**: DiffOpKind ぶちパック (5 variant 列挙)

**Example**:

```taida
diff <= bufferDiff(prev, next)
diff.ops |== first => first.kind |== DiffOpKind.write => stdout("write op")
```

**AI-Context**:
Tag values are frozen — the Rust `renderer::diff`
matches against these literals. Renumbering breaks the addon ABI.
variant は snake_case。Int tag は不変。

**AI-Hint**:
bufferDiff の戻り値 ops list の各要素の `kind` フィールドと比較。
直接 0..4 を書かず必ず `DiffOpKind.write` 等を経由する。

### DiffOp

> A single diff operation

**Returns**: DiffOp ぶちパック

**AI-Context**:
bufferDiff の出力要素。renderOps / renderFrame で ANSI 文字列に
展開される。ユーザコードは通常直接 DiffOp を構築しない (renderer 内部表現)。

# Module: style.td

## Exports

- `Color`
- `resetStyle`
- `stylize`
- `Color256`
- `ColorRgb`
- `stylize256`
- `stylizeRgb`

## Functions

### fgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### bgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### safeFgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### safeBgCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | `-` | - |

**Returns**: `Str`

### appendCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `acc` | `-` | - |
| `code` | `-` | - |

**Returns**: `Str`

### resetStyle

> Return ANSI reset sequence

**Returns**: `Str` - Str — ANSI escape sequence `ESC[0m`

**Example**:

```taida
stdout(resetStyle())
```

**AI-Context**:
stylize / stylize256 / stylizeRgb の戻り値は既に reset suffix を
含むため、通常はユーザコードで直接呼ぶ必要はない。生 SGR を組み立てる
advanced ケースでのみ末尾に append する。

**AI-SideEffects**:
- none

### stylize

> Apply color and decoration to text as ANSI string

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str — 装飾対象テキスト |
| `opts` | `-` | @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false) |

**Returns**: `Str` - Str — `ESC[<sgr>m` + text + `ESC[0m`、装飾なしなら text そのまま

**Throws**:
- - StylizeInvalidColor: 未知の fg / bg 色名 (Color パレット外)

**Example**:

```taida
stdout(stylize("hello", @(fg <= Color.red, bg <= "", bold <= true, dim <= false, underline <= false, italic <= false)))
```

**AI-Context**:
opts は **6 フィールド全てが必須**。1 つでも欠けると
ぶちパックの構造的部分型付けで undefined フィールド参照になる。
無装飾を表現するには空文字 / false を明示的に渡す。

**AI-Hint**:
256 色 / RGB を使いたい場合は stylize256 / stylizeRgb を使う。

**AI-SideEffects**:
- none

### validate256

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |
| `label` | `-` | - |

**Returns**: `Str`

### fg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### bg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### safeFg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### safeBg256Code

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `-` | - |

**Returns**: `Str`

### validateRgbComponent

| Parameter | Type | Description |
|-----------|------|-------------|
| `value` | `-` | - |
| `label` | `-` | - |
| `component` | `-` | - |

**Returns**: `Str`

### isNoColorRgb

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Bool`

### fgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### bgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### safeFgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### safeBgRgbCode

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |

**Returns**: `Str`

### validateRgbFull

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |
| `label` | `-` | - |

**Returns**: `Str`

### validateRgbG

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |
| `label` | `-` | - |
| `prev` | `-` | - |

**Returns**: `Str`

### validateRgbB

| Parameter | Type | Description |
|-----------|------|-------------|
| `color` | `-` | - |
| `label` | `-` | - |
| `prev` | `-` | - |

**Returns**: `Str`

### stylize256

> Apply 256-color styling to text

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str — 装飾対象テキスト |
| `opts` | `-` | @(fg, bg, bold, dim, underline, italic) |

**Returns**: `Str` - Str — `ESC[<sgr>m` + text + `ESC[0m`、無装飾なら text そのまま

**Throws**:
- - StylizeInvalidColor: index < -1 or index > 255

**Example**:

```taida
stdout(stylize256("ok", @(fg <= Color256(index <= 46), bg <= Color256(index <= -1), bold <= true, dim <= false, underline <= false, italic <= false)))
```

**AI-Context**:
256 色は 0-15 が basic 16 色、16-231 が 6×6×6 RGB cube、
232-255 が grayscale ramp。Color256(index <= -1) で「色なし」を表現する。

**AI-SideEffects**:
- none

### stylizeRgb

> Apply RGB color styling to text

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str — 装飾対象テキスト |
| `opts` | `-` | @(fg, bg, bold, dim, underline, italic) |

**Returns**: `Str` - Str — `ESC[38;2;<r>;<g>;<b>m` + text + `ESC[0m` 形

**Throws**:
- - StylizeInvalidColor: r / g / b いずれかが < -1 または > 255

**Example**:

```taida
blue <= ColorRgb(r <= 30, g <= 144, b <= 255)
stdout(stylizeRgb("info", @(fg <= blue, bg <= ColorRgb(r <= -1, g <= -1, b <= -1), bold <= false, dim <= false, underline <= false, italic <= false)))
```

**AI-Context**:
24-bit truecolor (16M 色) を SGR 38;2;r;g;b / 48;2;r;g;b で
出力。端末側 (e.g., GNOME Terminal, iTerm2) が COLORTERM=truecolor を
advertise している環境で有効。

**AI-Hint**:
256 色までで十分なら stylize256 のほうが端末互換性が高い。

**AI-SideEffects**:
- none

## Bindings

### Color

> Basic 16-color palette (name constants)

**Example**:

```taida
stdout(stylize("hello", @(fg <= Color.red, bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)))
stdout(stylize("warn", @(fg <= Color.bright_yellow, bg <= "", bold <= true, dim <= false, underline <= false, italic <= false)))
```

**AI-Context**:
Pass to stylize fg / bg arguments. 16 entries (8 base + 8 bright)。
全フィールドの値は文字列定数 (e.g. "red", "bright_blue") で、
`stylize` 内部で SGR コードに変換される。field 名は snake_case。

**AI-Hint**:
直接 SGR 数値を扱いたい場合は Color256 / ColorRgb を使う。

### Color256

> 256-color index (0-255) ぶちパック型

**Example**:

```taida
red256 <= Color256(index <= 196)
stdout(stylize256("alert", @(fg <= red256, bg <= Color256(index <= -1), bold <= true, dim <= false, underline <= false, italic <= false)))
```

**AI-Context**:
Pass to stylize256 fg / bg。`index <= -1` で「色指定なし」を
表現する sentinel 値。index 範囲外 (< -1 or > 255) は throw。

**AI-Hint**:
terminal が 256 色非対応の場合 (e.g. CI 上の `TERM=dumb`) でも
ANSI 文字列は出力されるが端末側で無視される。

### ColorRgb

> RGB color (each component 0-255) ぶちパック型

**Example**:

```taida
purple <= ColorRgb(r <= 128, g <= 0, b <= 128)
stdout(stylizeRgb("title", @(fg <= purple, bg <= ColorRgb(r <= -1, g <= -1, b <= -1), bold <= false, dim <= false, underline <= false, italic <= false)))
```

**AI-Context**:
Pass to stylizeRgb fg / bg。3 component 全 `-1` で「色指定なし」。
いずれかの component が範囲外 (< -1 or > 255) は throw。

**AI-Hint**:
24-bit color 非対応の端末では fallback されないため、フォールバック
が必要なら呼び出し側で env 変数 (`COLORTERM=truecolor`) を見て分岐する。

# Module: terminal.td

## Exports

- `terminalSize`
- `readKey`
- `KeyKind`
- `isTerminal`
- `rawModeEnter`
- `rawModeLeave`
- `clearScreen`
- `clearLine`
- `altScreenEnter`
- `altScreenLeave`
- `cursorMoveTo`
- `cursorHide`
- `cursorShow`
- `stylize`
- `Color`
- `resetStyle`
- `Color256`
- `ColorRgb`
- `stylize256`
- `stylizeRgb`
- `EventKind`
- `MouseKind`
- `readEvent`
- `mouseTrackingEnter`
- `mouseTrackingLeave`
- `write`
- `WidthMode`
- `measureGrapheme`
- `displayWidth`
- `normalizeCellText`
- `truncateWidth`
- `padWidth`
- `Cell`
- `CellStyle`
- `ScreenBuffer`
- `DiffOpKind`
- `DiffOp`
- `bufferNew`
- `bufferResize`
- `bufferClear`
- `bufferPut`
- `bufferWrite`
- `bufferFillRect`
- `bufferBlit`
- `renderFull`
- `bufferDiff`
- `renderOps`
- `renderFrame`
- `PromptMode`
- `PromptOptions`
- `CompletionState`
- `LineEditorAction`
- `LineEditorState`
- `lineEditorNew`
- `lineEditorStep`
- `lineEditorRender`
- `SpinnerState`
- `spinnerNext`
- `spinnerRender`
- `ProgressOptions`
- `progressBar`
- `statusLine`

## Functions

### terminalSize

> 現在の stdout ターミナルサイズをセル数で取得する

**Returns**: @(cols: Int, rows: Int) — 成功時は両方 1 以上

**Throws**:
- TerminalSizeNotATty: stdout が TTY ではない場合
- TerminalSizeIoctl: Unix ioctl または Windows console size query が失敗した場合

**Example**:

```taida
size <= terminalSize()
cols <= size.cols
rows <= size.rows
```

**AI-Context**:
レイアウト計算の入口。呼び出しごとに OS の現在値を読むため、resize 後は再取得する。

**AI-SideEffects**:
- stdout の端末サイズを問い合わせるだけで、端末状態は変更しない。

### readKey

> stdin から 1 キー分の入力を読み取り、KeyKind ベースのパックで返す

**Returns**: @(kind: Int, text: Str, ctrl: Bool, alt: Bool, shift: Bool) — kind は KeyKind と比較する

**Throws**:
- ReadKeyNotATty: stdin が TTY ではない場合
- ReadKeyRawMode: raw mode の開始または復元に失敗した場合
- ReadKeyEof: EOF を検出した場合
- ReadKeyInterrupted: シグナル割り込みが発生した場合

**Example**:

```taida
key <= readKey()
key.kind |== KeyKind.escape => write("cancel")
key.kind |== KeyKind.char => write(key.text)
```

**AI-Context**:
単発入力向け。イベントループで mouse / resize も扱う場合は rawModeEnter 後に readEvent を使う。

**AI-SideEffects**:
- 必要に応じて stdin を一時的に raw mode にし、読み取り後に復元する。

### isTerminal

> 指定ストリームが TTY かどうかを判定する

| Parameter | Type | Description |
|-----------|------|-------------|
| `stream` | `-` | 判定対象。値は "stdin" / "stdout" / "stderr" のいずれか |

**Returns**: Bool — 対象が TTY なら true

**Throws**:
- IsTerminalInvalidStream: stream が許可値以外の場合

**Example**:

```taida
interactive <= isTerminal("stdin")
canPaint <= isTerminal("stdout")
```

**AI-Context**:
CI や pipe 実行時の TUI fallback 判定に使う。未知 stream 名は false ではなく診断として扱う。

**AI-SideEffects**:
- TTY 判定の OS API を呼ぶ。端末状態は変更しない。

### rawModeEnter

> stdin を persistent raw mode に切り替え、後続の readEvent / readKey ループを可能にする

**Returns**: @(active: Bool) — 成功後は active=true

**Throws**:
- RawModeNotATty: stdin が TTY ではない場合
- RawModeAlreadyActive: 既に raw mode の場合
- RawModeEnterFailed: raw mode への切替に失敗した場合

**Example**:

```taida
entered <= rawModeEnter()
event <= readEvent()
left <= rawModeLeave()
```

**AI-Hint**:
rawModeEnter と rawModeLeave は必ず対にする。長時間 raw mode を保持する TUI では leave を終了処理に置く。

**AI-SideEffects**:
- stdin の端末設定を変更する。rawModeLeave で復元する。

### rawModeLeave

> rawModeEnter が保存した stdin の設定を復元する

**Returns**: @(active: Bool) — 成功後は active=false

**Throws**:
- RawModeNotActive: raw mode でない状態で呼んだ場合
- RawModeLeaveFailed: 端末設定の復元に失敗した場合

**Example**:

```taida
entered <= rawModeEnter()
key <= readKey()
left <= rawModeLeave()
```

**AI-Context**:
raw mode が有効でない場合は成功扱いにしない。呼び出し側は enter / leave の所有権を明確にする。

**AI-SideEffects**:
- rawModeEnter が保存した stdin の端末設定を復元する。

### readEvent

> raw mode 中のキーボード・マウス・リサイズイベントを 1 件読み取る

**Returns**: @(kind: Int, key: @(...), mouse: @(...), resize: @(cols: Int, rows: Int)) — kind は EventKind と比較する

**Throws**:
- ReadEventNotInRawMode: raw mode ではない状態で呼んだ場合
- ReadEventNotATty: stdin が TTY ではない場合
- ReadEventReadFailed: 入力読み取りに失敗した場合
- ReadEventEof: EOF を検出した場合
- ReadEventInterrupted: シグナル割り込みが発生した場合

**Example**:

```taida
entered <= rawModeEnter()
event <= readEvent()
event.kind |== EventKind.key => write(event.key.text)
left <= rawModeLeave()
```

**AI-Context**:
同じ stdin stream は 1 つの専用 blocking thread から読む。pending byte queue は thread-local。

**AI-Hint**:
mouse event を受けたい場合は rawModeEnter 後に mouseTrackingEnter の文字列を write する。

**AI-SideEffects**:
- stdin からブロッキング読み取りを行い、SIGWINCH / console resize 状態も観測する。

### write

> stdout に文字列を改行なしで即時書き出す

| Parameter | Type | Description |
|-----------|------|-------------|
| `bytes` | `-` | 書き出す UTF-8 文字列。ANSI escape sequence を含んでよい |

**Returns**: Int — 書き込んだ UTF-8 byte 数

**Throws**:
- WriteFailed: stdout write / flush に失敗した場合
- WriteBuildValue: 戻り値 Int のホスト側確保に失敗した場合
- WritePanic: write path 内で panic が発生した場合

**Example**:

```taida
n <= write("hello")
moved <= write(cursorMoveTo(1, 1))
```

**AI-Context**:
TUI の paint path では stdout ではなく write を使う。改行や flush policy を呼び出し側が制御できる。

**AI-SideEffects**:
- stdout に書き込み、flush する。改行は自動追加しない。

### bufferPut

> ScreenBuffer の指定セルを 1 つ置き換えた新しい buffer を返す

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | 更新元 ScreenBuffer |
| `col` | `-` | 1-based column。1 以上かつ buf.cols 以下 |
| `row` | `-` | 1-based row。1 以上かつ buf.rows 以下 |
| `cell` | `-` | 書き込む Cell |

**Returns**: ScreenBuffer — buf と同じ cols / rows を持つ新しい buffer

**Throws**:
- RendererOutOfBounds: 座標が buffer 外の場合
- RendererInvalidArg: buf または cell の shape が不正な場合

**Example**:

```taida
next <= bufferPut(buf, 1, 1, cell)
```

**AI-Context**:
buffer は immutable に扱う。戻り値を次フレームの状態として保持する。

### bufferWrite

> ScreenBuffer の指定位置から文字列を書き込み、表示幅に応じてカーソルを進める

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | 更新元 ScreenBuffer |
| `col` | `-` | 1-based column。開始位置は buffer 内 |
| `row` | `-` | 1-based row。開始位置は buffer 内 |
| `text` | `-` | 書き込む文字列。wide character は 2 cell 幅として扱う |
| `style` | `-` | text 全体に適用する CellStyle |

**Returns**: ScreenBuffer — 書き込み範囲だけが変わった新しい buffer

**Throws**:
- RendererInvalidArg: buffer / style shape が不正な場合
- RendererOutOfBounds: 開始座標が buffer 外の場合

**Example**:

```taida
next <= bufferWrite(buf, 2, 1, "title", style)
```

**AI-Hint**:
text が右端を超える部分は buffer 外に書かない。折り返しは呼び出し側で行う。

### bufferFillRect

> ScreenBuffer の矩形範囲を同じ Cell で埋める

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | 更新元 ScreenBuffer |
| `col` | `-` | 矩形左上の 1-based column |
| `row` | `-` | 矩形左上の 1-based row |
| `width` | `-` | 矩形幅。1 未満なら no-op |
| `height` | `-` | 矩形高さ。1 未満なら no-op |
| `cell` | `-` | 埋め込みに使う Cell |

**Returns**: ScreenBuffer — 指定矩形だけが変わった新しい buffer

**Throws**:
- RendererInvalidArg: buf または cell の shape が不正な場合
- RendererOutOfBounds: col / row が 1 未満の場合

**Example**:

```taida
panel <= bufferFillRect(buf, 1, 1, 20, 3, cell)
```

**AI-Hint**:
buffer 外にはみ出す右端・下端は切り詰めて塗る。

### bufferClear

> ScreenBuffer 全体を指定 Cell で塗り直す

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | 更新元 ScreenBuffer |
| `fill` | `-` | 全セルに入れる Cell |

**Returns**: ScreenBuffer — cols / rows を保ち、cells が fill で埋まった buffer

**Throws**:
- RendererInvalidArg: buf または fill の shape が不正な場合

**Example**:

```taida
cleared <= bufferClear(buf, blank)
```

**AI-Context**:
frame 開始時の full repaint 準備に使う。既存 buffer は変更しない。

### bufferDiff

> 2 つの ScreenBuffer の差分操作列を作る

| Parameter | Type | Description |
|-----------|------|-------------|
| `prev` | `-` | 直前に描画した ScreenBuffer |
| `next` | `-` | 次に描画したい ScreenBuffer |

**Returns**: @(ops: @[DiffOp], requires_full: Bool) — サイズ差異がある場合は requires_full=true

**Throws**:
- RendererInvalidArg: prev または next の shape が不正な場合

**Example**:

```taida
diff <= bufferDiff(prev, next)
text <= renderOps(diff.ops)
```

**AI-Context**:
renderFrame の下位 API。自前で paint scheduling する場合だけ直接使う。

### renderFull

> ScreenBuffer 全体を ANSI 文字列として描画する

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | 描画対象 ScreenBuffer |

**Returns**: Str — full repaint 用 ANSI escape sequence

**Throws**:
- RendererInvalidArg: buf の shape が不正な場合

**Example**:

```taida
text <= renderFull(buf)
bytes <= write(text)
```

**AI-Context**:
サイズ変更後や初回描画では renderFull を使う。通常 frame は renderFrame が差分を選ぶ。

### renderFrame

> prev から next への redraw frame を作り、次の基準 buffer を返す

| Parameter | Type | Description |
|-----------|------|-------------|
| `prev` | `-` | 直前に描画した ScreenBuffer |
| `next` | `-` | 次に描画したい ScreenBuffer |

**Returns**: @(text: Str, next: ScreenBuffer) — text を write し、戻り値 next を次回 prev にする

**Throws**:
- RendererInvalidArg: prev または next の shape が不正な場合

**Example**:

```taida
frame <= renderFrame(prev, next)
bytes <= write(frame.text)
prev <= frame.next
```

**AI-Hint**:
同一サイズなら diff、サイズ差異があれば full repaint に fallback する。

### renderOps

> DiffOp リストを ANSI 文字列に変換する

| Parameter | Type | Description |
|-----------|------|-------------|
| `ops` | `-` | bufferDiff が返した差分操作列 |

**Returns**: Str — cursor movement と style reset を含む差分描画文字列

**Throws**:
- RendererInvalidArg: ops の shape が不正な場合

**Example**:

```taida
diff <= bufferDiff(prev, next)
text <= renderOps(diff.ops)
```

**AI-Context**:
bufferDiff と renderOps を分けて instrumentation したい場合に使う。

### bufferBlit

> sub buffer を main buffer の指定位置へ合成する

| Parameter | Type | Description |
|-----------|------|-------------|
| `main` | `-` | 合成先 ScreenBuffer。戻り値はこのサイズを保つ |
| `sub` | `-` | 合成元 ScreenBuffer |
| `col` | `-` | main 内の左上 1-based column |
| `row` | `-` | main 内の左上 1-based row |

**Returns**: ScreenBuffer — sub が main 上に重なった新しい buffer

**Throws**:
- RendererInvalidArg: main または sub の shape が不正な場合
- RendererOutOfBounds: col / row が 1 未満の場合

**Example**:

```taida
composed <= bufferBlit(main, dialog, 10, 4)
```

**AI-Hint**:
main 外にはみ出す右端・下端は切り詰める。透明セルの概念はない。

### bufferNew

> 新しい ScreenBuffer を指定サイズで作る

| Parameter | Type | Description |
|-----------|------|-------------|
| `cols` | `-` | column count。1 以上 |
| `rows` | `-` | row count。1 以上 |

**Returns**: ScreenBuffer — default Cell で初期化された row-major buffer

**Throws**:
- RendererInvalidSize: cols または rows が 1 未満の場合

**Example**:

```taida
buf <= bufferNew(80, 24)
```

**AI-Context**:
terminalSize の戻り値を渡すのが通常の初期化 path。

### bufferResize

> ScreenBuffer を指定サイズへ resize した新しい buffer を返す

| Parameter | Type | Description |
|-----------|------|-------------|
| `buf` | `-` | resize 元 ScreenBuffer |
| `cols` | `-` | 新しい column count。1 以上 |
| `rows` | `-` | 新しい row count。1 以上 |
| `fill` | `-` | 新規領域に使う Cell |

**Returns**: ScreenBuffer — 指定サイズの buffer

**Throws**:
- RendererInvalidSize: cols または rows が 1 未満の場合
- RendererInvalidArg: buf または fill の shape が不正な場合

**Example**:

```taida
resized <= bufferResize(prev, size.cols, size.rows, blank)
```

**AI-Context**:
resize event 後に renderFrame へ渡す next buffer を作る。既存内容の保持範囲は実装契約に従う。

### measureGrapheme

> 文字列先頭の grapheme 表示幅と分類を測定する

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | 測定対象文字列。先頭 grapheme だけを評価する |

**Returns**: @(width: Int, mode: Int) — mode は WidthMode.narrow / wide / zero と比較する

**Example**:

```taida
g <= measureGrapheme("漢A")
w <= g.width
```

**AI-Context**:
renderer が wide character のセル消費を決めるための低レベル API。

### displayWidth

> 文字列全体の表示幅を terminal cell 数で返す

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | 測定対象文字列 |

**Returns**: Int — 結合文字と制御文字は 0、wide / fullwidth は 2、それ以外は 1 として合計する

**Example**:

```taida
width <= displayWidth("A漢")
```

**AI-Context**:
column alignment / truncation / progress bar 幅計算に使う。

### normalizeCellText

> renderer cell に入れる文字列を 1 cell 表示向けに正規化する

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | 正規化対象文字列 |

**Returns**: Str — 改行を除去し、空文字列は空白 1 文字へ正規化する

**Example**:

```taida
cellText <= normalizeCellText(input)
```

**AI-Context**:
Cell.text に入れる前の sanitize API。複数 cell 幅の処理は renderer が扱う。

### truncateWidth

> 表示幅が width 以内に収まる prefix へ切り詰める

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | 対象文字列 |
| `width` | `-` | 最大表示幅。1 未満なら空文字列を返す |

**Returns**: Str — wide character を途中で割らない prefix

**Example**:

```taida
clipped <= truncateWidth(label, 12)
```

**AI-Hint**:
省略記号は追加しない。必要なら呼び出し側で幅を確保してから連結する。

### padWidth

> 表示幅が width になるまで右側に空白を追加する

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | 対象文字列 |
| `width` | `-` | 目標表示幅 |

**Returns**: Str — 既に width 以上なら text をそのまま返す

**Example**:

```taida
padded <= padWidth(name, 20)
```

**AI-Context**:
table / status line の列揃えに使う。左 padding は提供しない。

## Bindings

### KeyKind

> キー種別を表す列挙パック（28バリアント）

**Returns**: KeyKind ぶちパック (28 variant)

**Example**:

```taida
key <= readKey()
key.kind |== KeyKind.enter => stdout("Enter pressed")
key.kind |== KeyKind.char  => stdout(key.text)
key.kind |== KeyKind.f5    => stdout("F5 hit")
```

**AI-Context**:
readKey / readEvent.key の戻り値 `kind` フィールドと比較。
タグ値（Int）は v1 ABI で凍結済み。追加・並び替えは ABI bump が必要。
variant は snake_case (`char`, `enter`, `arrow_up` 等)。
Rust 側 `src/key.rs` の Int tag 対応は不変。

**AI-Hint**:
ctrl/alt/shift 修飾キーは別フィールド (key.ctrl 等) で表現する。
`Ctrl+C` は `key.kind == KeyKind.char && key.text == "c" && key.ctrl == true`。

### EventKind

> イベント種別を表す列挙パック（4バリアント）

**Returns**: EventKind ぶちパック (4 variant)

**Example**:

```taida
event <= readEvent()
event.kind |== EventKind.key => stdout("Key event")
event.kind |== EventKind.mouse => stdout("Mouse event")
event.kind |== EventKind.resize => stdout("Resize event: " + event.resize.cols.toString())
```

**AI-Context**:
readEvent の戻り値 `kind` フィールドと比較して使う。
v1 ABI で凍結 (Int tag 不変)。
variant は snake_case。

**AI-Hint**:
event.kind に応じて event.key / event.mouse / event.resize の
どのフィールドを読むか分岐する。kind=unknown のときは何も読まない。

### MouseKind

> マウスイベント種別を表す列挙パック（6バリアント）

**Returns**: MouseKind ぶちパック (6 variant)

**Example**:

```taida
event <= readEvent()
event.kind |== EventKind.mouse =>
event.mouse.kind |== MouseKind.down => stdout("Click at " + event.mouse.col.toString())
event.mouse.kind |== MouseKind.scroll_up => stdout("Scroll up")
```

**AI-Context**:
readEvent の戻り値 `mouse.kind` フィールドと比較して使う。
v1 ABI で凍結 (Int tag 不変)。
variant は snake_case。

**AI-Hint**:
マウストラッキングを有効化していない (mouseTrackingEnter 未呼出) 場合、
readEvent はマウスイベントを emit しない。

# Module: widgets.td

## Exports

- `SpinnerState`
- `spinnerNext`
- `spinnerRender`
- `ProgressOptions`
- `progressBar`
- `statusLine`

## Functions

### spinnerNext

> Advance the spinner to the next frame

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | SpinnerState — 現在の状態 |

**Returns**: `SpinnerState` - SpinnerState — frame が 1 進んだ新状態 (done なら state そのまま)

**Example**:

```taida
s2 <= spinnerNext(s)
```

**AI-Context**:
state.done == true なら state を unchanged で返す (idempotent)。
フレーム数は 10 で循環 (frame_count = 10、Braille pattern)。

**AI-Hint**:
描画は外部ループで行う。本関数は **state transition のみ**。
frame 進行のタイミング (60 fps / 100ms 等) は caller が決める。

### spinnerNextInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `SpinnerState`

### spinnerRender

> Render the spinner as a display string

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | SpinnerState — 描画する状態 |

**Returns**: `Str` - Str — done なら "v" or "v <label>"、active なら "<frame_char>" or "<frame_char> <label>"

**Example**:

```taida
stdout(spinnerRender(s))   // "⠋ Loading" 等
```

**AI-Context**:
Braille pattern (⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏) を frame 0..9 に対応。
done 状態は ASCII "v" (チェック相当)。terminal の文字幅問題を避けて
ASCII にしている (将来 ✓/✔ への切替は config field で拡張可能)。

### spinnerDoneText

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### spinnerActiveText

| Parameter | Type | Description |
|-----------|------|-------------|
| `state` | `-` | - |

**Returns**: `Str`

### progressBar

> Render a progress bar string

| Parameter | Type | Description |
|-----------|------|-------------|
| `current` | `-` | Int — 現在進捗 (0..total、total 超過は total にクランプ) |
| `total` | `-` | Int — 全体 (>= 1) |
| `opts` | `-` | ProgressOptions — 装飾オプション (5 フィールド全必須) |

**Returns**: `Str` - Str — 1 行の表示文字列 (`<left_label> <bar> <right_label>` 形)

**Throws**:
- - ProgressInvalidTotal: total < 1
- - ProgressInvalidCurrent: current < 0

**Example**:

```taida
stdout(progressBar(50, 100, ProgressOptions(width <= 20, complete_char <= "#", incomplete_char <= "-", left_label <= "", right_label <= "50%")))
// "########## --------- 50%"
```

**AI-Context**:
左右ラベルは空文字列で省略可能。改行は付与しないので、
in-place 更新するなら caller が `\r` か cursorMoveTo で行頭に戻す。

**AI-Hint**:
同じ行で進捗を更新するパターン:
stdout("\r" + progressBar(i, n, opts))   // \r で行頭に戻して上書き
または cursorMoveTo(1, row) で固定行に描画

### progressBarInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `current` | `-` | - |
| `total` | `-` | - |
| `opts` | `-` | - |

**Returns**: `Str`

### repeatStr

| Parameter | Type | Description |
|-----------|------|-------------|
| `ch` | `-` | - |
| `count` | `-` | - |

**Returns**: `Str`

### statusLine

> Generate a status line with left/right text

| Parameter | Type | Description |
|-----------|------|-------------|
| `left` | `-` | Str — 左側テキスト |
| `right` | `-` | Str — 右側テキスト (右寄せ) |
| `width` | `-` | Int — 行幅 (0 ならパディングなしで連結のみ) |

**Returns**: `Str` - Str — width 幅の 1 行 (left + spaces + right、超過時は left を切詰め)

**Example**:

```taida
stdout(statusLine("file.txt", "[modified]", 60))
// "file.txt                                              [modified]"
```

**AI-Context**:
表示幅 (displayWidth) ベースで計算するので wide char (CJK) も
正しくスペース調整される。left + right が width を超過した場合、
left を truncateWidth(left, width - displayWidth(right)) で切り詰め、
right はそのまま表示。avail < 1 なら left を完全に drop して
`truncateWidth(right, width)` を返す。

**AI-Hint**:
TUI のフッターステータス行や tab タイトル行に使う基本 widget。

### statusLineInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `left` | `-` | - |
| `right` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

### statusLineTruncate

| Parameter | Type | Description |
|-----------|------|-------------|
| `left` | `-` | - |
| `right` | `-` | - |
| `width` | `-` | - |
| `right_w` | `-` | - |

**Returns**: `Str`

## Bindings

### SpinnerState

> Spinner state ぶちパック

**Returns**: SpinnerState ぶちパック

**Example**:

```taida
s <= SpinnerState(frame <= 0, label <= "Working", done <= false)
s2 <= spinnerNext(s)              // frame 0 -> 1
stdout(spinnerRender(s2))         // "⠙ Working"
```

**AI-Context**:
純粋な不変状態。spinnerNext で次状態を返す関数型 spinner。
描画タイミングは widget 側で持たない (caller がループで poll)。

**AI-Hint**:
起動時は `SpinnerState(frame <= 0, label <= "loading", done <= false)`、
完了時は `done <= true` をセットしたインスタンスに置き換える。

### ProgressOptions

> Progress bar options ぶちパック

**Returns**: ProgressOptions ぶちパック

**Example**:

```taida
opts <= ProgressOptions(width <= 30, complete_char <= "█", incomplete_char <= "░", left_label <= "Build", right_label <= "")
stdout(progressBar(7, 10, opts))   // "Build ████████████████████░░░░░░░░░░"
```

**AI-Context**:
progressBar に渡す設定。ラベルはバーの左右に空白区切りで append。
width < 1 は内部で 1 にクランプ (描画破綻防止)。

# Module: width.td

## Exports

- `WidthMode`
- `measureGrapheme`
- `displayWidth`
- `normalizeCellText`
- `truncateWidth`
- `padWidth`

## Functions

### inRange

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |
| `lo` | `-` | - |
| `hi` | `-` | - |

**Returns**: `Bool`

### isCombining

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |

**Returns**: `Bool`

### isWide

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |

**Returns**: `Bool`

### isControl

| Parameter | Type | Description |
|-----------|------|-------------|
| `cp` | `-` | - |

**Returns**: `Bool`

### measureGrapheme

> Measure the display width and category of a single grapheme

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str — the text to measure (only the first grapheme is measured) |

**Returns**: `@(width: Int, mode: Int)` - @(width: Int, mode: Int) — width はセル数 (0/1/2)、mode は WidthMode タグ

**Example**:

```taida
m <= measureGrapheme("a")    // @(width <= 1, mode <= WidthMode.narrow)
m <= measureGrapheme("あ")   // @(width <= 2, mode <= WidthMode.wide)
m <= measureGrapheme("")     // @(width <= 0, mode <= WidthMode.zero)
```

**AI-Context**:
width policy:
- ASCII printable (U+0020..U+007E) → width 1 / narrow
- 結合マーク / 制御文字 → width 0 / zero
- East Asian Wide / Fullwidth → width 2 / wide
- Ambiguous → width 1 / narrow (v1 決定、ja/zh ロケールでも narrow 扱い)
package-level import では native fast-path に切り替わり、本 facade は
sub-import 用 fallback。

**AI-Hint**:
文字列全体の幅は displayWidth を使う。本関数は **first grapheme のみ**。

### measureGraphemeInner

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |

**Returns**: `@(width: Int, mode: Int)`

### dwCalc

> Calculate the total display width (cell count) of a string

| Parameter | Type | Description |
|-----------|------|-------------|
| `src` | `-` | - |
| `idx` | `-` | - |
| `acc` | `-` | - |
| `len` | `-` | - |

**Returns**: `Int` - Int — display width in cells (0 以上)

**Example**:

```taida
displayWidth("hello")        // 5
displayWidth("あいう")        // 6 (wide × 3)
displayWidth("aあb")         // 4 (1 + 2 + 1)
displayWidth("")             // 0
```

**AI-Context**:
Hot path. package-level import では native に切替済、本 facade は
`widgets.td` / `prompt.td` の sub-import 経由で pure-Taida fallback として
利用される。typical inputs は短い (< 200 chars) ため O(N²) でも frame
budget を超えない。

**AI-Hint**:
表示幅 = ターミナルセル数。byte 数や char 数とは異なる。
切り詰めには truncateWidth、右側パディングには padWidth を使う。

### displayWidth

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |

**Returns**: `Int`

### normLoop

> Normalize cell text (TAB -> 4 spaces, newline -> strip, empty -> " ")

| Parameter | Type | Description |
|-----------|------|-------------|
| `src` | `-` | - |
| `idx` | `-` | - |
| `acc` | `-` | - |
| `len` | `-` | - |

**Returns**: `Str` - Str — 正規化済みテキスト (renderer Cell.text 制約を満たす)

**Example**:

```taida
normalizeCellText("a\tb")    // "a    b"  (TAB → 4 spaces)
normalizeCellText("a\nb")    // "ab"      (newline 除去)
normalizeCellText("")        // " "       (空文字 → 単一空白)
```

**AI-Context**:
ScreenBuffer の Cell.text は 1 grapheme 相当の文字列を期待し、
newline / TAB / 空文字を含むと renderer がカーソル位置を誤計算する。
bufferWrite が呼び出す前段でこの正規化を通すと安全。

**AI-Hint**:
通常 bufferWrite 内部で呼ばれるため、ユーザコードで直接呼ぶ
必要はあまりない。 raw 描画 (bufferPut) で wide char placeholder を
構築するときに使う。

### normFinish

| Parameter | Type | Description |
|-----------|------|-------------|
| `result` | `-` | - |

**Returns**: `Str`

### normalizeCellText

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |

**Returns**: `Str`

### twLoop

> Truncate text to fit within a given display width

| Parameter | Type | Description |
|-----------|------|-------------|
| `src` | `-` | - |
| `idx` | `-` | - |
| `acc` | `-` | - |
| `remaining` | `-` | - |
| `len` | `-` | - |

**Returns**: `Str` - Str — width 以内に収まる prefix。width<1 や text="" なら ""

**Example**:

```taida
truncateWidth("hello", 3)     // "hel"
truncateWidth("あいう", 3)     // "あ"   (wide=2、3 では 1 グリフのみ)
truncateWidth("hello", 10)    // "hello"
truncateWidth("hello", 0)     // ""
```

**AI-Context**:
wide char の境界を尊重 — 切り詰め後の last char が wide で
残幅 1 しかない場合、その wide char は drop される (中途半端な半角扱い
になるのを防ぐ)。

**AI-Hint**:
ステータスラインや tab title など、固定幅の slot に文字列を
詰める際の基本 primitive。ペアで padWidth と組み合わせて使う。

### truncateWidth

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | - |
| `width` | `-` | - |

**Returns**: `Str`

### padWidth

> Pad text with spaces on the right to reach a target display width

| Parameter | Type | Description |
|-----------|------|-------------|
| `text` | `-` | Str — パディング対象テキスト |
| `width` | `-` | Int — 目標表示幅 (セル数) |

**Returns**: `Str` - Str — width に達するまで右側に空白を付加。既に width 以上なら text そのまま

**Example**:

```taida
padWidth("ok", 5)         // "ok   "
padWidth("hello", 3)      // "hello" (no truncation; 既に超過)
padWidth("あ", 4)          // "あ  "  (wide=2 + 2 spaces = 4)
```

**AI-Context**:
`Repeat[" ", n]()` (Rust str::repeat 経由)
で O(N) の linear pad。statusLine / progressBar の右寄せに使う。

**AI-Hint**:
切り詰めは行わないため、`width < displayWidth(text)` の場合は
text がそのまま返る。固定幅の slot に詰めるときは
`padWidth(truncateWidth(text, width), width)` の合成パターンが定番。

## Bindings

### WidthMode

> Unicode width category enum pack

**Example**:

```taida
m <= measureGrapheme("あ")
m.mode |== WidthMode.wide => stdout("wide char (width 2)")
```

**AI-Context**:
Compare with measureGrapheme result `mode` field.
Tag values are frozen — the Rust `width.rs` matches against these
literals (narrow=0, wide=1, zero=2, ambiguous=3). 並び替え禁止 (ABI lock)。
variant は snake_case。Int tag は不変。

**AI-Hint**:
`m.mode == WidthMode.wide` のように比較する。直接 0/1/2/3 を
書くのは禁止 (lock 後の変更を吸収しないため)。

