//! Integration test for `readKey()` non-TTY contract (RC2-2d).
//!
//! `cargo test` runs with stdin redirected from `/dev/null`, so when
//! the addon's `readKey()` entry point is invoked it must detect that
//! stdin is not a TTY *before* entering raw mode and surface a
//! deterministic `ReadKeyNotATty` error rather than blocking, faking a
//! key press, or corrupting the terminal.
//!
//! This file builds a minimal stub host capability table, captures it
//! via the addon's `terminal_init` callback, and then drives the real
//! `readKey()` entry point. The stub host implements `error_new` so
//! the integration test can read back the deterministic error code
//! produced by the addon.
//!
//! This is the integration-test counterpart to the pty-based unit
//! tests in `src/key.rs` which exercise the RAII restore guarantee
//! (RC2B-202). Together they cover the four behaviours mandated by
//! `RC2_DESIGN.md` Section D for native + non-TTY:
//!
//! - Non-TTY stdin → `ReadKeyNotATty` (this file)
//! - Raw mode failure → `ReadKeyRawMode` (covered by RawModeGuard
//!   error path; not exercised here because we cannot induce
//!   tcgetattr to fail on a CI runner without a pty)
//! - EOF / EINTR → `ReadKeyEof` / `ReadKeyInterrupted` (covered in
//!   the unit tests for the decoder; the I/O layer paths are exercised
//!   by the pty-based tests)

#![cfg(unix)]

use core::ffi::{CStr, c_char, c_void};
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use taida_addon::{
    TAIDA_ADDON_ABI_VERSION, TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1,
};

use taida_lang_terminal::__test_only;

// ── Stub host capability table ───────────────────────────────────
//
// All `value_new_*` callbacks return a fresh `Box<TaidaAddonValueV1>`
// leaked into a raw pointer. We track every allocation in
// `LIVE_VALUES` so the test can confirm release happens (the host
// owns the lifetime, but for the deterministic-error path we never
// hand the value back to anyone — the test releases it directly).
//
// `error_new` constructs a `TaidaAddonErrorV1` whose `code` is the
// deterministic variant we want to assert on, and whose `message` is
// a host-allocated nul-terminated UTF-8 buffer.

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
    // Reclaim the box. Children are not tracked in the stub because
    // none of the test paths build a list / pack with leaked children.
    let _ = unsafe { Box::from_raw(value) };
    LIVE_VALUES.fetch_sub(1, Ordering::AcqRel);
}

// Track the original `Box<CString>` pointer so `h_error_release` can
// reclaim it without trying to reinterpret the inner C-string buffer
// as a `Box<CString>`. The wrapper makes the raw pointers `Send` so
// they can live behind a `Mutex` for the duration of the test.
struct MsgEntry {
    inner: *const c_char,
    cstr_box: *mut std::ffi::CString,
}
// SAFETY: this test owns both pointers and never aliases them across
// threads concurrently — `Mutex` provides the synchronisation.
unsafe impl Send for MsgEntry {}

static MSG_REGISTRY: Mutex<Vec<MsgEntry>> = Mutex::new(Vec::new());

extern "C" fn h_error_new(
    _h: *const TaidaHostV1,
    code: u32,
    msg_ptr: *const u8,
    msg_len: usize,
) -> *mut TaidaAddonErrorV1 {
    // Copy the message into a host-allocated nul-terminated buffer.
    let bytes = if msg_ptr.is_null() || msg_len == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(msg_ptr, msg_len) }.to_vec()
    };
    let msg = String::from_utf8_lossy(&bytes).into_owned();
    *LAST_ERROR_CODE.lock().unwrap() = code;
    *LAST_ERROR_MSG.lock().unwrap() = msg.clone();

    // The addon ABI says `message` is a nul-terminated UTF-8 C string
    // owned by the host allocator. We allocate it via Box<CString> and
    // remember the box pointer in MSG_REGISTRY so `h_error_release`
    // can reclaim the original allocation cleanly.
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
            // SAFETY: cstr_box was produced by Box::into_raw above and
            // has not been freed since.
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

// ── The actual integration test ──────────────────────────────────

