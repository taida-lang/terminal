// smoke_test.td -- Facade smoke test for taida-lang/terminal
//
// This file is loaded by the Taida interpreter to verify that all
// facade .td files parse correctly, exports are resolvable, and
// basic function calls produce the expected return values.
//
// Run: taida examples/smoke_test.td
//   (requires the package to be installed or symlinked into .taida/deps/)

>>> taida-lang/terminal => @(KeyKind, Color, ResetStyle, ClearScreen, ClearLine, AltScreenEnter, AltScreenLeave, CursorMoveTo, CursorHide, CursorShow, MouseTrackingEnter, MouseTrackingLeave, Stylize, Color256, ColorRgb, Stylize256, StylizeRgb, WidthMode, MeasureGrapheme, DisplayWidth, NormalizeCellText, TruncateWidth, PadWidth, Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, BufferNew, BufferPut, BufferWrite, BufferFillRect, BufferBlit, BufferClear, BufferResize, RenderFull, BufferDiff, RenderOps, RenderFrame, PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, LineEditorNew, LineEditorStep, LineEditorRender, SpinnerState, SpinnerNext, SpinnerRender, ProgressOptions, ProgressBar, StatusLine, EventKind, MouseKind)

// -- Type packs --
stdout("KeyKind.Enter:" + KeyKind.Enter.toString())
stdout("KeyKind.Unknown:" + KeyKind.Unknown.toString())
stdout("Color.red:" + Color.red)
stdout("Color.bright_white:" + Color.bright_white)
stdout("WidthMode.Wide:" + WidthMode.Wide.toString())
stdout("WidthMode.Zero:" + WidthMode.Zero.toString())
stdout("DiffOpKind.MoveTo:" + DiffOpKind.MoveTo.toString())
stdout("PromptMode.Password:" + PromptMode.Password.toString())
stdout("LineEditorAction.Submitted:" + LineEditorAction.Submitted.toString())
stdout("LineEditorAction.Cancelled:" + LineEditorAction.Cancelled.toString())
stdout("EventKind.Resize:" + EventKind.Resize.toString())
stdout("MouseKind.ScrollDown:" + MouseKind.ScrollDown.toString())

// -- ANSI facade functions --
stdout("ResetStyle.len:" + ResetStyle().length().toString())
stdout("ClearScreen.len:" + ClearScreen().length().toString())
stdout("ClearLine.len:" + ClearLine().length().toString())
stdout("AltScreenEnter.len:" + AltScreenEnter().length().toString())
stdout("AltScreenLeave.len:" + AltScreenLeave().length().toString())
stdout("CursorMoveTo.len:" + CursorMoveTo(10, 5).length().toString())
stdout("CursorHide.len:" + CursorHide().length().toString())
stdout("CursorShow.len:" + CursorShow().length().toString())
stdout("MouseTrackingEnter.len:" + MouseTrackingEnter().length().toString())
stdout("MouseTrackingLeave.len:" + MouseTrackingLeave().length().toString())

// -- Width functions --
stdout("MeasureGrapheme.A:" + MeasureGrapheme("A").width.toString())
stdout("DisplayWidth.hello:" + DisplayWidth("hello").toString())
stdout("DisplayWidth.empty:" + DisplayWidth("").toString())
stdout("NormalizeCellText.empty:" + NormalizeCellText("").length().toString())
stdout("TruncateWidth.hello3:" + TruncateWidth("hello", 3))
stdout("PadWidth.hi5:" + PadWidth("hi", 5).length().toString())

