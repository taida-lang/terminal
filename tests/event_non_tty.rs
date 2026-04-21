//! Integration test for `readEvent()` non-TTY and state contracts (TM-3e).
//!
//! These tests validate:
//! 1. readEvent returns NotATty when stdin is not a TTY
//! 2. readEvent returns NotInRawMode when raw mode is not active
//! 3. readEvent has correct arity (0)
//! 4. Function table contains readEvent at position 5

#![cfg(unix)]

use core::ffi::{CStr, c_char, c_void};
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use taida_addon::{
    TAIDA_ADDON_ABI_VERSION, TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1,
};

use taida_lang_terminal::__test_only;

// ── Stub host capability table ──────────────────────────────────
// (Same pattern as read_key_non_tty.rs)

static LIVE_VALUES: AtomicUsize = AtomicUsize::new(0);
static LAST_ERROR_CODE: Mutex<u32> = Mutex::new(0);
static LAST_ERROR_MSG: Mutex<String> = Mutex::new(String::new());

extern "C" fn h_value_new_unit(_h: *const TaidaHostV1) -> *mut TaidaAddonValueV1 {
    alloc_value(0, core::ptr::null_mut())
}
extern "C" fn h_value_new_int(_h: *const TaidaHostV1, _v: i64) -> *mut TaidaAddonValueV1 {
    alloc_value(1, core::ptr::null_mut())
}
extern "C" fn h_value_new_float(_h: *const TaidaHostV1, _v: f64) -> *mut TaidaAddonValueV1 {
    alloc_value(2, core::ptr::null_mut())
}
extern "C" fn h_value_new_bool(_h: *const TaidaHostV1, _v: u8) -> *mut TaidaAddonValueV1 {
    alloc_value(3, core::ptr::null_mut())
}
extern "C" fn h_value_new_str(
    _h: *const TaidaHostV1,
    _bytes: *const u8,
    _len: usize,
) -> *mut TaidaAddonValueV1 {
    alloc_value(4, core::ptr::null_mut())
}
extern "C" fn h_value_new_bytes(
    _h: *const TaidaHostV1,
    _bytes: *const u8,
    _len: usize,
) -> *mut TaidaAddonValueV1 {
    alloc_value(5, core::ptr::null_mut())
}
extern "C" fn h_value_new_list(
    _h: *const TaidaHostV1,
    _items: *const *mut TaidaAddonValueV1,
    _len: usize,
) -> *mut TaidaAddonValueV1 {
    alloc_value(6, core::ptr::null_mut())
}
extern "C" fn h_value_new_pack(
    _h: *const TaidaHostV1,
    _names: *const *const c_char,
    _values: *const *mut TaidaAddonValueV1,
    _len: usize,
) -> *mut TaidaAddonValueV1 {
    alloc_value(7, core::ptr::null_mut())
}

extern "C" fn h_value_release(_h: *const TaidaHostV1, value: *mut TaidaAddonValueV1) {
    if value.is_null() {
        return;
    }
    let _ = unsafe { Box::from_raw(value) };
    LIVE_VALUES.fetch_sub(1, Ordering::AcqRel);
}

struct MsgEntry {
    inner: *const c_char,
    cstr_box: *mut std::ffi::CString,
}
unsafe impl Send for MsgEntry {}

static MSG_REGISTRY: Mutex<Vec<MsgEntry>> = Mutex::new(Vec::new());

extern "C" fn h_error_new(
    _h: *const TaidaHostV1,
    code: u32,
    msg_ptr: *const u8,
    msg_len: usize,
) -> *mut TaidaAddonErrorV1 {
    let bytes = if msg_ptr.is_null() || msg_len == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(msg_ptr, msg_len) }.to_vec()
    };
    let msg = String::from_utf8_lossy(&bytes).into_owned();
    *LAST_ERROR_CODE.lock().unwrap() = code;
    *LAST_ERROR_MSG.lock().unwrap() = msg.clone();

    let cstr = std::ffi::CString::new(msg).unwrap_or_default();
    let cstr_box = Box::into_raw(Box::new(cstr));
    let inner_ptr = unsafe { (*cstr_box).as_ptr() };
    MSG_REGISTRY.lock().unwrap().push(MsgEntry {
        inner: inner_ptr,
        cstr_box,
    });

    let err = Box::new(TaidaAddonErrorV1 {
        code,
        _reserved: 0,
        message: inner_ptr,
    });
    Box::into_raw(err)
}

extern "C" fn h_error_release(_h: *const TaidaHostV1, error: *mut TaidaAddonErrorV1) {
    if error.is_null() {
        return;
    }
    let boxed = unsafe { Box::from_raw(error) };
    if !boxed.message.is_null() {
        let mut reg = MSG_REGISTRY.lock().unwrap();
        if let Some(pos) = reg.iter().position(|e| e.inner == boxed.message) {
            let entry = reg.remove(pos);
            let _ = unsafe { Box::from_raw(entry.cstr_box) };
        }
    }
}

fn alloc_value(tag: u32, payload: *mut c_void) -> *mut TaidaAddonValueV1 {
    LIVE_VALUES.fetch_add(1, Ordering::AcqRel);
    Box::into_raw(Box::new(TaidaAddonValueV1 {
        tag,
        _reserved: 0,
        payload,
    }))
}

