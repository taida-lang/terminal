//! `taida-lang/terminal` — Windows platform support (TM-6a through TM-6d).
//!
//! This module provides Windows-specific implementations for all addon
//! functions, absorbing the Windows Console API / VT mode differences
//! behind the same public API surface as the Unix implementation.
//!
//! ## Design
//!
//! 1. Public API is identical to Unix — same function signatures,
//!    same return shapes, same error contracts.
//! 2. Windows-specific branching is contained entirely within this
//!    module via `#[cfg(windows)]` on `mod windows` in `lib.rs`.
//! 3. ANSI facade helpers (style, screen, cursor) are shared across
//!    platforms — they work once VT mode is enabled.
//! 4. VT mode enable failure is NOT silent — returns deterministic
//!    `*Unsupported` / `*InvalidState` errors.
//!
//! ## VT Mode Initialization (TM-6a)
//!
//! On first terminal call, we attempt to enable VT processing on the
//! stdout console handle via `SetConsoleMode`. This is a lazy, once-only
//! operation stored in a global `OnceLock`.

use core::ffi::c_char;
use core::panic::AssertUnwindSafe;
use std::panic;
use std::sync::{Mutex, OnceLock};

use taida_addon::bridge::{HostValueBuilder, borrow_arg};
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_MOUSE_INPUT,
    ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WINDOW_INPUT, GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, INPUT_RECORD,
    KEY_EVENT, MOUSE_EVENT, ReadConsoleInputW, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE, SetConsoleMode, WINDOW_BUFFER_SIZE_EVENT,
};

// ── VT mode initialization (TM-6a) ─────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VtModeStatus {
    Enabled,
    Failed,
}

static VT_MODE_INIT: OnceLock<VtModeStatus> = OnceLock::new();

fn ensure_vt_mode() -> VtModeStatus {
    *VT_MODE_INIT.get_or_init(|| {
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        if handle == INVALID_HANDLE_VALUE || handle == 0 as HANDLE {
            return VtModeStatus::Failed;
        }
        let mut mode: u32 = 0;
        if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
            return VtModeStatus::Failed;
        }
        if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING == 0 {
            let new_mode = mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
            if unsafe { SetConsoleMode(handle, new_mode) } == 0 {
                return VtModeStatus::Failed;
            }
        }
        VtModeStatus::Enabled
    })
}

// ── Error codes (matching Unix modules) ─────────────────────────

mod tty_err {
    pub const IS_TERMINAL_INVALID_STREAM: u32 = 2101;
    pub const IS_TERMINAL_BUILD_VALUE: u32 = 2102;
}

mod size_err {
    pub const TERMINAL_SIZE_NOT_A_TTY: u32 = 2001;
    pub const TERMINAL_SIZE_IOCTL: u32 = 2002;
    pub const TERMINAL_SIZE_UNSUPPORTED: u32 = 2003;
}

mod raw_err {
    pub const RAW_MODE_NOT_A_TTY: u32 = 3001;
    pub const RAW_MODE_ALREADY_ACTIVE: u32 = 3002;
    pub const RAW_MODE_NOT_ACTIVE: u32 = 3003;
    pub const RAW_MODE_ENTER_FAILED: u32 = 3004;
    pub const RAW_MODE_LEAVE_FAILED: u32 = 3005;
}

mod key_err {
    pub const READ_KEY_NOT_A_TTY: u32 = 1001;
    pub const READ_KEY_RAW_MODE: u32 = 1002;
    pub const READ_KEY_EOF: u32 = 1003;
    #[allow(dead_code)]
    pub const READ_KEY_INTERRUPTED: u32 = 1004;
    // Historical quirk: 1005 is `ReadKeyRead` here but `ReadKeyPanic`
    // on unix. Both predate the cross-platform panic barrier; the
    // numbers stay frozen because they are public surface. New code
    // discriminates on the error *name*, which is consistent.
    pub const READ_KEY_READ: u32 = 1005;
    pub const READ_KEY_UNSUPPORTED: u32 = 1007;
    /// `ReadKeyPanic` (unix uses 1006 for InvalidState and 1005 for
    /// Panic; both are taken here, so the next free slot is used).
    pub const READ_KEY_PANIC: u32 = 1008;
}

mod event_err {
    pub const READ_EVENT_NOT_IN_RAW_MODE: u32 = 4001;
    pub const READ_EVENT_NOT_A_TTY: u32 = 4002;
    pub const READ_EVENT_READ_FAILED: u32 = 4003;
    pub const READ_EVENT_EOF: u32 = 4004;
    /// `ReadEventPanic` — same name and code as unix.
    pub const READ_EVENT_PANIC: u32 = 4006;
}

// ── Stream helpers ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Stdin,
    Stdout,
    Stderr,
}

fn parse_stream_kind(name: &str) -> Option<StreamKind> {
    match name {
        "stdin" => Some(StreamKind::Stdin),
        "stdout" => Some(StreamKind::Stdout),
        "stderr" => Some(StreamKind::Stderr),
        _ => None,
    }
}

fn std_handle_id(stream: StreamKind) -> u32 {
    match stream {
        StreamKind::Stdin => STD_INPUT_HANDLE,
        StreamKind::Stdout => STD_OUTPUT_HANDLE,
        StreamKind::Stderr => STD_ERROR_HANDLE,
    }
}

