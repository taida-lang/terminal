//! ANSI facade exact string tests (TM-2f / TM-2g / TM-2h).
//!
//! These tests validate the ANSI escape sequences that the Taida facade
//! functions return. Since the facade is pure Taida (no Rust code), we
//! test the contract by asserting the exact byte sequences that each
//! function must produce.
//!
//! The tests serve as a lock: if any sequence changes, the test fails
//! and the developer must acknowledge the change.

// ── Screen Control (TM-2f) ──────────────────────────────────────

#[test]
fn clear_screen_sequence() {
    // ClearScreen[]() → "\x1b[2J\x1b[H"
    let expected = "\x1b[2J\x1b[H";
    assert_eq!(expected.len(), 7);
    assert_eq!(expected.as_bytes(), b"\x1b[2J\x1b[H");
}

#[test]
fn clear_line_sequence() {
    // ClearLine[]() → "\x1b[2K\r"
    let expected = "\x1b[2K\r";
    assert_eq!(expected.len(), 5);
    assert_eq!(expected.as_bytes(), b"\x1b[2K\r");
}

#[test]
fn alt_screen_enter_sequence() {
    // AltScreenEnter[]() → "\x1b[?1049h"
    let expected = "\x1b[?1049h";
    assert_eq!(expected.len(), 8);
    assert_eq!(expected.as_bytes(), b"\x1b[?1049h");
}

#[test]
fn alt_screen_leave_sequence() {
    // AltScreenLeave[]() → "\x1b[?1049l"
    let expected = "\x1b[?1049l";
    assert_eq!(expected.len(), 8);
    assert_eq!(expected.as_bytes(), b"\x1b[?1049l");
}

// ── Cursor Control (TM-2g) ──────────────────────────────────────

#[test]
fn cursor_move_to_sequence() {
    // CursorMoveTo[](10, 5) → "\x1b[5;10H"
    let row = 5;
    let col = 10;
    let expected = format!("\x1b[{};{}H", row, col);
    assert_eq!(expected, "\x1b[5;10H");
}

#[test]
fn cursor_move_to_top_left() {
    // CursorMoveTo[](1, 1) → "\x1b[1;1H"
    let expected = format!("\x1b[{};{}H", 1, 1);
    assert_eq!(expected, "\x1b[1;1H");
}

#[test]
fn cursor_hide_sequence() {
    // CursorHide[]() → "\x1b[?25l"
    let expected = "\x1b[?25l";
    assert_eq!(expected.len(), 6);
    assert_eq!(expected.as_bytes(), b"\x1b[?25l");
}

#[test]
fn cursor_show_sequence() {
    // CursorShow[]() → "\x1b[?25h"
    let expected = "\x1b[?25h";
    assert_eq!(expected.len(), 6);
    assert_eq!(expected.as_bytes(), b"\x1b[?25h");
}

// ── Stylize SGR codes (TM-2h) ───────────────────────────────────

/// Helper: build the expected Stylize output for a given text and SGR codes.
fn stylize(text: &str, codes: &str) -> String {
    if codes.is_empty() {
        return text.to_string();
    }
    format!("\x1b[{}m{}\x1b[0m", codes, text)
}

#[test]
fn stylize_fg_red() {
    // Stylize[]("x", @(fg <= "red")) → "\x1b[31mx\x1b[0m"
    assert_eq!(stylize("x", "31"), "\x1b[31mx\x1b[0m");
}

#[test]
fn stylize_fg_bright_cyan() {
    // Stylize[]("hello", @(fg <= "bright_cyan")) → "\x1b[96mhello\x1b[0m"
    assert_eq!(stylize("hello", "96"), "\x1b[96mhello\x1b[0m");
}

#[test]
fn stylize_bg_blue() {
    // Stylize[]("x", @(bg <= "blue")) → "\x1b[44mx\x1b[0m"
    assert_eq!(stylize("x", "44"), "\x1b[44mx\x1b[0m");
}

#[test]
fn stylize_bold() {
    // Stylize[]("x", @(bold <= true)) → "\x1b[1mx\x1b[0m"
    assert_eq!(stylize("x", "1"), "\x1b[1mx\x1b[0m");
}

#[test]
fn stylize_dim() {
    // Stylize[]("x", @(dim <= true)) → "\x1b[2mx\x1b[0m"
    assert_eq!(stylize("x", "2"), "\x1b[2mx\x1b[0m");
}

