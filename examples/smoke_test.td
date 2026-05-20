// smoke_test.td -- Facade smoke test for taida-lang/terminal
//
// This file is loaded by the Taida interpreter to verify that all
// facade .td files parse correctly, exports are resolvable, and
// basic function calls produce the expected return values.
//
// Run: taida examples/smoke_test.td
//   (requires the package to be installed or symlinked into .taida/deps/)
//
// Symbol references use current package names: camelCase functions,
// snake_case non-function variants, and PascalCase pack types.

>>> taida-lang/terminal => @(KeyKind, Color, resetStyle, clearScreen, clearLine, altScreenEnter, altScreenLeave, cursorMoveTo, cursorHide, cursorShow, mouseTrackingEnter, mouseTrackingLeave, stylize, Color256, ColorRgb, stylize256, stylizeRgb, WidthMode, measureGrapheme, displayWidth, normalizeCellText, truncateWidth, padWidth, Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, bufferNew, bufferPut, bufferWrite, bufferFillRect, bufferBlit, bufferClear, bufferResize, renderFull, bufferDiff, renderOps, renderFrame, PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, lineEditorNew, lineEditorStep, lineEditorRender, SpinnerState, spinnerNext, spinnerRender, ProgressOptions, progressBar, statusLine, EventKind, MouseKind)

// -- Type packs --
stdout("KeyKind.enter:" + KeyKind.enter.toString())
stdout("KeyKind.unknown:" + KeyKind.unknown.toString())
stdout("Color.red:" + Color.red)
stdout("Color.bright_white:" + Color.bright_white)
stdout("WidthMode.wide:" + WidthMode.wide.toString())
stdout("WidthMode.zero:" + WidthMode.zero.toString())
stdout("DiffOpKind.move_to:" + DiffOpKind.move_to.toString())
stdout("PromptMode.password:" + PromptMode.password.toString())
stdout("LineEditorAction.submitted:" + LineEditorAction.submitted.toString())
stdout("LineEditorAction.cancelled:" + LineEditorAction.cancelled.toString())
stdout("EventKind.resize:" + EventKind.resize.toString())
stdout("MouseKind.scroll_down:" + MouseKind.scroll_down.toString())

// -- ANSI facade functions --
stdout("resetStyle.len:" + resetStyle().length().toString())
stdout("clearScreen.len:" + clearScreen().length().toString())
stdout("clearLine.len:" + clearLine().length().toString())
stdout("altScreenEnter.len:" + altScreenEnter().length().toString())
stdout("altScreenLeave.len:" + altScreenLeave().length().toString())
stdout("cursorMoveTo.len:" + cursorMoveTo(10, 5).length().toString())
stdout("cursorHide.len:" + cursorHide().length().toString())
stdout("cursorShow.len:" + cursorShow().length().toString())
stdout("mouseTrackingEnter.len:" + mouseTrackingEnter().length().toString())
stdout("mouseTrackingLeave.len:" + mouseTrackingLeave().length().toString())

// -- Width functions --
stdout("measureGrapheme.A:" + measureGrapheme("A").width.toString())
stdout("displayWidth.hello:" + displayWidth("hello").toString())
stdout("displayWidth.empty:" + displayWidth("").toString())
stdout("normalizeCellText.empty:" + normalizeCellText("").length().toString())
stdout("truncateWidth.hello3:" + truncateWidth("hello", 3))
stdout("padWidth.hi5:" + padWidth("hi", 5).length().toString())

