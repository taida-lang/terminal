//! Integration test for `Write[](bytes: Str) → Int` arity contract (TMB-016).
//!
//! Validates that the `write` dispatcher rejects any call with
//! `args_len != 1` **before** any host callback or syscall occurs, by
//! returning `TaidaAddonStatus::ArityMismatch`. This mirrors the
//! existing `readKey` / `readEvent` / `rawModeEnter` arity tests and
//! protects the ABI v1 append-only contract: the `write` entry is
//! locked at arity 1 and must never silently accept a different shape.
//!
//! Unlike the byte-count / non-TTY tests, this file does NOT need the
//! full host stub — the arity check short-circuits in `write_impl`
//! before dereferencing anything. We still wire up the standard stub
//! so a regression that pushes the arity check behind the host lookup
//! would be caught by the LIVE_VALUES / LAST_ERROR_CODE observers
//! staying at zero.

#![cfg(any(unix, windows))]

use core::ffi::{CStr, c_char, c_void};
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use taida_addon::{
    TAIDA_ADDON_ABI_VERSION, TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1,
};

use taida_lang_terminal::__test_only;

/// Serialize the arity tests so the `INT_CALLS` / `ERROR_CALLS`
/// observer snapshots are taken under a consistent view even when
/// libtest runs tests in parallel threads.
static TEST_LOCK: Mutex<()> = Mutex::new(());

// ── Stub host ────────────────────────────────────────────────────

static LIVE_VALUES: AtomicUsize = AtomicUsize::new(0);
static INT_CALLS: AtomicUsize = AtomicUsize::new(0);
static ERROR_CALLS: AtomicUsize = AtomicUsize::new(0);
static LAST_ERROR_CODE: Mutex<u32> = Mutex::new(0);

extern "C" fn h_value_new_unit(_h: *const TaidaHostV1) -> *mut TaidaAddonValueV1 {
    alloc_value(0, core::ptr::null_mut())
}
extern "C" fn h_value_new_int(_h: *const TaidaHostV1, _v: i64) -> *mut TaidaAddonValueV1 {
    INT_CALLS.fetch_add(1, Ordering::AcqRel);
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

extern "C" fn h_error_new(
    _h: *const TaidaHostV1,
    code: u32,
    _msg_ptr: *const u8,
    _msg_len: usize,
) -> *mut TaidaAddonErrorV1 {
    ERROR_CALLS.fetch_add(1, Ordering::AcqRel);
    *LAST_ERROR_CODE.lock().unwrap() = code;
    Box::into_raw(Box::new(TaidaAddonErrorV1 {
        code,
        _reserved: 0,
        message: core::ptr::null(),
    }))
}

extern "C" fn h_error_release(_h: *const TaidaHostV1, error: *mut TaidaAddonErrorV1) {
    if error.is_null() {
        return;
    }
    let _ = unsafe { Box::from_raw(error) };
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

// ── Arity contract tests ─────────────────────────────────────────

#[test]
fn write_arity_mismatch_when_zero_args() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Arity for `write` is locked at 1 (`bytes: Str`). Passing 0 args
    // must short-circuit with ArityMismatch before any host callback
    // runs and before any stdout syscall occurs.
    let functions = __test_only::functions();
    let host = Box::leak(Box::new(make_host()));
    let _ = __test_only::init(host as *const _);
    let write = find_write(functions);
    assert_eq!(write.arity, 1, "write is the TMB-016 entry, arity 1");

    // Snapshot observers.
    let int_before = INT_CALLS.load(Ordering::Acquire);
    let error_before = ERROR_CALLS.load(Ordering::Acquire);

    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
    let status = (write.call)(
        core::ptr::null(),
        0,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );

    assert_eq!(
        status,
        TaidaAddonStatus::ArityMismatch,
        "Write with 0 args must return ArityMismatch"
    );
    assert!(
        out_value.is_null(),
        "arity mismatch must NOT build a return value"
    );
    assert!(
        out_error.is_null(),
        "arity mismatch must NOT build a host error (the Status code is the only signal)"
    );
    assert_eq!(
        INT_CALLS.load(Ordering::Acquire),
        int_before,
        "arity mismatch must short-circuit before value_new_int"
    );
    assert_eq!(
        ERROR_CALLS.load(Ordering::Acquire),
        error_before,
        "arity mismatch must short-circuit before error_new"
    );
}

#[test]
fn write_arity_mismatch_when_two_args() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Symmetric case: passing 2 args must also short-circuit.
    let functions = __test_only::functions();
    let host = Box::leak(Box::new(make_host()));
    let _ = __test_only::init(host as *const _);
    let write = find_write(functions);

    let int_before = INT_CALLS.load(Ordering::Acquire);
    let error_before = ERROR_CALLS.load(Ordering::Acquire);

    let status = (write.call)(
        core::ptr::null(),
        2, // intentionally wrong arity
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );

    assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    assert_eq!(INT_CALLS.load(Ordering::Acquire), int_before);
    assert_eq!(ERROR_CALLS.load(Ordering::Acquire), error_before);
}

#[test]
fn write_arity_mismatch_short_circuits_before_host_lookup() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // Even without `terminal_init`, the arity check must fire first so
    // a caller that passes the wrong shape never reaches the
    // HOST_PTR.load() branch. This mirrors
    // `read_key_arity_mismatch_short_circuits_before_host_lookup` in
    // `tests/read_key_non_tty.rs`.
    let functions = __test_only::functions();
    let write = find_write(functions);

    let status = (write.call)(
        core::ptr::null(),
        7, // arbitrary wrong arity
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );

    assert_eq!(status, TaidaAddonStatus::ArityMismatch);
}
