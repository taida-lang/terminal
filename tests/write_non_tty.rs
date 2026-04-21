//! Integration test for `Write[](bytes: Str) → Int` non-TTY contract (TMB-016).
//!
//! The TMB-016 contract explicitly classifies "non-TTY stdout (pipe /
//! redirect)" as a **success path**, not an error: TUI apps that
//! happen to run with stdout redirected must continue to work, with
//! every byte (including ANSI escapes) handed to the underlying
//! stream. This is the opposite of `readKey` / `readEvent`, whose
//! contract rejects non-TTY input because raw mode is meaningless
//! there.
//!
//! `cargo test` runs with stdout captured by libtest into a pipe-like
//! buffer, so `isatty(STDOUT_FILENO) == 0` during the test. That is
//! exactly the scenario we want to pin: the addon must not short-
//! circuit with `WriteFailed` just because stdout is a pipe.

#![cfg(any(unix, windows))]

use core::ffi::{CStr, c_char, c_void};
use core::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;

use taida_addon::{
    TAIDA_ADDON_ABI_VERSION, TaidaAddonBytesPayload, TaidaAddonErrorV1, TaidaAddonStatus,
    TaidaAddonValueTag, TaidaAddonValueV1, TaidaHostV1,
};

use taida_lang_terminal::__test_only;

/// Serialize parallel test threads that share the `LAST_INT_VALUE` /
/// `LAST_ERROR_CODE` globals. See `write_returns_byte_count.rs` for
/// the full rationale.
static TEST_LOCK: Mutex<()> = Mutex::new(());

// ── Stub host (same skeleton as the siblings) ────────────────────

static LIVE_VALUES: AtomicUsize = AtomicUsize::new(0);
static LAST_INT_VALUE: AtomicI64 = AtomicI64::new(i64::MIN);
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

// ── Tests ────────────────────────────────────────────────────────

#[test]
fn write_succeeds_when_stdout_is_not_a_tty() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Precondition sanity check: libtest's stdout capture makes
    // STDOUT_FILENO a pipe under `cargo test`. If the developer runs
    // tests with `--nocapture` attached to a real TTY, the contract
    // still holds (Write always succeeds) so we proceed unconditionally.
    #[cfg(unix)]
    {
        let _is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) };
        // No skip: the TMB-016 contract must hold in both cases.
    }

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

    // Reset observer state.
    LAST_INT_VALUE.store(i64::MIN, Ordering::Release);
    *LAST_ERROR_CODE.lock().unwrap() = 0;
    LAST_ERROR_MSG.lock().unwrap().clear();

    let payload = b"tui-frame-not-a-tty";
    let (arg_value, payload_box) = make_str_arg(payload);
    let args = [arg_value];

    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
    let status = (write.call)(
        args.as_ptr(),
        args.len() as u32,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );

    // Non-TTY success contract: status = Ok, value populated, no error.
    assert_eq!(
        status,
        TaidaAddonStatus::Ok,
        "Write on a non-TTY stdout must succeed (got code {}, msg: {})",
        *LAST_ERROR_CODE.lock().unwrap(),
        LAST_ERROR_MSG.lock().unwrap()
    );
    assert!(
        !out_value.is_null(),
        "non-TTY success path must still allocate the Int return value"
    );
    assert!(
        out_error.is_null(),
        "non-TTY must NOT populate out_error (Write is not TTY-gated)"
    );
    assert_eq!(
        LAST_INT_VALUE.load(Ordering::Acquire),
        payload.len() as i64,
        "non-TTY byte count must still equal the payload length"
    );
    // Error observer must have stayed untouched — prove the addon did
    // not produce any deterministic error payload.
    assert_eq!(
        *LAST_ERROR_CODE.lock().unwrap(),
        0,
        "non-TTY path must not build a WriteFailed error"
    );

    h_value_release(host as *const _, out_value);
    let _ = unsafe { Box::from_raw(payload_box) };
}

#[test]
fn write_does_not_panic_on_non_tty() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Explicit re-assertion of the "non-TTY でも panic せず動作" clause:
    // we already exercised this above, but re-run inside
    // `catch_unwind` on the test side so a future regression that
    // panics in the write path is caught even if it somehow bypasses
    // the addon's own `catch_unwind` barrier.
    let functions = __test_only::functions();
    let host = Box::leak(Box::new(make_host()));
    let _ = __test_only::init(host as *const _);
    let write = find_write(functions);

    let payload = b"\x1b[1;1H"; // cursor home, no newline
    let (arg_value, payload_box) = make_str_arg(payload);
    let args = [arg_value];

    let result = std::panic::catch_unwind(|| {
        let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
        let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
        (write.call)(
            args.as_ptr(),
            args.len() as u32,
            &mut out_value as *mut _,
            &mut out_error as *mut _,
        )
    });
    assert!(
        result.is_ok(),
        "Write must not unwind across the FFI boundary on non-TTY stdout"
    );
    assert_eq!(result.unwrap(), TaidaAddonStatus::Ok);

    let _ = unsafe { Box::from_raw(payload_box) };
}