// -- stylize --
s1 <= stylize("x", @(fg <= "red", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
stdout("stylize.red.len:" + s1.length().toString())
s2 <= stylize("plain", @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
stdout("stylize.plain:" + s2)

// -- Line editor --
state <= lineEditorNew(@(prompt <= "> ", initial <= "hello", placeholder <= "", mode <= 0, history <= @["old"], completion <= CompletionState))
stdout("lineEditorNew.text:" + state.text)
stdout("lineEditorNew.cursor:" + state.cursor.toString())

// -- Spinner --
sp <= SpinnerState
stdout("SpinnerState.frame:" + sp.frame.toString())
sp2 <= spinnerNext(sp)
stdout("spinnerNext.frame:" + sp2.frame.toString())
sr <= spinnerRender(sp)
stdout("spinnerRender.len:" + sr.length().toString())

// -- Progress bar --
pb <= progressBar(50, 100, ProgressOptions)
stdout("progressBar.len:" + pb.length().toString())

// -- Status line --
sl <= statusLine("left", "right", 20)
stdout("statusLine.len:" + sl.length().toString())

// -- Renderer core --
buf0 <= bufferNew(3, 2)
stdout("bufferNew.cols:" + buf0.cols.toString())
stdout("bufferNew.rows:" + buf0.rows.toString())
stdout("bufferNew.cells.length:" + buf0.cells.length().toString())

// bufferPut replaces a single cell at (2, 1)
x_cell <= Cell(text <= "X", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf1 <= bufferPut(buf0, 2, 1, x_cell)
buf1.cells.get(1) >=> c1
stdout("bufferPut.text:" + c1.text)
buf1.cells.get(0) >=> c0
stdout("bufferPut.unchanged:" + c0.text)

// bufferWrite writes "ABC" starting at (1, 1) with plain style
plain <= CellStyle(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf2 <= bufferWrite(buf0, 1, 1, "ABC", plain)
buf2.cells.get(0) >=> w0
buf2.cells.get(1) >=> w1
buf2.cells.get(2) >=> w2
stdout("bufferWrite.0:" + w0.text)
stdout("bufferWrite.1:" + w1.text)
stdout("bufferWrite.2:" + w2.text)

// bufferWrite truncates past right edge: "ABCDE" on 3x2 drops DE
buf3 <= bufferWrite(buf0, 1, 1, "ABCDE", plain)
buf3.cells.get(2) >=> w2b
stdout("bufferWrite.truncated:" + w2b.text)

// bufferWrite with style preserves fg
red <= CellStyle(fg <= "red", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf4 <= bufferWrite(buf0, 1, 1, "R", red)
buf4.cells.get(0) >=> r0
stdout("bufferWrite.styled_fg:" + r0.fg)

// bufferFillRect fills a 2x1 area starting at (2, 2)
fill_cell <= Cell(text <= "*", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf5 <= bufferFillRect(buf0, 2, 2, 2, 1, fill_cell)
buf5.cells.get(4) >=> f4
buf5.cells.get(5) >=> f5
stdout("bufferFillRect.4:" + f4.text)
stdout("bufferFillRect.5:" + f5.text)

// renderFull emits a non-empty ANSI string with cursorHide and cursorMoveTo(1,1)
full <= renderFull(buf2)
stdout("renderFull.len_gt10:" + If[full.length() > 10, "yes", "no"]())

// bufferDiff on identical buffers returns empty ops
diff0 <= bufferDiff(buf0, buf0)
stdout("bufferDiff.identical.ops_len:" + diff0.ops.length().toString())
stdout("bufferDiff.identical.requires_full:" + diff0.requires_full.toString())

// bufferDiff of buf0 vs buf1 (one cell changed at (2,1)) emits one Write op
diff1 <= bufferDiff(buf0, buf1)
stdout("bufferDiff.single_change.ops_len:" + diff1.ops.length().toString())
diff1.ops.get(0) >=> op0
stdout("bufferDiff.single_change.op_kind:" + op0.kind.toString())
stdout("bufferDiff.single_change.op_col:" + op0.col.toString())
stdout("bufferDiff.single_change.op_row:" + op0.row.toString())
stdout("bufferDiff.single_change.op_text:" + op0.text)

// bufferDiff on different sizes → requires_full=true
buf6 <= bufferNew(5, 3)
diff2 <= bufferDiff(buf0, buf6)
stdout("bufferDiff.size_change.requires_full:" + diff2.requires_full.toString())

// renderOps emits ANSI for the single-change op list
rendered <= renderOps(diff1.ops)
stdout("renderOps.len_gt0:" + If[rendered.length() > 0, "yes", "no"]())

// renderFrame fallbacks to renderFull on size change
frame_full <= renderFrame(buf0, buf6)
stdout("renderFrame.size_change.len_gt10:" + If[frame_full.text.length() > 10, "yes", "no"]())

// renderFrame emits minimal diff for single-cell change
frame_diff <= renderFrame(buf0, buf1)
stdout("renderFrame.diff.len_gt0:" + If[frame_diff.text.length() > 0, "yes", "no"]())

// renderFrame on identical buffers emits empty text
frame_same <= renderFrame(buf0, buf0)
stdout("renderFrame.same.empty:" + If[frame_same.text == "", "yes", "no"]())

// -- bufferBlit --
// Identity blit: 3×2 sub into 3×2 main at (1,1) → every cell overwritten.
mainA <= bufferNew(3, 2)
subA  <= bufferWrite(bufferNew(3, 2), 1, 1, "abc", plain)
subA2 <= bufferWrite(subA, 1, 2, "def", plain)
blit0 <= bufferBlit(mainA, subA2, 1, 1)
blit0.cells.get(0) >=> blit0_0
blit0.cells.get(5) >=> blit0_5
stdout("bufferBlit.identity.0:" + blit0_0.text)
stdout("bufferBlit.identity.5:" + blit0_5.text)

// Partial blit: 2×1 sub into 5×3 main at (3,2) → sub lands at cells[7], [8].
mainB <= bufferNew(5, 3)
subB  <= bufferWrite(bufferNew(2, 1), 1, 1, "XY", plain)
blit1 <= bufferBlit(mainB, subB, 3, 2)
blit1.cells.get(7) >=> blit1_7
blit1.cells.get(8) >=> blit1_8
blit1.cells.get(9) >=> blit1_9
stdout("bufferBlit.partial.7:" + blit1_7.text)
stdout("bufferBlit.partial.8:" + blit1_8.text)
stdout("bufferBlit.partial.untouched:" + blit1_9.text)

// Out-of-bounds clip: 3×3 sub into 4×4 main at (3,3) → only 2×2 copied.
mainC <= bufferNew(4, 4)
subC  <= bufferFillRect(bufferNew(3, 3), 1, 1, 3, 3, fill_cell)
blit2 <= bufferBlit(mainC, subC, 3, 3)
// Last cell of row 3 = index 11 (row-major, 4-wide, row 3 starts at 8).
// Cells at (3,3) / (4,3) / (3,4) / (4,4) → indices 10 / 11 / 14 / 15.
blit2.cells.get(10) >=> blit2_10
blit2.cells.get(15) >=> blit2_15
// (5,3) would be outside main (only 4 cols) — assert (4,3) came through.
blit2.cells.get(9) >=> blit2_9
stdout("bufferBlit.clip.10:" + blit2_10.text)
stdout("bufferBlit.clip.15:" + blit2_15.text)
stdout("bufferBlit.clip.untouched:" + blit2_9.text)

// Style preserve: blit a styled sub into main, check fg carries over.
mainD <= bufferNew(3, 1)
subD  <= bufferWrite(bufferNew(3, 1), 1, 1, "R", red)
blit3 <= bufferBlit(mainD, subD, 1, 1)
blit3.cells.get(0) >=> blit3_0
stdout("bufferBlit.style_fg:" + blit3_0.fg)

// Start past main right edge → no-op (main returned unchanged).
mainE <= bufferWrite(bufferNew(3, 1), 1, 1, "ORG", plain)
blit4 <= bufferBlit(mainE, subB, 10, 1)
blit4.cells.get(0) >=> blit4_0
blit4.cells.get(1) >=> blit4_1
blit4.cells.get(2) >=> blit4_2
stdout("bufferBlit.past_right.0:" + blit4_0.text)
stdout("bufferBlit.past_right.1:" + blit4_1.text)
stdout("bufferBlit.past_right.2:" + blit4_2.text)

// Wide-char boundary:
mainF  <= bufferFillRect(bufferNew(4, 1), 1, 1, 4, 1, @(text <= ".", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
subF   <= bufferWrite(bufferNew(2, 1), 1, 1, "漢", plain)
blit5  <= bufferBlit(mainF, subF, 4, 1)
blit5.cells.get(3) >=> blit5_3
stdout("bufferBlit.wide_edge_drop.3:" + blit5_3.text)

// Positive control: same sub placed so both lead AND placeholder fit
mainG  <= bufferFillRect(bufferNew(4, 1), 1, 1, 4, 1, @(text <= ".", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
blit6  <= bufferBlit(mainG, subF, 3, 1)
blit6.cells.get(2) >=> blit6_2
blit6.cells.get(3) >=> blit6_3
stdout("bufferBlit.wide_inbounds.2:" + blit6_2.text)
stdout("bufferBlit.wide_inbounds.3:" + blit6_3.text)

stdout("PASS:renderer_ops")
stdout("PASS:all_smoke_tests")