// -- Stylize --
s1 <= Stylize("x", @(fg <= "red", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
stdout("Stylize.red.len:" + s1.length().toString())
s2 <= Stylize("plain", @(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
stdout("Stylize.plain:" + s2)

// -- Line editor --
state <= LineEditorNew(@(prompt <= "> ", initial <= "hello", placeholder <= "", mode <= 0, history <= @["old"], completion <= @()))
stdout("LineEditorNew.text:" + state.text)
stdout("LineEditorNew.cursor:" + state.cursor.toString())

// -- Spinner --
sp <= SpinnerState
stdout("SpinnerState.frame:" + sp.frame.toString())
sp2 <= SpinnerNext(sp)
stdout("SpinnerNext.frame:" + sp2.frame.toString())
sr <= SpinnerRender(sp)
stdout("SpinnerRender.len:" + sr.length().toString())

// -- Progress bar --
pb <= ProgressBar(50, 100, ProgressOptions)
stdout("ProgressBar.len:" + pb.length().toString())

// -- Status line --
sl <= StatusLine("left", "right", 20)
stdout("StatusLine.len:" + sl.length().toString())

// -- Renderer core (TMB-019 / Phase 7) --
buf0 <= BufferNew(3, 2)
stdout("BufferNew.cols:" + buf0.cols.toString())
stdout("BufferNew.rows:" + buf0.rows.toString())
stdout("BufferNew.cells.length:" + buf0.cells.length().toString())

// BufferPut replaces a single cell at (2, 1)
xCell <= Cell(text <= "X", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf1 <= BufferPut(buf0, 2, 1, xCell)
buf1.cells.get(1) ]=> c1
stdout("BufferPut.text:" + c1.text)
buf1.cells.get(0) ]=> c0
stdout("BufferPut.unchanged:" + c0.text)

// BufferWrite writes "ABC" starting at (1, 1) with plain style
plain <= CellStyle(fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf2 <= BufferWrite(buf0, 1, 1, "ABC", plain)
buf2.cells.get(0) ]=> w0
buf2.cells.get(1) ]=> w1
buf2.cells.get(2) ]=> w2
stdout("BufferWrite.0:" + w0.text)
stdout("BufferWrite.1:" + w1.text)
stdout("BufferWrite.2:" + w2.text)

// BufferWrite truncates past right edge: "ABCDE" on 3x2 drops DE
buf3 <= BufferWrite(buf0, 1, 1, "ABCDE", plain)
buf3.cells.get(2) ]=> w2b
stdout("BufferWrite.truncated:" + w2b.text)

// BufferWrite with style preserves fg
red <= CellStyle(fg <= "red", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf4 <= BufferWrite(buf0, 1, 1, "R", red)
buf4.cells.get(0) ]=> r0
stdout("BufferWrite.styled_fg:" + r0.fg)

// BufferFillRect fills a 2x1 area starting at (2, 2)
fillCell <= Cell(text <= "*", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false)
buf5 <= BufferFillRect(buf0, 2, 2, 2, 1, fillCell)
buf5.cells.get(4) ]=> f4
buf5.cells.get(5) ]=> f5
stdout("BufferFillRect.4:" + f4.text)
stdout("BufferFillRect.5:" + f5.text)

// RenderFull emits a non-empty ANSI string with CursorHide and CursorMoveTo(1,1)
full <= RenderFull(buf2)
stdout("RenderFull.len_gt10:" + If[full.length() > 10, "yes", "no"]())

// BufferDiff on identical buffers returns empty ops
diff0 <= BufferDiff(buf0, buf0)
stdout("BufferDiff.identical.ops_len:" + diff0.ops.length().toString())
stdout("BufferDiff.identical.requires_full:" + diff0.requires_full.toString())

// BufferDiff of buf0 vs buf1 (one cell changed at (2,1)) emits one Write op
diff1 <= BufferDiff(buf0, buf1)
stdout("BufferDiff.single_change.ops_len:" + diff1.ops.length().toString())
diff1.ops.get(0) ]=> op0
stdout("BufferDiff.single_change.op_kind:" + op0.kind.toString())
stdout("BufferDiff.single_change.op_col:" + op0.col.toString())
stdout("BufferDiff.single_change.op_row:" + op0.row.toString())
stdout("BufferDiff.single_change.op_text:" + op0.text)

// BufferDiff on different sizes → requires_full=true
buf6 <= BufferNew(5, 3)
diff2 <= BufferDiff(buf0, buf6)
stdout("BufferDiff.size_change.requires_full:" + diff2.requires_full.toString())

// RenderOps emits ANSI for the single-change op list
rendered <= RenderOps(diff1.ops)
stdout("RenderOps.len_gt0:" + If[rendered.length() > 0, "yes", "no"]())

// RenderFrame fallbacks to RenderFull on size change
frame_full <= RenderFrame(buf0, buf6)
stdout("RenderFrame.size_change.len_gt10:" + If[frame_full.text.length() > 10, "yes", "no"]())

// RenderFrame emits minimal diff for single-cell change
frame_diff <= RenderFrame(buf0, buf1)
stdout("RenderFrame.diff.len_gt0:" + If[frame_diff.text.length() > 0, "yes", "no"]())

// RenderFrame on identical buffers emits empty text
frame_same <= RenderFrame(buf0, buf0)
stdout("RenderFrame.same.empty:" + If[frame_same.text == "", "yes", "no"]())

// -- BufferBlit (TMB-022 / Phase 9) --
// Identity blit: 3×2 sub into 3×2 main at (1,1) → every cell overwritten.
mainA <= BufferNew(3, 2)
subA  <= BufferWrite(BufferNew(3, 2), 1, 1, "abc", plain)
subA2 <= BufferWrite(subA, 1, 2, "def", plain)
blit0 <= BufferBlit(mainA, subA2, 1, 1)
blit0.cells.get(0) ]=> blit0_0
blit0.cells.get(5) ]=> blit0_5
stdout("BufferBlit.identity.0:" + blit0_0.text)
stdout("BufferBlit.identity.5:" + blit0_5.text)

