//! Unicode display width entries (Phase 10 / TMB-025).
//!
//! This module owns the addon entries for `MeasureGrapheme`,
//! `DisplayWidth`, `NormalizeCellText`, `TruncateWidth`, and
//! `PadWidth`. All five used to live in `taida/width.td` as pure
//! Taida, but the `CharAt[src, idx]()` primitive is `O(idx)` in the
//! interpreter (`s.chars().nth(idx)`) and the `acc + ch` string
//! concatenation allocates a fresh `String` each iteration, making
//! each of the five `O(N²)` in input length. `tui_screen.td` calls
//! them 22× per frame for N ∈ [80, 200] which consumed hundreds of
//! milliseconds per frame on Hachikuma's real-terminal render path.
//!
//! The implementation here walks `text.chars()` exactly once,
//! classifying each scalar value with the **same width tables**
//! already used by `renderer::ops::char_width` so the two code paths
//! agree byte-for-byte with the @a.6 facade. Strings are built with
//! `String::with_capacity(...)` so the hot path performs one
//! allocation per call, not one per codepoint.
//!
//! ## Contract (frozen — must match @a.6 `width.td` output byte-for-byte)
//!
//! ### `measureGrapheme[](text) → @(width: Int, mode: Int)`
//!
//! - `""` → `@(width <= 0, mode <= WidthMode.Zero = 2)`
//! - Otherwise classify the **first** Unicode scalar value of `text`:
//!   - control / combining → `@(0, Zero = 2)`
//!   - East Asian Wide / Fullwidth → `@(2, Wide = 1)`
//!   - else → `@(1, Narrow = 0)`
//!
//! The Taida-side `WidthMode` enum tag values are part of the public
//! surface. Keep the integer constants in sync with `taida/width.td`:
//! `Narrow = 0`, `Wide = 1`, `Zero = 2`, `Ambiguous = 3`.
//!
//! ### `displayWidth[](text) → Int`
//!
//! - `""` → `0`
//! - Sum of `char_width(cp)` over every scalar value.
//!
//! ### `normalizeCellText[](text) → Str`
//!
//! - `""` → `" "` (single space)
//! - For each scalar value: `\t` → four spaces, `\n` and `\r` dropped,
//!   everything else appended verbatim.
//! - If the cleaned result is empty → return `" "`.
//!
//! ### `truncateWidth[](text, width) → Str`
//!
//! - `width < 1` → `""`
//! - `text == ""` → `""`
//! - Otherwise append scalar values until the next one's display width
//!   would exceed the remaining budget, at which point stop. Zero-width
//!   marks (combining / control) are included as long as the budget is
//!   still non-zero.
//!
//! ### `padWidth[](text, width) → Str`
//!
//! - If `DisplayWidth(text) >= width` → `text` unchanged
//! - Otherwise append `(width - DisplayWidth(text))` spaces
//!
//! ## Error model
//!
//! All five entries are pure functions. They never allocate a host
//! error except on arity / type / panic. The error band is reused from
//! the renderer (`6xxx`) with a single addition:
//!
//! - `WIDTH_INVALID_ARG = 6101` → Taida-side name `WidthInvalidArg`
//! - `WIDTH_BUILD_VALUE = 6102` → `WidthBuildValue`
//! - `WIDTH_PANIC       = 6103` → `WidthPanic`
//!
//! These are **distinct** from the renderer band so callers that
//! `|== "WidthInvalidArg"` can tell width-layer failures from renderer
//! failures. The band `61xx` has no collisions in the frozen error
//! code table (`tests::error_code_ranges_are_frozen_unix`).

use core::ffi::c_char;

use taida_addon::bridge::{BorrowedValue, HostValueBuilder, borrow_arg};
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

// ── Error codes (6101..=6103) ─────────────────────────────────────