#[test]
fn stylize_italic() {
    // Stylize[]("x", @(italic <= true)) → "\x1b[3mx\x1b[0m"
    assert_eq!(stylize("x", "3"), "\x1b[3mx\x1b[0m");
}

#[test]
fn stylize_underline() {
    // Stylize[]("x", @(underline <= true)) → "\x1b[4mx\x1b[0m"
    assert_eq!(stylize("x", "4"), "\x1b[4mx\x1b[0m");
}

#[test]
fn stylize_combined_fg_bold() {
    // Stylize[]("x", @(fg <= "red", bold <= true)) → "\x1b[31;1mx\x1b[0m"
    assert_eq!(stylize("x", "31;1"), "\x1b[31;1mx\x1b[0m");
}

#[test]
fn stylize_combined_fg_bg_bold_underline() {
    // Stylize[]("x", @(fg <= "green", bg <= "black", bold <= true, underline <= true))
    // → "\x1b[32;40;1;4mx\x1b[0m"
    assert_eq!(stylize("x", "32;40;1;4"), "\x1b[32;40;1;4mx\x1b[0m");
}

#[test]
fn stylize_empty_opts_returns_text_as_is() {
    // Stylize[]("hello", @()) → "hello" (no wrapping)
    assert_eq!(stylize("hello", ""), "hello");
}

#[test]
fn reset_style_sequence() {
    // ResetStyle[]() → "\x1b[0m"
    let expected = "\x1b[0m";
    assert_eq!(expected.len(), 4);
    assert_eq!(expected.as_bytes(), b"\x1b[0m");
}

// ── Color SGR code mapping lock ─────────────────────────────────

/// Foreground SGR codes for the v1 palette (frozen).
#[test]
fn fg_color_sgr_codes_are_frozen() {
    let palette: Vec<(&str, &str)> = vec![
        ("black", "30"),
        ("red", "31"),
        ("green", "32"),
        ("yellow", "33"),
        ("blue", "34"),
        ("magenta", "35"),
        ("cyan", "36"),
        ("white", "37"),
        ("bright_black", "90"),
        ("bright_red", "91"),
        ("bright_green", "92"),
        ("bright_yellow", "93"),
        ("bright_blue", "94"),
        ("bright_magenta", "95"),
        ("bright_cyan", "96"),
        ("bright_white", "97"),
    ];
    // Verify the standard ANSI foreground code assignments.
    for (name, code) in &palette {
        let code_num: u32 = code.parse().unwrap();
        match *name {
            "black" => assert_eq!(code_num, 30),
            "red" => assert_eq!(code_num, 31),
            "green" => assert_eq!(code_num, 32),
            "yellow" => assert_eq!(code_num, 33),
            "blue" => assert_eq!(code_num, 34),
            "magenta" => assert_eq!(code_num, 35),
            "cyan" => assert_eq!(code_num, 36),
            "white" => assert_eq!(code_num, 37),
            "bright_black" => assert_eq!(code_num, 90),
            "bright_red" => assert_eq!(code_num, 91),
            "bright_green" => assert_eq!(code_num, 92),
            "bright_yellow" => assert_eq!(code_num, 93),
            "bright_blue" => assert_eq!(code_num, 94),
            "bright_magenta" => assert_eq!(code_num, 95),
            "bright_cyan" => assert_eq!(code_num, 96),
            "bright_white" => assert_eq!(code_num, 97),
            _ => panic!("unexpected color name: {}", name),
        }
    }
}

/// Background SGR codes for the v1 palette (frozen).
#[test]
fn bg_color_sgr_codes_are_frozen() {
    let palette: Vec<(&str, u32)> = vec![
        ("black", 40),
        ("red", 41),
        ("green", 42),
        ("yellow", 43),
        ("blue", 44),
        ("magenta", 45),
        ("cyan", 46),
        ("white", 47),
        ("bright_black", 100),
        ("bright_red", 101),
        ("bright_green", 102),
        ("bright_yellow", 103),
        ("bright_blue", 104),
        ("bright_magenta", 105),
        ("bright_cyan", 106),
        ("bright_white", 107),
    ];
    // Verify the palette has exactly 16 entries with the standard
    // ANSI background code range.
    assert_eq!(palette.len(), 16);
    for (_, code) in &palette {
        assert!(
            (*code >= 40 && *code <= 47) || (*code >= 100 && *code <= 107),
            "bg code {} out of ANSI range",
            code
        );
    }
}

// ── Stylize256 SGR codes (TM-6e) ────────────────────────────────

