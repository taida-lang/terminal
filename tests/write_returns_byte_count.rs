//! Integration test for `Write[](bytes: Str) → Int` (TMB-016).
//!
//! Validates that the addon's `write` entry:
//!
//! 1. Accepts a `Str` argument wrapped in a host-owned `TaidaAddonValueV1`.
//! 2. Routes the payload bytes to `io::stdout().write_all + flush`.
//! 3. Returns an `Int` whose value is exactly the written byte count.
//! 4. Reports `TaidaAddonStatus::Ok` on the success path (no out_error).
//!
//! This is the TMB-016 counterpart to `tests/read_key_non_tty.rs`:
//! instead of stubbing only the `error_new` callback, we also instrument
//! `value_new_int` so the test can observe the exact integer handed back.
//!
//! The stdout target during `cargo test` is libtest's capture buffer,
//! which is a `Write` impl backed by a pipe-like pair. That makes this
//! file cover the "non-TTY (pipe) success path" contract as a side
//! effect, while the dedicated `write_non_tty.rs` asserts the contract
//! more explicitly.

#![cfg(any(unix, windows))]

use core::ffi::{CStr, c_char, c_void};
use core::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;

use taida_addon::{
    TAIDA_ADDON_ABI_VERSION, TaidaAddonBytesPayload, TaidaAddonErrorV1, TaidaAddonStatus,
    TaidaAddonValueTag, TaidaAddonValueV1, TaidaHostV1,
};

use taida_lang_terminal::__test_only;

/// Serialize the four tests in this file: they all read/write the
/// same `LAST_INT_VALUE` / `LAST_ERROR_CODE` globals, and libtest runs
/// tests within a single binary in parallel threads. Holding this
/// mutex for the duration of each test body keeps the observers
/// coherent without requiring `--test-threads=1`.
static TEST_LOCK: Mutex<()> = Mutex::new(());

// ── Stub host capability table ──────────────────────────────────
//
// Same general shape as the sibling integration tests. The key
// difference is `h_value_new_int`: it records the i64 into
// `LAST_INT_VALUE` so the assertion can prove the byte count was
// passed through faithfully.

static LIVE_VALUES: AtomicUsize = AtomicUsize::new(0);
static LAST_INT_VALUE: AtomicI64 = AtomicI64::new(i64::MIN); // sentinel
static LAST_ERROR_CODE: Mutex<u32> = Mutex::new(0);
static LAST_ERROR_MSG: Mutex<String> = Mutex::new(String::new());

extern "C" fn h_value_new_unit(_h: *const TaidaHostV1) -> *mut TaidaAddonValueV1 {
    alloc_value(0, core::ptr::null_mut())
}
extern "C" fn h_value_new_int(_h: *const TaidaHostV1, v: i64) -> *mut TaidaAddonValueV1 {
    LAST_INT_VALUE.store(v, Ordering::Release);
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
// SAFETY: all registry access is serialised through `MSG_REGISTRY`.
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
    // SAFETY: cstr_box is a valid Box<CString> just produced.
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
            // SAFETY: cstr_box was produced by Box::into_raw above.
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

/// Build a host-owned Str argument wrapper around `bytes`. Returns
/// `(value, payload_box)` so the caller can keep both alive for the
/// duration of the call and release them afterwards.
///
/// The TaidaAddonValueV1 carries `tag = Str (4)` and `payload` points
/// to a heap-allocated `TaidaAddonBytesPayload` describing the bytes.
/// Both allocations are leaked via `Box::into_raw` and reclaimed
/// explicitly after the call.
fn make_str_arg(bytes: &'static [u8]) -> (TaidaAddonValueV1, *mut TaidaAddonBytesPayload) {
    let payload = Box::into_raw(Box::new(TaidaAddonBytesPayload {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    }));
    let value = TaidaAddonValueV1 {
        tag: TaidaAddonValueTag::Str as u32,
        _reserved: 0,
        payload: payload as *mut c_void,
    };
    (value, payload)
}

fn find_write(
    functions: &[taida_addon::TaidaAddonFunctionV1],
) -> &taida_addon::TaidaAddonFunctionV1 {
    for f in functions.iter() {
        let name = unsafe { CStr::from_ptr(f.name) }.to_str().unwrap();
        if name == "write" {
            return f;
        }
    }
    panic!("function table must contain 'write' (TMB-016)");
}

// ── The integration tests ────────────────────────────────────────

#[test]
fn write_returns_byte_count_for_ascii_payload() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // 1. Wire up the stub host and run init.
    let functions = __test_only::functions();
    let host = Box::leak(Box::new(make_host()));
    let status = __test_only::init(host as *const _);
    assert_eq!(
        status,
        TaidaAddonStatus::Ok,
        "init must succeed with a valid host"
    );
    let write = find_write(functions);
    assert_eq!(write.arity, 1);

    // 2. Build the argument vector with a single Str value.
    let payload = b"hello"; // 5 bytes
    let (arg_value, payload_box) = make_str_arg(payload);
    let args = [arg_value];

    // 3. Reset observer state.
    LAST_INT_VALUE.store(i64::MIN, Ordering::Release);
    *LAST_ERROR_CODE.lock().unwrap() = 0;
    LAST_ERROR_MSG.lock().unwrap().clear();

    // 4. Invoke write.
    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
    let status = (write.call)(
        args.as_ptr(),
        args.len() as u32,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );

    // 5. Assertions: Ok path, value set, no error, int == byte count.
    assert_eq!(
        status,
        TaidaAddonStatus::Ok,
        "Write on a valid Str argument must succeed (got error code {})",
        *LAST_ERROR_CODE.lock().unwrap()
    );
    assert!(
        !out_value.is_null(),
        "success path must populate out_value with the returned Int"
    );
    assert!(
        out_error.is_null(),
        "success path must NOT populate out_error"
    );
    assert_eq!(
        LAST_INT_VALUE.load(Ordering::Acquire),
        payload.len() as i64,
        "Write must return exactly the number of bytes handed to write_all"
    );

    // 6. Clean up the returned value and our argument allocation.
    h_value_release(host as *const _, out_value);
    // SAFETY: payload_box was produced by Box::into_raw in make_str_arg.
    let _ = unsafe { Box::from_raw(payload_box) };
}