pub mod err {
    /// Argument was not a string or integer as required.
    pub const WIDTH_INVALID_ARG: u32 = 6101;
    /// Host failed to allocate a return value.
    pub const WIDTH_BUILD_VALUE: u32 = 6102;
    /// A panic escaped the addon body.
    pub const WIDTH_PANIC: u32 = 6103;
}

// ── WidthMode tag constants (mirror `taida/width.td::WidthMode`) ──

/// Narrow — width 1, non-combining.
const MODE_NARROW: i64 = 0;
/// Wide — East Asian Wide / Fullwidth (width 2).
const MODE_WIDE: i64 = 1;
/// Zero — combining / control (width 0).
const MODE_ZERO: i64 = 2;

// ── Width tables (mirror `renderer::ops::char_width`) ─────────────
//
// Duplicated on purpose. The renderer needs this table for the
// `BufferWrite` hot path; the width addon entries need it for the
// public API. Keeping them in sync is the responsibility of the
// `width_table_matches_renderer_ops` regression test below.

fn is_control(cp: u32) -> bool {
    cp < 0x20 || cp == 0x7F || (0x80..=0x9F).contains(&cp)
}

fn is_combining(cp: u32) -> bool {
    matches!(
        cp,
        0x0300..=0x036F
        | 0x1100..=0x117F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20FF
        | 0xFE20..=0xFE2F
        | 0xFE00..=0xFE0F
        | 0x200B
        | 0x200C
        | 0x200D
        | 0xFEFF
        | 0x309A..=0x309B
        | 0xE0100..=0xE01EF
    )
}

fn is_wide(cp: u32) -> bool {
    matches!(
        cp,
        0x2E80..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF01..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1100..=0x115F
        | 0x20000..=0x3FFFD
    )
}

/// Classify one scalar value. Mirrors the renderer `char_width`
/// exactly — changing one without the other violates the cross-module
/// width invariant.
fn char_width(cp: u32) -> u32 {
    if is_control(cp) {
        return 0;
    }
    if is_combining(cp) {
        return 0;
    }
    if is_wide(cp) {
        return 2;
    }
    1
}

/// Return the WidthMode tag for a scalar value. Controls and
/// combining marks both map to `Zero` — this matches the Taida-side
/// `_measureGraphemeInner` pattern.
fn width_mode(cp: u32) -> i64 {
    if is_control(cp) || is_combining(cp) {
        return MODE_ZERO;
    }
    if is_wide(cp) {
        return MODE_WIDE;
    }
    MODE_NARROW
}

// ── Core pure helpers (O(N) over the input) ───────────────────────

/// `DisplayWidth(text)` as a pure Rust function.
pub fn display_width(text: &str) -> i64 {
    let mut total: i64 = 0;
    for c in text.chars() {
        total += char_width(c as u32) as i64;
    }
    total
}

/// `MeasureGrapheme(text)` — returns `(width, mode)` for the *first*
/// scalar value. Empty string → `(0, Zero)`.
pub fn measure_grapheme(text: &str) -> (i64, i64) {
    match text.chars().next() {
        None => (0, MODE_ZERO),
        Some(c) => {
            let cp = c as u32;
            (char_width(cp) as i64, width_mode(cp))
        }
    }
}

/// `NormalizeCellText(text)`:
///   - empty → `" "`
///   - `\t` → 4 spaces
///   - `\n` / `\r` dropped
///   - everything else appended verbatim
///   - if the result ends up empty → `" "` (matches `_normFinish`).
pub fn normalize_cell_text(text: &str) -> String {
    if text.is_empty() {
        return " ".to_string();
    }
    // Worst case: every char is a tab → 4x expansion. Typical case
    // is byte-length of input; reserve the larger of the two without
    // being wasteful on normal text.
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => out.push_str("    "),
            '\n' | '\r' => { /* drop */ }
            _ => out.push(c),
        }
    }
    if out.is_empty() {
        return " ".to_string();
    }
    out
}