// Partial blit: 2×1 sub into 5×3 main at (3,2) → sub lands at cells[7], [8].
mainB <= BufferNew(5, 3)
subB  <= BufferWrite(BufferNew(2, 1), 1, 1, "XY", plain)
blit1 <= BufferBlit(mainB, subB, 3, 2)
blit1.cells.get(7) ]=> blit1_7
blit1.cells.get(8) ]=> blit1_8
blit1.cells.get(9) ]=> blit1_9
stdout("BufferBlit.partial.7:" + blit1_7.text)
stdout("BufferBlit.partial.8:" + blit1_8.text)
stdout("BufferBlit.partial.untouched:" + blit1_9.text)

// Out-of-bounds clip: 3×3 sub into 4×4 main at (3,3) → only 2×2 copied.
mainC <= BufferNew(4, 4)
subC  <= BufferFillRect(BufferNew(3, 3), 1, 1, 3, 3, fillCell)
blit2 <= BufferBlit(mainC, subC, 3, 3)
// Last cell of row 3 = index 11 (row-major, 4-wide, row 3 starts at 8).
// Cells at (3,3) / (4,3) / (3,4) / (4,4) → indices 10 / 11 / 14 / 15.
blit2.cells.get(10) ]=> blit2_10
blit2.cells.get(15) ]=> blit2_15
// (5,3) would be outside main (only 4 cols) — assert (4,3) came through.
blit2.cells.get(9) ]=> blit2_9
stdout("BufferBlit.clip.10:" + blit2_10.text)
stdout("BufferBlit.clip.15:" + blit2_15.text)
stdout("BufferBlit.clip.untouched:" + blit2_9.text)

// Style preserve: blit a styled sub into main, check fg carries over.
mainD <= BufferNew(3, 1)
subD  <= BufferWrite(BufferNew(3, 1), 1, 1, "R", red)
blit3 <= BufferBlit(mainD, subD, 1, 1)
blit3.cells.get(0) ]=> blit3_0
stdout("BufferBlit.style_fg:" + blit3_0.fg)

// Start past main right edge → no-op (main returned unchanged).
mainE <= BufferWrite(BufferNew(3, 1), 1, 1, "ORG", plain)
blit4 <= BufferBlit(mainE, subB, 10, 1)
blit4.cells.get(0) ]=> blit4_0
blit4.cells.get(1) ]=> blit4_1
blit4.cells.get(2) ]=> blit4_2
stdout("BufferBlit.past_right.0:" + blit4_0.text)
stdout("BufferBlit.past_right.1:" + blit4_1.text)
stdout("BufferBlit.past_right.2:" + blit4_2.text)

// Wide-char boundary (TMB-022 HOLD fix):
// sub lays a wide-char lead at its rightmost in-bounds column — when
// the placeholder would spill past main's right edge, the lead must
// also be dropped (matches BufferWrite right-edge policy). Before the
// fix the lead wrote at main.cols and the placeholder got clipped,
// leaving a half wide char that broke ScreenBuffer's pairing invariant.
// Layout: mainF is 4 cols wide, sub is 2 cols "漢" (lead + placeholder).
// Place sub at col 4 → lead lands at col 4 (= main.cols), placeholder
// would need col 5 (doesn't exist) → lead drops, main cell 4 stays ".".
mainF  <= BufferFillRect(BufferNew(4, 1), 1, 1, 4, 1, @(text <= ".", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
subF   <= BufferWrite(BufferNew(2, 1), 1, 1, "漢", plain)
blit5  <= BufferBlit(mainF, subF, 4, 1)
blit5.cells.get(3) ]=> blit5_3
stdout("BufferBlit.wide_edge_drop.3:" + blit5_3.text)

// Positive control: same sub placed so both lead AND placeholder fit
// → wide char preserved and main cells 3/4 carry the pair.
mainG  <= BufferFillRect(BufferNew(4, 1), 1, 1, 4, 1, @(text <= ".", fg <= "", bg <= "", bold <= false, dim <= false, underline <= false, italic <= false))
blit6  <= BufferBlit(mainG, subF, 3, 1)
blit6.cells.get(2) ]=> blit6_2
blit6.cells.get(3) ]=> blit6_3
stdout("BufferBlit.wide_inbounds.2:" + blit6_2.text)
stdout("BufferBlit.wide_inbounds.3:" + blit6_3.text)

stdout("PASS:renderer_ops")
stdout("PASS:all_smoke_tests")