#[test]
fn write_returns_byte_count_for_ansi_escape_payload() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // The primary TMB-016 use case: TUI apps emit ANSI escape sequences
    // with NO trailing newline. The return value is the full byte
    // length including every escape byte.
    let functions = __test_only::functions();
    let host = Box::leak(Box::new(make_host()));
    let _ = __test_only::init(host as *const _);
    let write = find_write(functions);

    let payload = b"\x1b[2J\x1b[H"; // clear screen + cursor home, 7 bytes
    let (arg_value, payload_box) = make_str_arg(payload);
    let args = [arg_value];
    LAST_INT_VALUE.store(i64::MIN, Ordering::Release);

    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
    let status = (write.call)(
        args.as_ptr(),
        args.len() as u32,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );

    assert_eq!(status, TaidaAddonStatus::Ok);
    assert_eq!(LAST_INT_VALUE.load(Ordering::Acquire), 7);

    h_value_release(host as *const _, out_value);
    let _ = unsafe { Box::from_raw(payload_box) };
}

#[test]
fn write_returns_zero_for_empty_payload() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Empty write is a legitimate no-op (e.g. a conditional redraw
    // that turned out to produce no ANSI bytes). The returned count
    // must be exactly 0, and the call must still flush cleanly.
    let functions = __test_only::functions();
    let host = Box::leak(Box::new(make_host()));
    let _ = __test_only::init(host as *const _);
    let write = find_write(functions);

    let (arg_value, payload_box) = make_str_arg(b"");
    let args = [arg_value];
    LAST_INT_VALUE.store(i64::MIN, Ordering::Release);

    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
    let status = (write.call)(
        args.as_ptr(),
        args.len() as u32,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );

    assert_eq!(status, TaidaAddonStatus::Ok);
    assert_eq!(LAST_INT_VALUE.load(Ordering::Acquire), 0);

    h_value_release(host as *const _, out_value);
    let _ = unsafe { Box::from_raw(payload_box) };
}

#[test]
fn write_returns_utf8_byte_count_not_char_count() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // The `bytes: Str` parameter is a UTF-8 Str; the returned Int is
    // the raw byte count, not the char / grapheme count. This pins the
    // contract documented in `Write[]()` doc comments.
    let functions = __test_only::functions();
    let host = Box::leak(Box::new(make_host()));
    let _ = __test_only::init(host as *const _);
    let write = find_write(functions);

    // "あいう" is 3 chars × 3 bytes = 9 bytes in UTF-8.
    const UTF8_PAYLOAD: &[u8] = "あいう".as_bytes();
    let (arg_value, payload_box) = make_str_arg(UTF8_PAYLOAD);
    let args = [arg_value];
    LAST_INT_VALUE.store(i64::MIN, Ordering::Release);

    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
    let status = (write.call)(
        args.as_ptr(),
        args.len() as u32,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );

    assert_eq!(status, TaidaAddonStatus::Ok);
    assert_eq!(
        LAST_INT_VALUE.load(Ordering::Acquire),
        9,
        "Write must return byte count (9), not char count (3)"
    );

    h_value_release(host as *const _, out_value);
    let _ = unsafe { Box::from_raw(payload_box) };
}