/// `TruncateWidth(text, width)`:
///   - `width < 1` → `""`
///   - `text == ""` → `""`
///   - Accumulate scalar values until the next one would exceed the
///     remaining budget.
pub fn truncate_width(text: &str, width: i64) -> String {
    if width < 1 || text.is_empty() {
        return String::new();
    }
    let mut remaining = width;
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let cp = c as u32;
        let w = char_width(cp) as i64;
        // Matches the `_twLoop` contract: stop *before* appending if
        // appending would exceed. Zero-width marks (`w == 0`) are
        // still included because `m.width > remaining` is false.
        if w > remaining {
            break;
        }
        out.push(c);
        remaining -= w;
        if remaining < 1 && w > 0 {
            // Any further positive-width char would exceed; avoid the
            // extra loop iteration cost.
            continue;
        }
    }
    out
}

/// `PadWidth(text, width)`:
///   - If display width of `text` is `>= width` → return `text` as-is
///   - Otherwise pad with spaces on the right
pub fn pad_width(text: &str, width: i64) -> String {
    let current = display_width(text);
    if current >= width {
        return text.to_string();
    }
    let pad = (width - current) as usize;
    let mut out = String::with_capacity(text.len() + pad);
    out.push_str(text);
    for _ in 0..pad {
        out.push(' ');
    }
    out
}

// ── FFI helpers (shared shape with renderer modules) ──────────────

fn arg_at<'a>(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    idx: usize,
) -> Option<BorrowedValue<'a>> {
    unsafe { borrow_arg(args_ptr, args_len, idx) }
}

fn emit_invalid_arg(
    builder: &HostValueBuilder,
    out_error: *mut *mut TaidaAddonErrorV1,
    what: &str,
) -> TaidaAddonStatus {
    let msg = format!("WidthInvalidArg: {what}");
    let e = builder.error(err::WIDTH_INVALID_ARG, &msg);
    if !out_error.is_null() {
        unsafe { *out_error = e };
    }
    TaidaAddonStatus::Error
}

fn emit_build_failure(
    builder: &HostValueBuilder,
    out_error: *mut *mut TaidaAddonErrorV1,
    what: &str,
) -> TaidaAddonStatus {
    let msg = format!("WidthBuildValue: failed to build {what}");
    let e = builder.error(err::WIDTH_BUILD_VALUE, &msg);
    if !out_error.is_null() {
        unsafe { *out_error = e };
    }
    TaidaAddonStatus::Error
}

fn emit_panic(
    builder: &HostValueBuilder,
    out_error: *mut *mut TaidaAddonErrorV1,
    what: &str,
) -> TaidaAddonStatus {
    let msg = format!("WidthPanic: {what} panicked (caught at FFI boundary)");
    let e = builder.error(err::WIDTH_PANIC, &msg);
    if !out_error.is_null() {
        unsafe { *out_error = e };
    }
    TaidaAddonStatus::Error
}

fn need_str_arg(v: &BorrowedValue<'_>, what: &str) -> Result<String, String> {
    v.as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("{what} must be a Str"))
}

fn need_int_arg(v: &BorrowedValue<'_>, what: &str) -> Result<i64, String> {
    v.as_int().ok_or_else(|| format!("{what} must be an Int"))
}

// ── Entries ───────────────────────────────────────────────────────

const FIELD_WIDTH: &core::ffi::CStr = c"width";
const FIELD_MODE: &core::ffi::CStr = c"mode";

/// `measureGrapheme[](text: Str) → @(width: Int, mode: Int)`
pub fn measure_grapheme_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 1 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let a0 = match arg_at(args_ptr, args_len, 0) {
        Some(v) => v,
        None => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let text = need_str_arg(&a0, "text")?;
        Ok::<(i64, i64), String>(measure_grapheme(&text))
    }));

    match result {
        Ok(Ok((w, m))) => {
            let names: [*const c_char; 2] = [FIELD_WIDTH.as_ptr(), FIELD_MODE.as_ptr()];
            let values = [builder.int(w), builder.int(m)];
            let value = builder.pack(&names, &values);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "MeasureGrapheme result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(msg)) => emit_invalid_arg(&builder, out_error, &msg),
        Err(_) => emit_panic(&builder, out_error, "measure_grapheme"),
    }
}