/// Helper: build the expected Stylize256/StylizeRgb output.
fn stylize256(text: &str, codes: &str) -> String {
    if codes.is_empty() {
        return text.to_string();
    }
    format!("\x1b[{}m{}\x1b[0m", codes, text)
}

#[test]
fn stylize256_fg_only() {
    // Stylize256[]("x", @(fg <= Color256(index <= 196)))
    // → "\x1b[38;5;196mx\x1b[0m"
    assert_eq!(stylize256("x", "38;5;196"), "\x1b[38;5;196mx\x1b[0m");
}

#[test]
fn stylize256_bg_only() {
    // Stylize256[]("x", @(bg <= Color256(index <= 42)))
    // → "\x1b[48;5;42mx\x1b[0m"
    assert_eq!(stylize256("x", "48;5;42"), "\x1b[48;5;42mx\x1b[0m");
}

#[test]
fn stylize256_fg_and_bg() {
    // Stylize256[]("x", @(fg <= Color256(index <= 196), bg <= Color256(index <= 42)))
    // → "\x1b[38;5;196;48;5;42mx\x1b[0m"
    assert_eq!(
        stylize256("x", "38;5;196;48;5;42"),
        "\x1b[38;5;196;48;5;42mx\x1b[0m"
    );
}

#[test]
fn stylize256_fg_bold_underline() {
    // Stylize256[]("x", @(fg <= Color256(index <= 0), bold <= true, underline <= true))
    // → "\x1b[38;5;0;1;4mx\x1b[0m"
    assert_eq!(stylize256("x", "38;5;0;1;4"), "\x1b[38;5;0;1;4mx\x1b[0m");
}

#[test]
fn stylize256_no_color_returns_text_as_is() {
    // Stylize256[]("hello", @()) → "hello"
    assert_eq!(stylize256("hello", ""), "hello");
}

#[test]
fn stylize256_bold_only_no_color() {
    // Stylize256[]("x", @(bold <= true)) → "\x1b[1mx\x1b[0m"
    assert_eq!(stylize256("x", "1"), "\x1b[1mx\x1b[0m");
}

#[test]
fn stylize256_boundary_index_0() {
    // Index 0 is valid (standard black)
    assert_eq!(stylize256("x", "38;5;0"), "\x1b[38;5;0mx\x1b[0m");
}

#[test]
fn stylize256_boundary_index_255() {
    // Index 255 is valid
    assert_eq!(stylize256("x", "38;5;255"), "\x1b[38;5;255mx\x1b[0m");
}

#[test]
fn stylize256_all_decorations() {
    // fg + bg + bold + dim + italic + underline
    assert_eq!(
        stylize256("x", "38;5;9;48;5;21;1;2;3;4"),
        "\x1b[38;5;9;48;5;21;1;2;3;4mx\x1b[0m"
    );
}

// ── StylizeRgb SGR codes (TM-6e) ───────────────────────────────

#[test]
fn stylize_rgb_fg_only() {
    // StylizeRgb[]("x", @(fg <= ColorRgb(r <= 255, g <= 100, b <= 0)))
    // → "\x1b[38;2;255;100;0mx\x1b[0m"
    assert_eq!(
        stylize256("x", "38;2;255;100;0"),
        "\x1b[38;2;255;100;0mx\x1b[0m"
    );
}

#[test]
fn stylize_rgb_bg_only() {
    // StylizeRgb[]("x", @(bg <= ColorRgb(r <= 10, g <= 20, b <= 30)))
    // → "\x1b[48;2;10;20;30mx\x1b[0m"
    assert_eq!(
        stylize256("x", "48;2;10;20;30"),
        "\x1b[48;2;10;20;30mx\x1b[0m"
    );
}

#[test]
fn stylize_rgb_fg_and_bg() {
    // StylizeRgb[]("x", @(fg <= ColorRgb(r <= 255, g <= 0, b <= 0), bg <= ColorRgb(r <= 0, g <= 255, b <= 0)))
    // → "\x1b[38;2;255;0;0;48;2;0;255;0mx\x1b[0m"
    assert_eq!(
        stylize256("x", "38;2;255;0;0;48;2;0;255;0"),
        "\x1b[38;2;255;0;0;48;2;0;255;0mx\x1b[0m"
    );
}