fn get_console_handle(stream: StreamKind) -> Option<HANDLE> {
    let handle = unsafe { GetStdHandle(std_handle_id(stream)) };
    if handle == INVALID_HANDLE_VALUE || handle == 0 as HANDLE {
        return None;
    }
    let mut mode: u32 = 0;
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        None
    } else {
        Some(handle)
    }
}

// ── IsTerminal (TM-6b) ─────────────────────────────────────────

pub fn is_terminal_impl(
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

    let arg = match unsafe { borrow_arg(args_ptr, args_len, 0) } {
        Some(v) => v,
        None => return TaidaAddonStatus::NullPointer,
    };
    let Some(stream_name) = arg.as_str() else {
        return TaidaAddonStatus::UnsupportedValue;
    };
    let Some(stream) = parse_stream_kind(stream_name) else {
        let err = builder.error(
            tty_err::IS_TERMINAL_INVALID_STREAM,
            &format!(
                "IsTerminalInvalidStream: expected stdin|stdout|stderr, got {}",
                stream_name
            ),
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    };

    let is_tty = get_console_handle(stream).is_some();
    let value = builder.bool(is_tty);
    if value.is_null() {
        let err = builder.error(
            tty_err::IS_TERMINAL_BUILD_VALUE,
            "IsTerminalBuildValue: failed to allocate Bool return value",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }
    if !out_value.is_null() {
        unsafe { *out_value = value };
    }
    TaidaAddonStatus::Ok
}

// ── TerminalSize (TM-6b) ───────────────────────────────────────

pub fn terminal_size_impl(
    host_ptr: *mut TaidaHostV1,
    _args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr as *const _) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    if ensure_vt_mode() == VtModeStatus::Failed {
        let err = builder.error(
            size_err::TERMINAL_SIZE_UNSUPPORTED,
            "TerminalSizeUnsupported: VT mode initialization failed",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    let handle = match get_console_handle(StreamKind::Stdout) {
        Some(h) => h,
        None => {
            let err = builder.error(
                size_err::TERMINAL_SIZE_NOT_A_TTY,
                "TerminalSizeNotATty: stdout is not a terminal",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
    };

    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { core::mem::zeroed() };
    if unsafe { GetConsoleScreenBufferInfo(handle, &mut info) } == 0 {
        let err = builder.error(
            size_err::TERMINAL_SIZE_IOCTL,
            "TerminalSizeIoctl: GetConsoleScreenBufferInfo failed",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    let cols = (info.srWindow.Right - info.srWindow.Left + 1) as i64;
    let rows = (info.srWindow.Bottom - info.srWindow.Top + 1) as i64;

    if cols < 1 || rows < 1 {
        let err = builder.error(
            size_err::TERMINAL_SIZE_IOCTL,
            "TerminalSizeIoctl: console reported zero dimensions",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    let cols_v = builder.int(cols);
    let rows_v = builder.int(rows);
    let names: [*const c_char; 2] = [c"cols".as_ptr(), c"rows".as_ptr()];
    let values: [*mut TaidaAddonValueV1; 2] = [cols_v, rows_v];
    let value = builder.pack(&names, &values);

    if value.is_null() {
        let err = builder.error(
            size_err::TERMINAL_SIZE_IOCTL,
            "TerminalSizeIoctl: failed to build return pack",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }
    if !out_value.is_null() {
        unsafe { *out_value = value };
    }
    TaidaAddonStatus::Ok
}

// ── RawMode (TM-6c) ────────────────────────────────────────────

struct WinRawModeState {
    active: bool,
    saved_mode: u32,
}

static WIN_RAW_MODE_STATE: Mutex<WinRawModeState> = Mutex::new(WinRawModeState {
    active: false,
    saved_mode: 0,
});

fn is_raw_mode_active() -> bool {
    WIN_RAW_MODE_STATE.lock().map(|s| s.active).unwrap_or(false)
}

fn build_raw_mode_state_pack(
    builder: &HostValueBuilder<'_>,
    active: bool,
) -> *mut TaidaAddonValueV1 {
    let active_v = builder.bool(active);
    if active_v.is_null() {
        return core::ptr::null_mut();
    }
    let names = [c"active".as_ptr() as *const c_char];
    let values = [active_v];
    builder.pack(&names, &values)
}

pub fn raw_mode_enter_impl(
    host_ptr: *mut TaidaHostV1,
    _args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr as *const _) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    let handle = match get_console_handle(StreamKind::Stdin) {
        Some(h) => h,
        None => {
            let err = builder.error(
                raw_err::RAW_MODE_NOT_A_TTY,
                "RawModeNotATty: stdin is not a terminal",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
    };

    let mut state = match WIN_RAW_MODE_STATE.lock() {
        Ok(s) => s,
        Err(_) => {
            let err = builder.error(
                raw_err::RAW_MODE_ENTER_FAILED,
                "RawModeEnterFailed: failed to acquire state lock",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
    };

    if state.active {
        let err = builder.error(
            raw_err::RAW_MODE_ALREADY_ACTIVE,
            "RawModeAlreadyActive: raw mode is already active",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    let mut current_mode: u32 = 0;
    if unsafe { GetConsoleMode(handle, &mut current_mode) } == 0 {
        let err = builder.error(
            raw_err::RAW_MODE_ENTER_FAILED,
            "RawModeEnterFailed: GetConsoleMode failed",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    // ENABLE_WINDOW_INPUT / ENABLE_MOUSE_INPUT: without these flags
    // ReadConsoleInputW never receives WINDOW_BUFFER_SIZE_EVENT /
    // MOUSE_EVENT records, leaving readEvent's Resize and Mouse
    // decode branches unreachable.
    let raw_mode = (current_mode
        & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT))
        | ENABLE_VIRTUAL_TERMINAL_INPUT
        | ENABLE_WINDOW_INPUT
        | ENABLE_MOUSE_INPUT;

    if unsafe { SetConsoleMode(handle, raw_mode) } == 0 {
        let err = builder.error(
            raw_err::RAW_MODE_ENTER_FAILED,
            "RawModeEnterFailed: SetConsoleMode failed",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    state.active = true;
    state.saved_mode = current_mode;

    let value = build_raw_mode_state_pack(&builder, true);
    if value.is_null() {
        return emit_error(
            &builder,
            out_error,
            raw_err::RAW_MODE_ENTER_FAILED,
            "RawModeEnterFailed: failed to build return pack",
        );
    }
    if !out_value.is_null() {
        unsafe { *out_value = value };
    }
    TaidaAddonStatus::Ok
}

pub fn raw_mode_leave_impl(
    host_ptr: *mut TaidaHostV1,
    _args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr as *const _) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    let mut state = match WIN_RAW_MODE_STATE.lock() {
        Ok(s) => s,
        Err(_) => {
            let err = builder.error(
                raw_err::RAW_MODE_LEAVE_FAILED,
                "RawModeLeaveFailed: failed to acquire state lock",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
    };

    if !state.active {
        let err = builder.error(
            raw_err::RAW_MODE_NOT_ACTIVE,
            "RawModeNotActive: raw mode is not active",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    let handle = match get_console_handle(StreamKind::Stdin) {
        Some(h) => h,
        None => {
            let err = builder.error(
                raw_err::RAW_MODE_LEAVE_FAILED,
                "RawModeLeaveFailed: stdin handle lost",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
    };

    if unsafe { SetConsoleMode(handle, state.saved_mode) } == 0 {
        let err = builder.error(
            raw_err::RAW_MODE_LEAVE_FAILED,
            "RawModeLeaveFailed: SetConsoleMode restore failed",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    state.active = false;
    state.saved_mode = 0;

    let value = build_raw_mode_state_pack(&builder, false);
    if value.is_null() {
        return emit_error(
            &builder,
            out_error,
            raw_err::RAW_MODE_LEAVE_FAILED,
            "RawModeLeaveFailed: failed to build return pack",
        );
    }
    if !out_value.is_null() {
        unsafe { *out_value = value };
    }
    TaidaAddonStatus::Ok
}

// ── KeyKind mapping ─────────────────────────────────────────────

#[allow(dead_code)]
mod key_kind {
    pub const CHAR: i64 = 0;
    pub const ENTER: i64 = 1;
    pub const ESCAPE: i64 = 2;
    pub const TAB: i64 = 3;
    pub const BACKSPACE: i64 = 4;
    pub const DELETE: i64 = 5;
    pub const ARROW_UP: i64 = 6;
    pub const ARROW_DOWN: i64 = 7;
    pub const ARROW_LEFT: i64 = 8;
    pub const ARROW_RIGHT: i64 = 9;
    pub const HOME: i64 = 10;
    pub const END: i64 = 11;
    pub const PAGE_UP: i64 = 12;
    pub const PAGE_DOWN: i64 = 13;
    pub const INSERT: i64 = 14;
    pub const F1: i64 = 15;
    pub const F2: i64 = 16;
    pub const F3: i64 = 17;
    pub const F4: i64 = 18;
    pub const F5: i64 = 19;
    pub const F6: i64 = 20;
    pub const F7: i64 = 21;
    pub const F8: i64 = 22;
    pub const F9: i64 = 23;
    pub const F10: i64 = 24;
    pub const F11: i64 = 25;
    pub const F12: i64 = 26;
    pub const UNKNOWN: i64 = 27;
}

fn vk_to_key_kind(vk: u16) -> i64 {
    const VK_RETURN: u16 = 0x0D;
    const VK_ESCAPE: u16 = 0x1B;
    const VK_TAB: u16 = 0x09;
    const VK_BACK: u16 = 0x08;
    const VK_DELETE: u16 = 0x2E;
    const VK_UP: u16 = 0x26;
    const VK_DOWN: u16 = 0x28;
    const VK_LEFT: u16 = 0x25;
    const VK_RIGHT: u16 = 0x27;
    const VK_HOME: u16 = 0x24;
    const VK_END: u16 = 0x23;
    const VK_PRIOR: u16 = 0x21;
    const VK_NEXT: u16 = 0x22;
    const VK_INSERT: u16 = 0x2D;
    const VK_F1: u16 = 0x70;
    const VK_F12: u16 = 0x7B;

    match vk {
        VK_RETURN => key_kind::ENTER,
        VK_ESCAPE => key_kind::ESCAPE,
        VK_TAB => key_kind::TAB,
        VK_BACK => key_kind::BACKSPACE,
        VK_DELETE => key_kind::DELETE,
        VK_UP => key_kind::ARROW_UP,
        VK_DOWN => key_kind::ARROW_DOWN,
        VK_LEFT => key_kind::ARROW_LEFT,
        VK_RIGHT => key_kind::ARROW_RIGHT,
        VK_HOME => key_kind::HOME,
        VK_END => key_kind::END,
        VK_PRIOR => key_kind::PAGE_UP,
        VK_NEXT => key_kind::PAGE_DOWN,
        VK_INSERT => key_kind::INSERT,
        v if v >= VK_F1 && v <= VK_F12 => key_kind::F1 + (v - VK_F1) as i64,
        _ => key_kind::UNKNOWN,
    }
}

// ── Control key state flags ─────────────────────────────────────

const RIGHT_ALT_PRESSED: u32 = 0x0001;
const LEFT_ALT_PRESSED: u32 = 0x0002;
const RIGHT_CTRL_PRESSED: u32 = 0x0004;
const LEFT_CTRL_PRESSED: u32 = 0x0008;
const SHIFT_PRESSED: u32 = 0x0010;

fn decode_modifiers(ctrl_state: u32) -> (bool, bool, bool) {
    let ctrl = (ctrl_state & LEFT_CTRL_PRESSED) != 0 || (ctrl_state & RIGHT_CTRL_PRESSED) != 0;
    let alt = (ctrl_state & LEFT_ALT_PRESSED) != 0 || (ctrl_state & RIGHT_ALT_PRESSED) != 0;
    let shift = (ctrl_state & SHIFT_PRESSED) != 0;
    (ctrl, alt, shift)
}

// ── ReadKey (TM-6c) ─────────────────────────────────────────────

/// Restores a saved console mode on drop — on success, error, and
/// panic-unwind paths alike. Mirrors the unix `RawModeGuard` so a
/// panic inside the read loop can no longer leave the console raw.
struct ModeRestoreGuard {
    handle: HANDLE,
    mode: Option<u32>,
}

impl Drop for ModeRestoreGuard {
    fn drop(&mut self) {
        if let Some(mode) = self.mode {
            unsafe { SetConsoleMode(self.handle, mode) };
        }
    }
}

/// Convert one UTF-16 code unit from a KEY_EVENT into text, assembling
/// surrogate pairs that Windows delivers as two consecutive KEY_EVENT
/// records. Returns `None` while holding a high surrogate — the caller
/// should continue to the next record. Orphan halves surface as
/// U+FFFD instead of being dropped silently.
fn utf16_unit_to_text(unit: u16, pending_high: &mut Option<u16>) -> Option<String> {
    if (0xD800..=0xDBFF).contains(&unit) {
        *pending_high = Some(unit);
        return None;
    }
    let text = if (0xDC00..=0xDFFF).contains(&unit) {
        match pending_high.take() {
            Some(high) => String::from_utf16_lossy(&[high, unit]),
            None => String::from_utf16_lossy(&[unit]),
        }
    } else {
        *pending_high = None;
        String::from_utf16_lossy(&[unit])
    };
    Some(text)
}

pub fn read_key_impl(
    host_ptr: *mut TaidaHostV1,
    _args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr as *const _) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    if ensure_vt_mode() == VtModeStatus::Failed {
        let err = builder.error(
            key_err::READ_KEY_UNSUPPORTED,
            "ReadKeyUnsupported: VT mode initialization failed",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    let handle = match get_console_handle(StreamKind::Stdin) {
        Some(h) => h,
        None => {
            let err = builder.error(
                key_err::READ_KEY_NOT_A_TTY,
                "ReadKeyNotATty: stdin is not a terminal",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
    };

    let standalone_raw = is_raw_mode_active();
    let saved_mode: Option<u32> = if !standalone_raw {
        let mut current_mode: u32 = 0;
        if unsafe { GetConsoleMode(handle, &mut current_mode) } == 0 {
            let err = builder.error(
                key_err::READ_KEY_RAW_MODE,
                "ReadKeyRawMode: GetConsoleMode failed",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
        let raw_mode = (current_mode
            & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT))
            | ENABLE_VIRTUAL_TERMINAL_INPUT;
        if unsafe { SetConsoleMode(handle, raw_mode) } == 0 {
            let err = builder.error(
                key_err::READ_KEY_RAW_MODE,
                "ReadKeyRawMode: failed to enter raw mode",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
        Some(current_mode)
    } else {
        None
    };

    // RAII restore + catch_unwind: a panic inside the read loop must
    // neither leave the console raw nor unwind across the extern "C"
    // dispatcher (which aborts the process). Mirrors the unix entry.
    let _restore = ModeRestoreGuard {
        handle,
        mode: saved_mode,
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        read_key_event(handle, &builder, out_value, out_error)
    }));

    match result {
        Ok(status) => status,
        Err(_) => {
            let err = builder.error(
                key_err::READ_KEY_PANIC,
                "ReadKeyPanic: read_key panicked (caught at FFI boundary)",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
    }
}

fn read_key_event(
    handle: HANDLE,
    builder: &HostValueBuilder<'_>,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let mut pending_high: Option<u16> = None;
    loop {
        let mut record: INPUT_RECORD = unsafe { core::mem::zeroed() };
        let mut count: u32 = 0;

        if unsafe { ReadConsoleInputW(handle, &mut record, 1, &mut count) } == 0 {
            let err = builder.error(
                key_err::READ_KEY_READ,
                "ReadKeyRead: ReadConsoleInputW failed",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }

        if count == 0 {
            let err = builder.error(key_err::READ_KEY_EOF, "ReadKeyEof: no input available");
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }

        if record.EventType as u32 != KEY_EVENT {
            continue;
        }

        let key_event = unsafe { record.Event.KeyEvent };
        if key_event.bKeyDown == 0 {
            continue;
        }

        let vk = key_event.wVirtualKeyCode;
        let ch = unsafe { key_event.uChar.UnicodeChar };
        let (ctrl, alt, shift) = decode_modifiers(key_event.dwControlKeyState);

        let kind = vk_to_key_kind(vk);
        let text = if (kind == key_kind::CHAR || kind == key_kind::UNKNOWN) && ch != 0 {
            match utf16_unit_to_text(ch, &mut pending_high) {
                Some(t) => t,
                // High surrogate stashed — the low half arrives as the
                // next KEY_EVENT record.
                None => continue,
            }
        } else {
            String::new()
        };

        let final_kind = if kind == key_kind::UNKNOWN && !text.is_empty() {
            key_kind::CHAR
        } else {
            kind
        };

        return build_key_pack(
            builder, final_kind, &text, ctrl, alt, shift, out_value, out_error,
        );
    }
}

fn build_key_pack(
    builder: &HostValueBuilder<'_>,
    kind: i64,
    text: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let kind_v = builder.int(kind);
    let text_v = builder.str(text);
    let ctrl_v = builder.bool(ctrl);
    let alt_v = builder.bool(alt);
    let shift_v = builder.bool(shift);

    // Match the unix contract: an allocation failure surfaces as a
    // deterministic addon error, never as `Ok` without an out_value
    // (which the host folds into a generic InvalidReturn).
    let children = [kind_v, text_v, ctrl_v, alt_v, shift_v];
    if children.iter().any(|v| v.is_null()) {
        release_values(builder, &children);
        return emit_error(
            builder,
            out_error,
            key_err::READ_KEY_RAW_MODE,
            "ReadKeyRawMode: failed to build return pack",
        );
    }

    let names: [*const c_char; 5] = [
        c"kind".as_ptr(),
        c"text".as_ptr(),
        c"ctrl".as_ptr(),
        c"alt".as_ptr(),
        c"shift".as_ptr(),
    ];
    let value = builder.pack(&names, &children);
    if value.is_null() {
        // Pack construction failed — ownership of the children stays
        // with us (ABI contract), so roll them back before erroring.
        release_values(builder, &children);
        return emit_error(
            builder,
            out_error,
            key_err::READ_KEY_RAW_MODE,
            "ReadKeyRawMode: failed to build return pack",
        );
    }
    if !out_value.is_null() {
        unsafe { *out_value = value };
    }
    TaidaAddonStatus::Ok
}

/// Release any non-null values after a partial pack-construction
/// failure so they do not leak host-side allocations.
fn release_values(builder: &HostValueBuilder<'_>, values: &[*mut TaidaAddonValueV1]) {
    let host = builder.as_raw();
    for &v in values {
        if !v.is_null() {
            unsafe { ((*host).value_release)(host, v) };
        }
    }
}

/// Emit a deterministic addon error (shared shape with the unix
/// entries).
fn emit_error(
    builder: &HostValueBuilder<'_>,
    out_error: *mut *mut TaidaAddonErrorV1,
    code: u32,
    message: &str,
) -> TaidaAddonStatus {
    let err = builder.error(code, message);
    if !out_error.is_null() {
        unsafe { *out_error = err };
    }
    TaidaAddonStatus::Error
}

// ── EventKind / MouseKind constants ─────────────────────────────

mod event_kind {
    pub const KEY: i64 = 0;
    pub const MOUSE: i64 = 1;
    pub const RESIZE: i64 = 2;
    #[allow(dead_code)]
    pub const UNKNOWN: i64 = 3;
}

mod mouse_kind {
    pub const DOWN: i64 = 0;
    pub const UP: i64 = 1;
    pub const MOVE: i64 = 2;
    pub const DRAG: i64 = 3;
    pub const SCROLL_UP: i64 = 4;
    pub const SCROLL_DOWN: i64 = 5;
}

// ── ReadEvent (TM-6d) ──────────────────────────────────────────

pub fn read_event_impl(
    host_ptr: *mut TaidaHostV1,
    _args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr as *const _) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    if !is_raw_mode_active() {
        let err = builder.error(
            event_err::READ_EVENT_NOT_IN_RAW_MODE,
            "ReadEventNotInRawMode: raw mode must be active before calling ReadEvent",
        );
        if !out_error.is_null() {
            unsafe { *out_error = err };
        }
        return TaidaAddonStatus::Error;
    }

    let handle = match get_console_handle(StreamKind::Stdin) {
        Some(h) => h,
        None => {
            let err = builder.error(
                event_err::READ_EVENT_NOT_A_TTY,
                "ReadEventNotATty: stdin is not a terminal",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }
    };

    // catch_unwind mirrors the unix entry: a panic in the decode loop
    // must surface as ReadEventPanic, not unwind across the
    // extern "C" dispatcher (process abort).
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        read_event_loop(handle, &builder, out_value, out_error)
    }));

    match result {
        Ok(status) => status,
        Err(_) => emit_error(
            &builder,
            out_error,
            event_err::READ_EVENT_PANIC,
            "ReadEventPanic: read_event panicked (caught at FFI boundary)",
        ),
    }
}

fn read_event_loop(
    handle: HANDLE,
    builder: &HostValueBuilder<'_>,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let mut pending_high: Option<u16> = None;
    loop {
        let mut record: INPUT_RECORD = unsafe { core::mem::zeroed() };
        let mut count: u32 = 0;

        if unsafe { ReadConsoleInputW(handle, &mut record, 1, &mut count) } == 0 {
            let err = builder.error(
                event_err::READ_EVENT_READ_FAILED,
                "ReadEventReadFailed: ReadConsoleInputW failed",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }

        if count == 0 {
            let err = builder.error(
                event_err::READ_EVENT_EOF,
                "ReadEventEof: no input available",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            return TaidaAddonStatus::Error;
        }

        match record.EventType as u32 {
            KEY_EVENT => {
                let key_event = unsafe { record.Event.KeyEvent };
                if key_event.bKeyDown == 0 {
                    continue;
                }

                let vk = key_event.wVirtualKeyCode;
                let ch = unsafe { key_event.uChar.UnicodeChar };
                let (ctrl, alt, shift) = decode_modifiers(key_event.dwControlKeyState);

                let kind = vk_to_key_kind(vk);
                let text = if (kind == key_kind::CHAR || kind == key_kind::UNKNOWN) && ch != 0 {
                    match utf16_unit_to_text(ch, &mut pending_high) {
                        Some(t) => t,
                        // High surrogate stashed — the low half arrives
                        // as the next KEY_EVENT record.
                        None => continue,
                    }
                } else {
                    String::new()
                };

                let final_kind = if kind == key_kind::UNKNOWN && !text.is_empty() {
                    key_kind::CHAR
                } else {
                    kind
                };

                let key_sub = build_key_subpack(builder, final_kind, &text, ctrl, alt, shift);
                let mouse_sub = build_default_mouse_subpack(builder);
                let resize_sub = build_default_resize_subpack(builder);

                return build_event_pack(
                    builder,
                    event_kind::KEY,
                    key_sub,
                    mouse_sub,
                    resize_sub,
                    out_value,
                    out_error,
                );
            }
            MOUSE_EVENT => {
                let mouse_event = unsafe { record.Event.MouseEvent };
                let flags = mouse_event.dwEventFlags;
                let buttons = mouse_event.dwButtonState;
                let (ctrl, alt, shift) = decode_modifiers(mouse_event.dwControlKeyState);
                let pos = mouse_event.dwMousePosition;

                const MOUSE_WHEELED: u32 = 0x0004;
                const MOUSE_MOVED: u32 = 0x0001;

                let (mk, button) = if flags & MOUSE_WHEELED != 0 {
                    let wheel_delta = (buttons >> 16) as i16;
                    if wheel_delta > 0 {
                        (mouse_kind::SCROLL_UP, 0i64)
                    } else {
                        (mouse_kind::SCROLL_DOWN, 0)
                    }
                } else if flags & MOUSE_MOVED != 0 {
                    if buttons != 0 {
                        (mouse_kind::DRAG, (buttons & 0x07) as i64)
                    } else {
                        (mouse_kind::MOVE, 0)
                    }
                } else if flags == 0 {
                    if buttons != 0 {
                        (mouse_kind::DOWN, (buttons & 0x07) as i64)
                    } else {
                        (mouse_kind::UP, 0)
                    }
                } else {
                    continue;
                };

                let col = (pos.X + 1) as i64;
                let row = (pos.Y + 1) as i64;

                let key_sub = build_default_key_subpack(builder);
                let mouse_sub =
                    build_mouse_subpack(builder, mk, col, row, button, ctrl, alt, shift);
                let resize_sub = build_default_resize_subpack(builder);

                return build_event_pack(
                    builder,
                    event_kind::MOUSE,
                    key_sub,
                    mouse_sub,
                    resize_sub,
                    out_value,
                    out_error,
                );
            }
            WINDOW_BUFFER_SIZE_EVENT => {
                let size_event = unsafe { record.Event.WindowBufferSizeEvent };
                let cols = size_event.dwSize.X as i64;
                let rows = size_event.dwSize.Y as i64;

                let key_sub = build_default_key_subpack(builder);
                let mouse_sub = build_default_mouse_subpack(builder);
                let resize_sub = build_resize_subpack(builder, cols, rows);

                return build_event_pack(
                    builder,
                    event_kind::RESIZE,
                    key_sub,
                    mouse_sub,
                    resize_sub,
                    out_value,
                    out_error,
                );
            }
            _ => continue,
        }
    }
}

// ── Event subpack builders ──────────────────────────────────────

fn build_key_subpack(
    builder: &HostValueBuilder<'_>,
    kind: i64,
    text: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> *mut TaidaAddonValueV1 {
    let names: [*const c_char; 5] = [
        c"kind".as_ptr(),
        c"text".as_ptr(),
        c"ctrl".as_ptr(),
        c"alt".as_ptr(),
        c"shift".as_ptr(),
    ];
    let values: [*mut TaidaAddonValueV1; 5] = [
        builder.int(kind),
        builder.str(text),
        builder.bool(ctrl),
        builder.bool(alt),
        builder.bool(shift),
    ];
    builder.pack(&names, &values)
}

fn build_default_key_subpack(builder: &HostValueBuilder<'_>) -> *mut TaidaAddonValueV1 {
    build_key_subpack(builder, key_kind::UNKNOWN, "", false, false, false)
}

fn build_mouse_subpack(
    builder: &HostValueBuilder<'_>,
    kind: i64,
    col: i64,
    row: i64,
    button: i64,
    ctrl: bool,
    alt: bool,
    shift: bool,
) -> *mut TaidaAddonValueV1 {
    let names: [*const c_char; 7] = [
        c"kind".as_ptr(),
        c"col".as_ptr(),
        c"row".as_ptr(),
        c"button".as_ptr(),
        c"ctrl".as_ptr(),
        c"alt".as_ptr(),
        c"shift".as_ptr(),
    ];
    let values: [*mut TaidaAddonValueV1; 7] = [
        builder.int(kind),
        builder.int(col),
        builder.int(row),
        builder.int(button),
        builder.bool(ctrl),
        builder.bool(alt),
        builder.bool(shift),
    ];
    builder.pack(&names, &values)
}

fn build_default_mouse_subpack(builder: &HostValueBuilder<'_>) -> *mut TaidaAddonValueV1 {
    build_mouse_subpack(builder, mouse_kind::MOVE, 0, 0, 0, false, false, false)
}

fn build_resize_subpack(
    builder: &HostValueBuilder<'_>,
    cols: i64,
    rows: i64,
) -> *mut TaidaAddonValueV1 {
    let names: [*const c_char; 2] = [c"cols".as_ptr(), c"rows".as_ptr()];
    let values: [*mut TaidaAddonValueV1; 2] = [builder.int(cols), builder.int(rows)];
    builder.pack(&names, &values)
}

fn build_default_resize_subpack(builder: &HostValueBuilder<'_>) -> *mut TaidaAddonValueV1 {
    build_resize_subpack(builder, 0, 0)
}

fn build_event_pack(
    builder: &HostValueBuilder<'_>,
    kind: i64,
    key_sub: *mut TaidaAddonValueV1,
    mouse_sub: *mut TaidaAddonValueV1,
    resize_sub: *mut TaidaAddonValueV1,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let values: [*mut TaidaAddonValueV1; 4] = [builder.int(kind), key_sub, mouse_sub, resize_sub];
    // Match the unix contract: allocation failure is a deterministic
    // addon error, never `Ok` without an out_value.
    if values.iter().any(|v| v.is_null()) {
        release_values(builder, &values);
        return emit_error(
            builder,
            out_error,
            event_err::READ_EVENT_READ_FAILED,
            "ReadEventReadFailed: failed to build return pack",
        );
    }
    let names: [*const c_char; 4] = [
        c"kind".as_ptr(),
        c"key".as_ptr(),
        c"mouse".as_ptr(),
        c"resize".as_ptr(),
    ];
    let value = builder.pack(&names, &values);
    if value.is_null() {
        // Pack construction failed — ownership of the children stays
        // with us (ABI contract), so roll them back before erroring.
        release_values(builder, &values);
        return emit_error(
            builder,
            out_error,
            event_err::READ_EVENT_READ_FAILED,
            "ReadEventReadFailed: failed to build return pack",
        );
    }
    if !out_value.is_null() {
        unsafe { *out_value = value };
    }
    TaidaAddonStatus::Ok
}

// ── Unit tests ──────────────────────────────────────────────────
//
// These tests verify platform-independent logic (VK mapping, error
// codes, constants). They compile only on Windows since this module
// is #[cfg(windows)] gated.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vk_to_key_kind_maps_special_keys() {
        assert_eq!(vk_to_key_kind(0x0D), key_kind::ENTER);
        assert_eq!(vk_to_key_kind(0x1B), key_kind::ESCAPE);
        assert_eq!(vk_to_key_kind(0x09), key_kind::TAB);
        assert_eq!(vk_to_key_kind(0x08), key_kind::BACKSPACE);
        assert_eq!(vk_to_key_kind(0x2E), key_kind::DELETE);
    }

    #[test]
    fn vk_to_key_kind_maps_arrows() {
        assert_eq!(vk_to_key_kind(0x26), key_kind::ARROW_UP);
        assert_eq!(vk_to_key_kind(0x28), key_kind::ARROW_DOWN);
        assert_eq!(vk_to_key_kind(0x25), key_kind::ARROW_LEFT);
        assert_eq!(vk_to_key_kind(0x27), key_kind::ARROW_RIGHT);
    }

    #[test]
    fn vk_to_key_kind_maps_navigation() {
        assert_eq!(vk_to_key_kind(0x24), key_kind::HOME);
        assert_eq!(vk_to_key_kind(0x23), key_kind::END);
        assert_eq!(vk_to_key_kind(0x21), key_kind::PAGE_UP);
        assert_eq!(vk_to_key_kind(0x22), key_kind::PAGE_DOWN);
        assert_eq!(vk_to_key_kind(0x2D), key_kind::INSERT);
    }

    #[test]
    fn vk_to_key_kind_maps_f_keys() {
        for i in 0u16..12 {
            assert_eq!(
                vk_to_key_kind(0x70 + i),
                key_kind::F1 + i as i64,
                "F{} mismatch",
                i + 1
            );
        }
    }

    #[test]
    fn vk_to_key_kind_unknown_for_unrecognized() {
        assert_eq!(vk_to_key_kind(0x00), key_kind::UNKNOWN);
        assert_eq!(vk_to_key_kind(0xFF), key_kind::UNKNOWN);
    }

    #[test]
    fn parse_stream_kind_valid_and_invalid() {
        assert_eq!(parse_stream_kind("stdin"), Some(StreamKind::Stdin));
        assert_eq!(parse_stream_kind("stdout"), Some(StreamKind::Stdout));
        assert_eq!(parse_stream_kind("stderr"), Some(StreamKind::Stderr));
        assert_eq!(parse_stream_kind("STDIN"), None);
        assert_eq!(parse_stream_kind(""), None);
    }

    #[test]
    fn key_kind_constants_match_terminal_td() {
        assert_eq!(key_kind::CHAR, 0);
        assert_eq!(key_kind::ENTER, 1);
        assert_eq!(key_kind::ESCAPE, 2);
        assert_eq!(key_kind::TAB, 3);
        assert_eq!(key_kind::BACKSPACE, 4);
        assert_eq!(key_kind::DELETE, 5);
        assert_eq!(key_kind::ARROW_UP, 6);
        assert_eq!(key_kind::ARROW_DOWN, 7);
        assert_eq!(key_kind::ARROW_LEFT, 8);
        assert_eq!(key_kind::ARROW_RIGHT, 9);
        assert_eq!(key_kind::HOME, 10);
        assert_eq!(key_kind::END, 11);
        assert_eq!(key_kind::PAGE_UP, 12);
        assert_eq!(key_kind::PAGE_DOWN, 13);
        assert_eq!(key_kind::INSERT, 14);
        assert_eq!(key_kind::UNKNOWN, 27);
    }

    #[test]
    fn event_kind_constants_match() {
        assert_eq!(event_kind::KEY, 0);
        assert_eq!(event_kind::MOUSE, 1);
        assert_eq!(event_kind::RESIZE, 2);
    }

    #[test]
    fn mouse_kind_constants_match() {
        assert_eq!(mouse_kind::DOWN, 0);
        assert_eq!(mouse_kind::UP, 1);
        assert_eq!(mouse_kind::MOVE, 2);
        assert_eq!(mouse_kind::DRAG, 3);
        assert_eq!(mouse_kind::SCROLL_UP, 4);
        assert_eq!(mouse_kind::SCROLL_DOWN, 5);
    }

    #[test]
    fn error_codes_match_unix() {
        assert_eq!(tty_err::IS_TERMINAL_INVALID_STREAM, 2101);
        assert_eq!(size_err::TERMINAL_SIZE_NOT_A_TTY, 2001);
        assert_eq!(size_err::TERMINAL_SIZE_IOCTL, 2002);
        assert_eq!(raw_err::RAW_MODE_NOT_A_TTY, 3001);
        assert_eq!(raw_err::RAW_MODE_ALREADY_ACTIVE, 3002);
        assert_eq!(raw_err::RAW_MODE_NOT_ACTIVE, 3003);
        assert_eq!(raw_err::RAW_MODE_ENTER_FAILED, 3004);
        assert_eq!(raw_err::RAW_MODE_LEAVE_FAILED, 3005);
        assert_eq!(key_err::READ_KEY_NOT_A_TTY, 1001);
        assert_eq!(event_err::READ_EVENT_NOT_IN_RAW_MODE, 4001);
    }

    #[test]
    fn read_key_unsupported_error_code_is_frozen() {
        // Windows-only error: VT mode init failure path for ReadKey.
        // Code 1007 follows the 1xxx ReadKey range (1001-1006 are Unix shared).
        assert_eq!(key_err::READ_KEY_UNSUPPORTED, 1007);
    }

    #[test]
    fn vt_mode_status_variants_exist() {
        // Ensure the VtModeStatus enum has the expected variants used
        // by the VT mode init check in read_key_impl.
        let _enabled = VtModeStatus::Enabled;
        let _failed = VtModeStatus::Failed;
        assert_ne!(VtModeStatus::Enabled, VtModeStatus::Failed);
    }

    #[test]
    fn decode_modifiers_parses_flags() {
        let (ctrl, alt, shift) = decode_modifiers(LEFT_CTRL_PRESSED | SHIFT_PRESSED);
        assert!(ctrl);
        assert!(!alt);
        assert!(shift);

        let (ctrl, alt, shift) = decode_modifiers(RIGHT_ALT_PRESSED);
        assert!(!ctrl);
        assert!(alt);
        assert!(!shift);

        let (ctrl, alt, shift) = decode_modifiers(0);
        assert!(!ctrl);
        assert!(!alt);
        assert!(!shift);
    }
}