/// `displayWidth[](text: Str) → Int`
pub fn display_width_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 1 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let a0 = match arg_at(args_ptr, args_len, 0) {
        Some(v) => v,
        None => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let text = need_str_arg(&a0, "text")?;
        Ok::<i64, String>(display_width(&text))
    }));

    match result {
        Ok(Ok(w)) => {
            let value = builder.int(w);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "DisplayWidth result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(msg)) => emit_invalid_arg(&builder, out_error, &msg),
        Err(_) => emit_panic(&builder, out_error, "display_width"),
    }
}

/// `normalizeCellText[](text: Str) → Str`
pub fn normalize_cell_text_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 1 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let a0 = match arg_at(args_ptr, args_len, 0) {
        Some(v) => v,
        None => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let text = need_str_arg(&a0, "text")?;
        Ok::<String, String>(normalize_cell_text(&text))
    }));

    match result {
        Ok(Ok(s)) => {
            let value = builder.str(&s);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "NormalizeCellText result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(msg)) => emit_invalid_arg(&builder, out_error, &msg),
        Err(_) => emit_panic(&builder, out_error, "normalize_cell_text"),
    }
}

/// `truncateWidth[](text: Str, width: Int) → Str`
pub fn truncate_width_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 2 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let (a0, a1) = match (arg_at(args_ptr, args_len, 0), arg_at(args_ptr, args_len, 1)) {
        (Some(a), Some(b)) => (a, b),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let text = need_str_arg(&a0, "text")?;
        let width = need_int_arg(&a1, "width")?;
        Ok::<String, String>(truncate_width(&text, width))
    }));

    match result {
        Ok(Ok(s)) => {
            let value = builder.str(&s);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "TruncateWidth result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(msg)) => emit_invalid_arg(&builder, out_error, &msg),
        Err(_) => emit_panic(&builder, out_error, "truncate_width"),
    }
}