#[test]
fn read_key_returns_not_a_tty_error_when_stdin_is_not_a_tty() {
    // Sanity precondition: cargo test pipes /dev/null into stdin, so
    // STDIN_FILENO must not be a TTY here. If a developer somehow
    // runs `cargo test` from inside an interactive shell with stdin
    // attached to a real terminal, the test would block forever
    // waiting for a key press. Skip in that case rather than hang.
    let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
    if is_tty == 1 {
        eprintln!(
            "skipping non-TTY test: STDIN is a TTY in this test environment\n\
             (this happens when cargo test is run from a terminal without stdin redirection)"
        );
        return;
    }

    // Step 1: pull the function table via the test-only re-export.
    // The Native loader gets here via dlsym(taida_addon_get_v1) →
    // descriptor → functions; the in-process test uses the same
    // table directly. We still validate the table layout to keep
    // the v1 lock honest.
    let functions = __test_only::functions();
    // Append-only contract: TMB-016 made it 7 entries; TMB-020 /
    // Phase 8 appended 8 renderer entries → 15 total; TMB-022 /
    // Phase 9 appended `bufferBlit` at position 15 → 16 total.
    // The count can grow but never shrink.
    assert_eq!(functions.len(), 16);

    // Step 2: drive the init callback so the addon captures our host
    // pointer (this is the same handshake the Native loader performs).
    let host = Box::leak(Box::new(make_host()));
    let status = __test_only::init(host as *const _);
    assert_eq!(
        status,
        TaidaAddonStatus::Ok,
        "init must succeed with a valid host"
    );

    // Step 3: locate the readKey entry by walking the function table
    // (we don't hard-code the index because the table layout is
    // testable but not part of the user-facing surface).
    let mut read_key = None;
    for f in functions.iter() {
        let name = unsafe { CStr::from_ptr(f.name) }.to_str().unwrap();
        if name == "readKey" {
            read_key = Some(f);
            break;
        }
    }
    let read_key = read_key.expect("function table must contain readKey");
    assert_eq!(read_key.arity, 0);

    // Step 4: invoke readKey with no args. Expected outcome:
    // Status::Error and out_error filled with a deterministic
    // ReadKeyNotATty payload (per RC2_DESIGN.md Section D).
    *LAST_ERROR_CODE.lock().unwrap() = 0;
    LAST_ERROR_MSG.lock().unwrap().clear();
    let mut out_value: *mut TaidaAddonValueV1 = core::ptr::null_mut();
    let mut out_error: *mut TaidaAddonErrorV1 = core::ptr::null_mut();
    let status = (read_key.call)(
        core::ptr::null(),
        0,
        &mut out_value as *mut _,
        &mut out_error as *mut _,
    );
    assert_eq!(
        status,
        TaidaAddonStatus::Error,
        "readKey on a non-TTY stdin must return Status::Error, \
         not silently produce a fake key event"
    );
    assert!(
        out_value.is_null(),
        "non-TTY error path must not also produce a value pointer"
    );
    assert!(
        !out_error.is_null(),
        "non-TTY error path must populate out_error with a deterministic variant"
    );

    // Step 5: confirm the deterministic error variant is the one
    // pinned by RC2_DESIGN.md Section D — `ReadKeyNotATty`.
    let last_code = *LAST_ERROR_CODE.lock().unwrap();
    let last_msg = LAST_ERROR_MSG.lock().unwrap().clone();
    assert_eq!(
        last_code, 1001,
        "ReadKey non-TTY path must use error code 1001 (READ_KEY_NOT_A_TTY); got {}",
        last_code
    );
    assert!(
        last_msg.contains("ReadKeyNotATty"),
        "error message must include the variant name; got: {}",
        last_msg
    );

    // Step 6: clean up. The addon hands ownership of out_error to the
    // host, so the test (acting as host) is responsible for releasing
    // it via the same callback the host would invoke after the call.
    h_error_release(host as *const _, out_error);
}

#[test]
fn read_key_arity_mismatch_short_circuits_before_host_lookup() {
    // Mirrors the in-tree unit test for arity, but goes through the
    // public function pointer to confirm the dispatcher contract: an
    // arity error must come back *before* any host callback runs and
    // *before* any termios syscall is issued.
    let functions = __test_only::functions();
    let mut read_key = None;
    for f in functions.iter() {
        let name = unsafe { CStr::from_ptr(f.name) }.to_str().unwrap();
        if name == "readKey" {
            read_key = Some(f);
            break;
        }
    }
    let read_key = read_key.expect("readKey must be in the table");
    let status = (read_key.call)(
        core::ptr::null(),
        7, // intentionally wrong arity
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    );
    assert_eq!(status, TaidaAddonStatus::ArityMismatch);
}
