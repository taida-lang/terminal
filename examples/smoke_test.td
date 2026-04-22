// smoke_test.td -- Facade smoke test for taida-lang/terminal
//
// This file is loaded by the Taida interpreter to verify that all
// facade .td files parse correctly, exports are resolvable, and
// basic function calls produce the expected return values.
//
// Run: taida examples/smoke_test.td
//   (requires the package to be installed or symlinked into .taida/deps/)

>>> taida-lang/terminal => @(KeyKind, Color, ResetStyle, ClearScreen, ClearLine, AltScreenEnter, AltScreenLeave, CursorMoveTo, CursorHide, CursorShow, MouseTrackingEnter, MouseTrackingLeave, Stylize, Color256, ColorRgb, Stylize256, StylizeRgb, WidthMode, MeasureGrapheme, DisplayWidth, NormalizeCellText, TruncateWidth, PadWidth, Cell, CellStyle, ScreenBuffer, DiffOpKind, DiffOp, BufferNew, BufferPut, BufferWrite, BufferFillRect, BufferClear, BufferResize, RenderFull, BufferDiff, RenderOps, RenderFrame, PromptMode, PromptOptions, CompletionState, LineEditorAction, LineEditorState, LineEditorNew, LineEditorStep, LineEditorRender, SpinnerState, SpinnerNext, SpinnerRender, ProgressOptions, ProgressBar, StatusLine, EventKind, MouseKind)

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

stdout("PASS:renderer_ops")
stdout("PASS:all_smoke_tests")