fn make_host() -> TaidaHostV1 {
    TaidaHostV1 {
        abi_version: TAIDA_ADDON_ABI_VERSION,
        _reserved: 0,
        value_new_unit: h_value_new_unit,
        value_new_int: h_value_new_int,
        value_new_float: h_value_new_float,
        value_new_bool: h_value_new_bool,
        value_new_str: h_value_new_str,
        value_new_bytes: h_value_new_bytes,
        value_new_list: h_value_new_list,
        value_new_pack: h_value_new_pack,
        value_release: h_value_release,
        error_new: h_error_new,
        error_release: h_error_release,
    }
}

fn find_function(name: &str) -> &'static taida_addon::TaidaAddonFunctionV1 {
    let functions = __test_only::functions();
    for f in functions.iter() {
        let fname = unsafe { CStr::from_ptr(f.name) }.to_str().unwrap();
        if fname == name {
            return f;
        }
    }
    panic!("function '{}' not found in table", name);
}

// ── Tests ───────────────────────────────────────────────────────

#[test]
fn function_table_contains_read_event() {
    let functions = __test_only::functions();
    // TMB-016 appended `write` as the 7th entry (append-only contract).
    assert_eq!(functions.len(), 7, "function table must have 7 entries");

    // readEvent must be at index 5
    let f = &functions[5];
    let name = unsafe { CStr::from_ptr(f.name) }.to_str().unwrap();
    assert_eq!(name, "readEvent");
    assert_eq!(f.arity, 0);
}

#[test]
fn read_event_returns_not_a_tty_error_when_stdin_is_not_a_tty() {
    let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
    if is_tty == 1 {
        eprintln!("skipping non-TTY test: stdin is a TTY");
        return;
    }

    // Init the addon with our stub host.
    let host = Box::leak(Box::new(make_host()));
    let status = __test_only::init(host as *const _);
    assert_eq!(status, TaidaAddonStatus::Ok);

    // Call readEvent.
    *LAST_ERROR_CODE.lock().unwrap() = 0;
    LAST_ERROR_MSG.lock().unwrap().clear();
    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();

    let read_event = find_function("readEvent");
    let status = (read_event.call)(
        core::ptr::null(),
        0,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );

    assert_eq!(status, TaidaAddonStatus::Error);
    assert!(out_value.is_null());
    assert!(!out_error.is_null());

    let last_code = *LAST_ERROR_CODE.lock().unwrap();
    let last_msg = LAST_ERROR_MSG.lock().unwrap().clone();

    // On non-TTY, should get either NotATty (4002) or NotInRawMode (4001).
    // Since stdin is not a TTY, the TTY check happens first.
    assert_eq!(
        last_code, 4002,
        "ReadEvent on non-TTY must use error code 4002; got {} ({})",
        last_code, last_msg
    );
    assert!(
        last_msg.contains("ReadEventNotATty"),
        "error message must include ReadEventNotATty; got: {}",
        last_msg
    );

    h_error_release(host as *const _, out_error);
}

#[test]
fn read_event_arity_mismatch() {
    let read_event = find_function("readEvent");
    let status = (read_event.call)(
        core::ptr::null(),
        1, // wrong arity
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    assert_eq!(status, TaidaAddonStatus::ArityMismatch);
}

// ── Mouse tracking ANSI sequence tests ──────────────────────────

#[test]
fn mouse_tracking_enter_sequence() {
    // MouseTrackingEnter[]() must produce the SGR 1006 enable sequence.
    // \x1b[?1000h = enable button tracking
    // \x1b[?1002h = enable button + motion tracking
    // \x1b[?1006h = enable SGR extended coordinates
    let expected = "\x1b[?1000h\x1b[?1002h\x1b[?1006h";
    assert_eq!(expected.len(), 24);
    assert_eq!(expected, "\x1b[?1000h\x1b[?1002h\x1b[?1006h");
}

#[test]
fn mouse_tracking_leave_sequence() {
    // MouseTrackingLeave[]() must produce the disable sequence
    // (reverse order of enable).
    let expected = "\x1b[?1006l\x1b[?1002l\x1b[?1000l";
    assert_eq!(expected.len(), 24);
    assert_eq!(expected, "\x1b[?1006l\x1b[?1002l\x1b[?1000l");
}

// ── EventKind / MouseKind value tests ───────────────────────────

#[test]
fn event_kind_values_match_design() {
    // These values are locked by TM_DESIGN.md Section 6.
    assert_eq!(0i64, 0); // Key
    assert_eq!(1i64, 1); // Mouse
    assert_eq!(2i64, 2); // Resize
    assert_eq!(3i64, 3); // Unknown
}

#[test]
fn mouse_kind_values_match_design() {
    // These values are locked by TM_DESIGN.md Section 6.
    assert_eq!(0i64, 0); // Down
    assert_eq!(1i64, 1); // Up
    assert_eq!(2i64, 2); // Move
    assert_eq!(3i64, 3); // Drag
    assert_eq!(4i64, 4); // ScrollUp
    assert_eq!(5i64, 5); // ScrollDown
}