/// `padWidth[](text: Str, width: Int) → Str`
pub fn pad_width_impl(
    host_ptr: *const TaidaHostV1,
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 2 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };
    let (a0, a1) = match (arg_at(args_ptr, args_len, 0), arg_at(args_ptr, args_len, 1)) {
        (Some(a), Some(b)) => (a, b),
        _ => return TaidaAddonStatus::NullPointer,
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let text = need_str_arg(&a0, "text")?;
        let width = need_int_arg(&a1, "width")?;
        Ok::<String, String>(pad_width(&text, width))
    }));

    match result {
        Ok(Ok(s)) => {
            let value = builder.str(&s);
            if value.is_null() {
                return emit_build_failure(&builder, out_error, "PadWidth result");
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(msg)) => emit_invalid_arg(&builder, out_error, &msg),
        Err(_) => emit_panic(&builder, out_error, "pad_width"),
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── display_width ──
    #[test]
    fn display_width_empty_is_zero() {
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn display_width_ascii_is_length() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width("a"), 1);
    }

    #[test]
    fn display_width_cjk_is_double() {
        // 漢字 = 2 CJK chars × 2 cells each = 4
        assert_eq!(display_width("漢字"), 4);
    }

    #[test]
    fn display_width_mixed() {
        // A(1) + 漢(2) + B(1) = 4
        assert_eq!(display_width("A漢B"), 4);
    }

    #[test]
    fn display_width_combining_is_zero() {
        // Cafe\u{0301} — combining acute adds 0 width
        assert_eq!(display_width("Cafe\u{0301}"), 4);
    }

    #[test]
    fn display_width_control_is_zero() {
        // \t is a control char (< 0x20) → width 0
        assert_eq!(display_width("\t"), 0);
        // Same for \0 and DEL
        assert_eq!(display_width("\0"), 0);
        assert_eq!(display_width("\x7f"), 0);
    }

    // ── measure_grapheme ──
    #[test]
    fn measure_grapheme_empty() {
        assert_eq!(measure_grapheme(""), (0, MODE_ZERO));
    }

    #[test]
    fn measure_grapheme_ascii_narrow() {
        assert_eq!(measure_grapheme("A"), (1, MODE_NARROW));
    }

    #[test]
    fn measure_grapheme_cjk_wide() {
        assert_eq!(measure_grapheme("漢"), (2, MODE_WIDE));
    }

    #[test]
    fn measure_grapheme_combining_zero() {
        assert_eq!(measure_grapheme("\u{0301}"), (0, MODE_ZERO));
    }

    #[test]
    fn measure_grapheme_only_looks_at_first_scalar() {
        // "Ab" — first scalar is A, narrow
        assert_eq!(measure_grapheme("Ab"), (1, MODE_NARROW));
        // "漢A" — first scalar is 漢, wide
        assert_eq!(measure_grapheme("漢A"), (2, MODE_WIDE));
    }

    // ── normalize_cell_text ──
    #[test]
    fn normalize_empty_becomes_space() {
        assert_eq!(normalize_cell_text(""), " ");
    }

    #[test]
    fn normalize_tab_becomes_four_spaces() {
        assert_eq!(normalize_cell_text("\t"), "    ");
        assert_eq!(normalize_cell_text("a\tb"), "a    b");
    }

    #[test]
    fn normalize_newline_stripped() {
        assert_eq!(normalize_cell_text("a\nb"), "ab");
        assert_eq!(normalize_cell_text("a\rb"), "ab");
        assert_eq!(normalize_cell_text("a\r\nb"), "ab");
    }

    #[test]
    fn normalize_only_newline_becomes_space() {
        // Matches `_normFinish`: if the cleaned result is empty, use
        // a single space.
        assert_eq!(normalize_cell_text("\n"), " ");
        assert_eq!(normalize_cell_text("\n\r"), " ");
    }

    #[test]
    fn normalize_preserves_printable_text() {
        assert_eq!(normalize_cell_text("hello"), "hello");
        assert_eq!(normalize_cell_text("漢字"), "漢字");
    }

    // ── truncate_width ──
    #[test]
    fn truncate_width_less_than_one_returns_empty() {
        assert_eq!(truncate_width("hello", 0), "");
        assert_eq!(truncate_width("hello", -1), "");
    }

    #[test]
    fn truncate_width_empty_input_returns_empty() {
        assert_eq!(truncate_width("", 10), "");
    }

    #[test]
    fn truncate_width_fits_returns_all() {
        assert_eq!(truncate_width("hello", 10), "hello");
        assert_eq!(truncate_width("hello", 5), "hello");
    }

    #[test]
    fn truncate_width_stops_before_overflow() {
        assert_eq!(truncate_width("hello", 3), "hel");
        // A wide char that would overflow is dropped entirely
        assert_eq!(truncate_width("A漢B", 2), "A");
        assert_eq!(truncate_width("A漢B", 3), "A漢");
        assert_eq!(truncate_width("A漢B", 4), "A漢B");
    }

    // ── pad_width ──
    #[test]
    fn pad_width_already_wider_returns_text() {
        assert_eq!(pad_width("hello", 3), "hello");
        assert_eq!(pad_width("hello", 5), "hello");
    }

    #[test]
    fn pad_width_adds_spaces() {
        assert_eq!(pad_width("hi", 5), "hi   ");
        assert_eq!(pad_width("", 3), "   ");
    }

    #[test]
    fn pad_width_respects_cjk_width() {
        // "漢" has display width 2; padding to 5 adds 3 spaces.
        assert_eq!(pad_width("漢", 5), "漢   ");
    }

    // ── Error code invariants ──
    #[test]
    fn width_error_codes_are_frozen() {
        // 61xx band, no collisions with renderer 60xx band or any
        // other addon error code.
        assert_eq!(err::WIDTH_INVALID_ARG, 6101);
        assert_eq!(err::WIDTH_BUILD_VALUE, 6102);
        assert_eq!(err::WIDTH_PANIC, 6103);
    }

    #[test]
    fn width_mode_constants_match_facade() {
        // These must match `taida/width.td::WidthMode`. Renumbering
        // breaks the public surface.
        assert_eq!(MODE_NARROW, 0);
        assert_eq!(MODE_WIDE, 1);
        assert_eq!(MODE_ZERO, 2);
    }

    // ── Arity / InvalidState contract (mirrors renderer tests) ──
    #[test]
    fn display_width_arity_mismatch() {
        let status = display_width_impl(
            core::ptr::null(),
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn display_width_invalid_state_when_host_null() {
        let status = display_width_impl(
            core::ptr::null(),
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn measure_grapheme_arity_mismatch() {
        let status = measure_grapheme_impl(
            core::ptr::null(),
            core::ptr::null(),
            2,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn truncate_width_arity_mismatch() {
        let status = truncate_width_impl(
            core::ptr::null(),
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn pad_width_arity_mismatch() {
        let status = pad_width_impl(
            core::ptr::null(),
            core::ptr::null(),
            3,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn normalize_cell_text_arity_mismatch() {
        let status = normalize_cell_text_impl(
            core::ptr::null(),
            core::ptr::null(),
            2,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    // ── Linear-time regression guard ──
    #[test]
    fn display_width_is_linear_in_length() {
        // Same shape as alloc.rs::buffer_new_scales_linearly: we
        // assert that a 16× input produces well under 50× wall time
        // (quadratic would be 256×).
        use std::time::Instant;
        let small = "A".repeat(100);
        let large = "A".repeat(1600);

        let t_small = Instant::now();
        for _ in 0..200 {
            let _ = display_width(&small);
        }
        let ts = t_small.elapsed();

        let t_large = Instant::now();
        for _ in 0..200 {
            let _ = display_width(&large);
        }
        let tl = t_large.elapsed();

        let ratio = tl.as_nanos() as f64 / ts.as_nanos().max(1) as f64;
        assert!(
            ratio < 50.0,
            "display_width appears super-linear: 1600-char/100-char ratio = {ratio:.1}× \
             (linear target ≈ 16×, quadratic target ≈ 256×). small={ts:?}, large={tl:?}"
        );
    }

    #[test]
    fn pad_width_is_linear_in_target_width() {
        use std::time::Instant;
        let t_small = Instant::now();
        for _ in 0..500 {
            let _ = pad_width("hi", 100);
        }
        let ts = t_small.elapsed();

        let t_large = Instant::now();
        for _ in 0..500 {
            let _ = pad_width("hi", 1600);
        }
        let tl = t_large.elapsed();

        let ratio = tl.as_nanos() as f64 / ts.as_nanos().max(1) as f64;
        assert!(
            ratio < 50.0,
            "pad_width appears super-linear: 1600/100 ratio = {ratio:.1}× \
             (linear target ≈ 16×, quadratic target ≈ 256×). small={ts:?}, large={tl:?}"
        );
    }

    /// The width tables in this module and in `renderer::ops` must
    /// agree on every scalar value. We sample a wide range of
    /// codepoints (one-per-decade up to U+20000) and compare.
    #[test]
    fn width_table_matches_renderer_ops() {
        use crate::renderer::ops::char_width as ops_char_width;
        for cp in (0..=0x20000u32).step_by(17) {
            let ours = char_width(cp);
            let theirs = ops_char_width(cp);
            assert_eq!(
                ours, theirs,
                "width mismatch at U+{cp:04X}: width.rs={ours} vs renderer/ops.rs={theirs}"
            );
        }
    }
}