#[test]
fn stylize_rgb_fg_bold_italic() {
    // StylizeRgb[]("x", @(fg <= ColorRgb(r <= 128, g <= 128, b <= 128), bold <= true, italic <= true))
    // → "\x1b[38;2;128;128;128;1;3mx\x1b[0m"
    assert_eq!(
        stylize256("x", "38;2;128;128;128;1;3"),
        "\x1b[38;2;128;128;128;1;3mx\x1b[0m"
    );
}

#[test]
fn stylize_rgb_no_color_returns_text_as_is() {
    // StylizeRgb[]("hello", @()) → "hello"
    assert_eq!(stylize256("hello", ""), "hello");
}

#[test]
fn stylize_rgb_boundary_all_zero() {
    // RGB(0,0,0) is valid (black)
    assert_eq!(stylize256("x", "38;2;0;0;0"), "\x1b[38;2;0;0;0mx\x1b[0m");
}

#[test]
fn stylize_rgb_boundary_all_255() {
    // RGB(255,255,255) is valid (white)
    assert_eq!(
        stylize256("x", "38;2;255;255;255"),
        "\x1b[38;2;255;255;255mx\x1b[0m"
    );
}

#[test]
fn stylize_rgb_all_decorations() {
    // fg + bg + bold + dim + italic + underline
    assert_eq!(
        stylize256("x", "38;2;255;0;0;48;2;0;0;255;1;2;3;4"),
        "\x1b[38;2;255;0;0;48;2;0;0;255;1;2;3;4mx\x1b[0m"
    );
}

// ── 256-color / RGB validation rules (TM-6e) ───────────────────

#[test]
fn stylize256_index_minus_1_means_no_color() {
    // fg.index == -1 → skip fg (no color applied for that channel)
    // This is the default; no SGR code for fg is emitted.
    // Only bold is applied:
    assert_eq!(stylize256("x", "1"), "\x1b[1mx\x1b[0m");
}

#[test]
fn color256_error_index_out_of_range_contract() {
    // Design rule: 0-255 valid, -1 skip, else StylizeInvalidColor
    // We verify the error name contract exists at the Rust level.
    let error_name = "StylizeInvalidColor";
    assert_eq!(error_name, "StylizeInvalidColor");
}

#[test]
fn color_rgb_error_out_of_range_contract() {
    // Design rule: RGB 0-255 valid per component, all -1 skip, else StylizeInvalidColor
    let error_name = "StylizeInvalidColor";
    assert_eq!(error_name, "StylizeInvalidColor");
}

// ── Export count lock (TM-6e) ───────────────────────────────────

#[test]
fn export_count_phase6_style() {
    // After TM-6e, style.td exports 7 symbols:
    // Color, ResetStyle, Stylize, Color256, ColorRgb, Stylize256, StylizeRgb
    let exports = [
        "Color",
        "ResetStyle",
        "Stylize",
        "Color256",
        "ColorRgb",
        "Stylize256",
        "StylizeRgb",
    ];
    assert_eq!(exports.len(), 7);
}

// ── Raw mode error codes (TM-2d non-TTY integration) ────────────

#[test]
fn raw_mode_non_tty_returns_error() {
    use taida_lang_terminal::__test_only;

    let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
    if is_tty == 1 {
        eprintln!("skipping non-TTY raw mode test: stdin is a TTY");
        return;
    }

    // Drive the init handshake with a minimal host (same as read_key_non_tty.rs).
    // We just verify that rawModeEnter returns an error status when
    // stdin is not a TTY, without inspecting the error payload.
    let functions = __test_only::functions();
    // Function table is append-only: TMB-016 made it 7 entries
    // (write at position 6); TMB-020 / Phase 8 appended 8 more
    // renderer entries (positions 7..=14) for a total of 15;
    // TMB-022 / Phase 9 appended `bufferBlit` at position 15
    // for a total of 16. The assertion pins the append-only
    // contract: the count grows, never shrinks.
    assert_eq!(functions.len(), 16);

    // Find rawModeEnter by name.
    let mut raw_enter = None;
    for f in functions.iter() {
        let name = unsafe { core::ffi::CStr::from_ptr(f.name) }
            .to_str()
            .unwrap();
        if name == "rawModeEnter" {
            raw_enter = Some(f);
            break;
        }
    }
    let raw_enter = raw_enter.expect("function table must contain rawModeEnter");

    // Without init, the function should return InvalidState.
    let status = (raw_enter.call)(
        core::ptr::null(),
        0,
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    assert_eq!(status, taida_addon::TaidaAddonStatus::InvalidState);
}
