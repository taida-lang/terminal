//! `taida-lang-terminal` — terminal package implementation.
//!
//! This crate is the production `taida-lang/terminal` package. The v1
//! surface is frozen in `.dev/RC2_DESIGN.md` in the main `taida`
//! repository:
//!
//! - `terminalSize()` → `@(cols: Int, rows: Int)`
//! - `readKey()` → `@(kind: KeyKind, text: Str, ctrl: Bool, alt: Bool, shift: Bool)`
//! - `isTerminal(stream)` → `Bool`
//!
//! Phase 2 landed `readKey()` with the full RAII raw-mode lifecycle
//! mandated by `RC2_BLOCKERS.md` RC2B-202. The decoder lives in
//! [`key`] so unit tests can drive it without a real TTY.
//!
//! Phase 3 lands `terminalSize()` with the non-TTY / ioctl /
//! degenerate-zero contract mandated by RC2B-203. The probe lives in
//! [`size`] and is driven through the same host-builder bridge as
//! `readKey`.
//!
//! Non-unix targets compile but every function returns `Error`. The
//! Native loader rejects non-native backends at import time via the
//! RC1 `addon::backend_policy` (no runtime fallback).

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(unix)]
mod event;
#[cfg(unix)]
mod key;
#[cfg(unix)]
mod raw_mode;
#[cfg(unix)]
mod size;
#[cfg(unix)]
mod tty;

#[cfg(windows)]
mod windows;

// `write` is platform-shared: `io::stdout().write_all + flush` works
// identically on Unix and Windows and does not require platform-gated
// syscalls. Introduced by TMB-016.
#[cfg(any(unix, windows))]
mod write;

// `renderer` is platform-shared: pure value computation on
// ScreenBuffer / Cell / DiffOp values, no syscalls. Introduced by
// TMB-020 (Phase 8) to migrate the pure-Taida renderer core off the
// O(N²) list-replace path.
//
// Stays `pub(crate)` so the addon entries above and the bench
// re-export below can reach it without exposing the raw FFI
// `*_impl` signatures (which would trip clippy::not_unsafe_ptr_arg_deref).
#[cfg(any(unix, windows))]
pub(crate) mod renderer;

use core::ffi::c_char;
use core::sync::atomic::{AtomicPtr, Ordering};

use taida_addon::{
    TaidaAddonErrorV1, TaidaAddonFunctionV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1,
};

/// Captured host callback table. Populated by `terminal_init` and read
/// by per-call entry points.
static HOST_PTR: AtomicPtr<TaidaHostV1> = AtomicPtr::new(core::ptr::null_mut());

extern "C" fn terminal_init(host: *const TaidaHostV1) -> TaidaAddonStatus {
    if host.is_null() {
        return TaidaAddonStatus::NullPointer;
    }
    HOST_PTR.store(host as *mut _, Ordering::Release);
    TaidaAddonStatus::Ok
}

// ── terminalSize (Phase 3) ───────────────────────────────────────

extern "C" fn terminal_size(
    _args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 0 {
        return TaidaAddonStatus::ArityMismatch;
    }
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    #[cfg(unix)]
    {
        // Phase 3 body. Owns the isatty + ioctl(TIOCGWINSZ) probe and
        // the deterministic error variants (`TerminalSizeNotATty`,
        // `TerminalSizeIoctl`). See `size.rs`.
        size::terminal_size_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(windows)]
    {
        windows::terminal_size_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = out_value;
        let _ = out_error;
        TaidaAddonStatus::Error
    }
}

// ── readKey (Phase 2) ────────────────────────────────────────────

extern "C" fn read_key(
    _args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 0 {
        return TaidaAddonStatus::ArityMismatch;
    }
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    #[cfg(unix)]
    {
        // The real Phase 2 body. Owns the raw-mode RAII guard,
        // catch_unwind, non-TTY detection, and escape-sequence
        // decoding. See `key.rs`.
        key::read_key_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(windows)]
    {
        windows::read_key_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = out_value;
        let _ = out_error;
        TaidaAddonStatus::Error
    }
}

// ── isTerminal (TM-2c) ───────────────────────────────────────────

extern "C" fn is_terminal(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 1 {
        return TaidaAddonStatus::ArityMismatch;
    }
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    #[cfg(unix)]
    {
        tty::is_terminal_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }

    #[cfg(windows)]
    {
        windows::is_terminal_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = args_ptr;
        let _ = out_value;
        let _ = out_error;
        TaidaAddonStatus::Error
    }
}

// ── rawModeEnter (TM-2d) ────────────────────────────────────────

extern "C" fn raw_mode_enter(
    _args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 0 {
        return TaidaAddonStatus::ArityMismatch;
    }
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    #[cfg(unix)]
    {
        raw_mode::raw_mode_enter_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(windows)]
    {
        windows::raw_mode_enter_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = out_value;
        let _ = out_error;
        TaidaAddonStatus::Error
    }
}

// ── rawModeLeave (TM-2d) ───────────────────────────────────────

extern "C" fn raw_mode_leave(
    _args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 0 {
        return TaidaAddonStatus::ArityMismatch;
    }
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    #[cfg(unix)]
    {
        raw_mode::raw_mode_leave_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(windows)]
    {
        windows::raw_mode_leave_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = out_value;
        let _ = out_error;
        TaidaAddonStatus::Error
    }
}

// ── readEvent (TM-3d) ──────────────────────────────────────────

extern "C" fn read_event(
    _args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 0 {
        return TaidaAddonStatus::ArityMismatch;
    }
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    #[cfg(unix)]
    {
        event::read_event_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(windows)]
    {
        windows::read_event_impl(host_ptr, args_len, out_value, out_error)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = out_value;
        let _ = out_error;
        TaidaAddonStatus::Error
    }
}

// ── write (TMB-016) ────────────────────────────────────────────

extern "C" fn write_entry(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    // Arity + host checks happen inside `write_impl` so the same
    // contract is applied on both platforms and the unit tests in
    // `write.rs` can drive the dispatcher directly.
    let host_ptr = HOST_PTR.load(Ordering::Acquire);

    #[cfg(any(unix, windows))]
    {
        write::write_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

// ── Renderer entries (Phase 8 / TMB-020) ───────────────────────
//
// All renderer entries are platform-shared: they perform pure value
// computation on `ScreenBuffer` / `Cell` / `DiffOp` packs. No
// syscalls, no signals, no termios. The split into
// `buffer_*` (mutation) and `render_*` / `buffer_diff` (read-only)
// matches the module split in `src/renderer/`.

extern "C" fn buffer_put(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::ops::buffer_put_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

extern "C" fn buffer_write(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::ops::buffer_write_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

extern "C" fn buffer_fill_rect(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::ops::buffer_fill_rect_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

extern "C" fn buffer_clear(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::ops::buffer_clear_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

extern "C" fn buffer_diff(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::diff::buffer_diff_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

extern "C" fn render_full(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::diff::render_full_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

extern "C" fn render_frame(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::diff::render_frame_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

extern "C" fn render_ops(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::diff::render_ops_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

// ── BufferBlit (Phase 9 / TMB-022) ────────────────────────────────

extern "C" fn buffer_blit(
    args_ptr: *const TaidaAddonValueV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    let host_ptr = HOST_PTR.load(Ordering::Acquire);
    #[cfg(any(unix, windows))]
    {
        renderer::blit::buffer_blit_impl(host_ptr, args_ptr, args_len, out_value, out_error)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (host_ptr, args_ptr, args_len, out_value, out_error);
        TaidaAddonStatus::Error
    }
}

// ── Function table ───────────────────────────────────────────────

/// Function table for the terminal package.
///
/// Existing entries stay append-only so already-published names keep
/// their original shape.
pub static TERMINAL_FUNCTIONS: &[TaidaAddonFunctionV1] = &[
    TaidaAddonFunctionV1 {
        name: c"terminalSize".as_ptr() as *const c_char,
        arity: 0,
        call: terminal_size,
    },
    TaidaAddonFunctionV1 {
        name: c"readKey".as_ptr() as *const c_char,
        arity: 0,
        call: read_key,
    },
    TaidaAddonFunctionV1 {
        name: c"isTerminal".as_ptr() as *const c_char,
        arity: 1,
        call: is_terminal,
    },
    TaidaAddonFunctionV1 {
        name: c"rawModeEnter".as_ptr() as *const c_char,
        arity: 0,
        call: raw_mode_enter,
    },
    TaidaAddonFunctionV1 {
        name: c"rawModeLeave".as_ptr() as *const c_char,
        arity: 0,
        call: raw_mode_leave,
    },
    TaidaAddonFunctionV1 {
        name: c"readEvent".as_ptr() as *const c_char,
        arity: 0,
        call: read_event,
    },
    TaidaAddonFunctionV1 {
        name: c"write".as_ptr() as *const c_char,
        arity: 1,
        call: write_entry,
    },
    // ── Phase 8 / TMB-020 (append-only, positions 7..=14) ─────
    TaidaAddonFunctionV1 {
        name: c"bufferPut".as_ptr() as *const c_char,
        arity: 4,
        call: buffer_put,
    },
    TaidaAddonFunctionV1 {
        name: c"bufferWrite".as_ptr() as *const c_char,
        arity: 5,
        call: buffer_write,
    },
    TaidaAddonFunctionV1 {
        name: c"bufferFillRect".as_ptr() as *const c_char,
        arity: 6,
        call: buffer_fill_rect,
    },
    TaidaAddonFunctionV1 {
        name: c"bufferClear".as_ptr() as *const c_char,
        arity: 2,
        call: buffer_clear,
    },
    TaidaAddonFunctionV1 {
        name: c"bufferDiff".as_ptr() as *const c_char,
        arity: 2,
        call: buffer_diff,
    },
    TaidaAddonFunctionV1 {
        name: c"renderFull".as_ptr() as *const c_char,
        arity: 1,
        call: render_full,
    },
    TaidaAddonFunctionV1 {
        name: c"renderFrame".as_ptr() as *const c_char,
        arity: 2,
        call: render_frame,
    },
    TaidaAddonFunctionV1 {
        name: c"renderOps".as_ptr() as *const c_char,
        arity: 1,
        call: render_ops,
    },
    // ── Phase 9 / TMB-022 (append-only, position 15) ──────────
    TaidaAddonFunctionV1 {
        name: c"bufferBlit".as_ptr() as *const c_char,
        arity: 4,
        call: buffer_blit,
    },
];

taida_addon::declare_addon! {
    name: "taida-lang/terminal",
    functions: TERMINAL_FUNCTIONS,
    init: terminal_init,
}

/// Bench-only re-exports for `benches/renderer_perf.rs`.
///
/// The criterion harness lives in a separate crate (the bench
/// target) and needs to call `BufferState::write_text` /
/// `render_full` etc. without setting up a real `TaidaHostV1`
/// callback table. This module exposes the **internal** Rust
/// functions (not the FFI entries) so the bench can measure the
/// same hot path the production addon executes after marshalling.
///
/// **Not** part of the user-facing addon ABI.
#[doc(hidden)]
#[cfg(any(unix, windows))]
pub mod renderer_bench_api {
    pub use crate::renderer::blit::__bench::blit_into;
    pub use crate::renderer::diff::__bench::{diff_buffers, render_full, render_ops_to_string};
    pub use crate::renderer::ops::__bench::write_text;
    pub use crate::renderer::state::{BufferState, Cell, CellStyle, DiffOp, diff_kind};

    /// Bench-only re-export of [`BufferState::compute_row_hashes`].
    /// Lets the criterion harness mirror the production invariant
    /// (`parse_buffer` always populates `row_hashes`) after a manual
    /// `cells` mutation in the bench setup.
    pub fn compute_row_hashes(buf: &mut BufferState) {
        buf.compute_row_hashes();
    }
}

/// Test-only re-exports.
///
/// `cargo test --test <name>` compiles integration tests in their own
/// crate, so they cannot reach the private `terminal_init` callback or
/// the function table entries directly. The Native loader uses the
/// `taida_addon_get_v1` cdylib symbol; in-process tests use this
/// module to drive the same handshake without going through `dlsym`.
///
/// This module is **not** part of the user-facing addon ABI — it
/// exists purely so the Phase 2 non-TTY contract can be exercised
/// from `tests/read_key_non_tty.rs` without spawning a real Native
/// loader.
#[doc(hidden)]
pub mod __test_only {
    use super::*;

    /// Re-export the init callback used by `declare_addon!`.
    pub fn init(host: *const TaidaHostV1) -> TaidaAddonStatus {
        super::terminal_init(host)
    }

    /// Borrow the frozen function table.
    pub fn functions() -> &'static [TaidaAddonFunctionV1] {
        super::TERMINAL_FUNCTIONS
    }

    /// TMB-017 probe: install the SIGWINCH self-pipe and return the
    /// observed ordering snapshot for integration tests. Returns
    /// `(pipe_rfd, installed_flag, old_handler_is_non_null)`.
    ///
    /// The invariant pinned by TMB-017 is:
    ///   1. If the install succeeds (`rfd >= 0`), then by the time
    ///      this function returns `OLD_SIGWINCH` must already be
    ///      published (non-null). It cannot become non-null *after*
    ///      the new handler is installed — that is the race window
    ///      the fix closes.
    ///   2. `SIGWINCH_INSTALLED` must be `true` only after the new
    ///      handler is live, so any fast-path caller observes a
    ///      fully-published state.
    #[cfg(unix)]
    pub fn sigwinch_install_snapshot() -> (i32, bool, bool) {
        super::event::__test_only_sigwinch_snapshot()
    }

    /// TMB-017 review follow-up: **pure** probe of the SIGWINCH install
    /// globals without triggering install as a side effect.
    ///
    /// `sigwinch_install_snapshot()` above invokes `ensure_sigwinch_pipe()`
    /// and therefore installs the addon handler *on observation*, which
    /// destroys the very state integration tests of the "external
    /// handler → addon install → chain on SIGWINCH delivery" path need
    /// to assert their pre-condition. This pure probe loads the
    /// atomics only (no syscall, no install) so tests can answer
    /// "is the addon already installed?" safely before deciding to
    /// pre-install their own external handler.
    ///
    /// Returns `(installed_flag, old_handler_non_null)`.
    #[cfg(unix)]
    pub fn sigwinch_pure_probe() -> (bool, bool) {
        super::event::__test_only_sigwinch_pure_probe()
    }
}

// ── Unit tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::CStr;
    use taida_addon::{TAIDA_ADDON_ABI_VERSION, TaidaAddonDescriptorV1};

    unsafe extern "C" {
        fn taida_addon_get_v1() -> *const TaidaAddonDescriptorV1;
    }

    #[test]
    fn entry_symbol_returns_descriptor() {
        let ptr = unsafe { taida_addon_get_v1() };
        assert!(!ptr.is_null());
        let d = unsafe { &*ptr };
        assert_eq!(d.abi_version, TAIDA_ADDON_ABI_VERSION);
    }

    #[test]
    fn descriptor_advertises_sixteen_functions() {
        // v1 lock (3) + Phase 2 (rawModeEnter/Leave = 2) + Phase 3
        // (readEvent = 1) + TMB-016 (write = 1) + Phase 8 / TMB-020
        // (bufferPut, bufferWrite, bufferFillRect, bufferClear,
        // bufferDiff, renderFull, renderFrame, renderOps = 8) +
        // Phase 9 / TMB-022 (bufferBlit = 1) = 16.
        // Adding an entry is append-only and bumps this count by one.
        let ptr = unsafe { taida_addon_get_v1() };
        let d = unsafe { &*ptr };
        assert_eq!(d.function_count as usize, TERMINAL_FUNCTIONS.len());
        assert_eq!(d.function_count, 16);
    }

    #[test]
    fn descriptor_addon_name_is_terminal() {
        let ptr = unsafe { taida_addon_get_v1() };
        let d = unsafe { &*ptr };
        let name = unsafe { CStr::from_ptr(d.addon_name) };
        assert_eq!(name.to_str().unwrap(), "taida-lang/terminal");
    }

    #[test]
    fn function_table_v1_entries_are_stable() {
        // The first three entries are the v1 lock: position, name, and
        // arity must never change. New entries are appended after them.
        let v1_expected: Vec<(String, u32)> = vec![
            ("terminalSize".to_string(), 0u32),
            ("readKey".to_string(), 0),
            ("isTerminal".to_string(), 1),
        ];
        let ptr = unsafe { taida_addon_get_v1() };
        let d = unsafe { &*ptr };
        let mut seen = Vec::new();
        for i in 0..d.function_count as isize {
            let f = unsafe { &*d.functions.offset(i) };
            let name = unsafe { CStr::from_ptr(f.name) }.to_str().unwrap();
            seen.push((name.to_string(), f.arity));
        }
        // v1 entries must be at the same positions.
        assert_eq!(&seen[..3], &v1_expected[..]);
        // Full table includes v1 + Phase 2 + Phase 3 + TMB-016 +
        // Phase 8 / TMB-020 (8 entries appended) + Phase 9 / TMB-022
        // (bufferBlit appended at position 15).
        let full_expected: Vec<(String, u32)> = vec![
            ("terminalSize".to_string(), 0u32),
            ("readKey".to_string(), 0),
            ("isTerminal".to_string(), 1),
            ("rawModeEnter".to_string(), 0),
            ("rawModeLeave".to_string(), 0),
            ("readEvent".to_string(), 0),
            ("write".to_string(), 1),
            ("bufferPut".to_string(), 4),
            ("bufferWrite".to_string(), 5),
            ("bufferFillRect".to_string(), 6),
            ("bufferClear".to_string(), 2),
            ("bufferDiff".to_string(), 2),
            ("renderFull".to_string(), 1),
            ("renderFrame".to_string(), 2),
            ("renderOps".to_string(), 1),
            ("bufferBlit".to_string(), 4),
        ];
        assert_eq!(seen, full_expected);
    }

    #[test]
    fn terminal_size_returns_invalid_state_when_host_not_initialised() {
        // Phase 3: terminal_size forwards to size::terminal_size_impl
        // after the arity check. With no `terminal_init` call (and
        // therefore no captured host pointer), the implementation
        // must report InvalidState rather than dereference a null
        // host. The deeper non-TTY / ioctl behaviour is exercised by
        // the unit tests in `size.rs` which drive `probe_terminal_size`
        // directly.
        let f = &TERMINAL_FUNCTIONS[0];
        // Snapshot and clear the global so a previous test that
        // captured a real host pointer can't perturb us.
        let prev = HOST_PTR.swap(core::ptr::null_mut(), Ordering::AcqRel);
        let status = (f.call)(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        // Restore for other tests that might rely on it.
        HOST_PTR.store(prev, Ordering::Release);
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn read_key_returns_invalid_state_when_host_not_initialised() {
        // Phase 2: read_key forwards to key::read_key_impl after the
        // arity check. With no `terminal_init` call (and therefore no
        // captured host pointer), the implementation must report
        // InvalidState rather than dereference a null host. The
        // deeper raw-mode / non-TTY behaviour is exercised by the
        // unit tests in `key.rs` which can drive a real pty.
        let f = &TERMINAL_FUNCTIONS[1];
        // Snapshot and clear the global so a previous test that
        // captured a real host pointer can't perturb us.
        let prev = HOST_PTR.swap(core::ptr::null_mut(), Ordering::AcqRel);
        let status = (f.call)(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        // Restore for other tests that might rely on it.
        HOST_PTR.store(prev, Ordering::Release);
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn is_terminal_returns_invalid_state_when_host_not_initialised() {
        let f = &TERMINAL_FUNCTIONS[2];
        let prev = HOST_PTR.swap(core::ptr::null_mut(), Ordering::AcqRel);
        let status = (f.call)(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        HOST_PTR.store(prev, Ordering::Release);
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn terminal_size_arity_mismatch_when_args_given() {
        let f = &TERMINAL_FUNCTIONS[0];
        let status = (f.call)(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn read_key_arity_mismatch_when_args_given() {
        let f = &TERMINAL_FUNCTIONS[1];
        let status = (f.call)(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn is_terminal_arity_mismatch_when_args_missing() {
        let f = &TERMINAL_FUNCTIONS[2];
        let status = (f.call)(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn raw_mode_enter_returns_invalid_state_when_host_not_initialised() {
        let f = &TERMINAL_FUNCTIONS[3];
        let prev = HOST_PTR.swap(core::ptr::null_mut(), Ordering::AcqRel);
        let status = (f.call)(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        HOST_PTR.store(prev, Ordering::Release);
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn raw_mode_enter_arity_mismatch_when_args_given() {
        let f = &TERMINAL_FUNCTIONS[3];
        let status = (f.call)(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn raw_mode_leave_returns_invalid_state_when_host_not_initialised() {
        let f = &TERMINAL_FUNCTIONS[4];
        let prev = HOST_PTR.swap(core::ptr::null_mut(), Ordering::AcqRel);
        let status = (f.call)(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        HOST_PTR.store(prev, Ordering::Release);
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn raw_mode_leave_arity_mismatch_when_args_given() {
        let f = &TERMINAL_FUNCTIONS[4];
        let status = (f.call)(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn read_event_returns_invalid_state_when_host_not_initialised() {
        let f = &TERMINAL_FUNCTIONS[5];
        let prev = HOST_PTR.swap(core::ptr::null_mut(), Ordering::AcqRel);
        let status = (f.call)(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        HOST_PTR.store(prev, Ordering::Release);
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn read_event_arity_mismatch_when_args_given() {
        let f = &TERMINAL_FUNCTIONS[5];
        let status = (f.call)(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    // ── write dispatcher (TMB-016) ─────────────────────────────

    #[test]
    fn write_entry_is_at_position_six_with_arity_one() {
        // Function table position is part of the append-only contract:
        // TMB-016 reserves position 6 (0-indexed) for `write`. Any new
        // entry must go after this to avoid perturbing downstream tests.
        let f = &TERMINAL_FUNCTIONS[6];
        let name = unsafe { CStr::from_ptr(f.name) }.to_str().unwrap();
        assert_eq!(name, "write");
        assert_eq!(f.arity, 1);
    }

    #[test]
    fn write_returns_invalid_state_when_host_not_initialised() {
        let f = &TERMINAL_FUNCTIONS[6];
        let prev = HOST_PTR.swap(core::ptr::null_mut(), Ordering::AcqRel);
        let status = (f.call)(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        HOST_PTR.store(prev, Ordering::Release);
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn write_arity_mismatch_when_args_missing() {
        let f = &TERMINAL_FUNCTIONS[6];
        let status = (f.call)(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn write_arity_mismatch_when_too_many_args() {
        let f = &TERMINAL_FUNCTIONS[6];
        let status = (f.call)(
            core::ptr::null(),
            3,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    // ── Cross-platform capability error contract (TM-6f) ────────

    /// Error code ranges are frozen and must be identical across platforms.
    /// - ReadKey:       1001-1006
    /// - TerminalSize:  2001-2003
    /// - IsTerminal:    2101-2102
    /// - RawMode:       3001-3005
    /// - ReadEvent:     4001-4007
    /// - Write:         5001-5003 (TMB-016)
    /// - Renderer:      6001-6005 (TMB-020 / Phase 8)
    #[test]
    #[cfg(unix)]
    fn error_code_ranges_are_frozen_unix() {
        use crate::event::err as ee;
        use crate::key::err as ke;
        use crate::raw_mode::err as re;
        use crate::renderer::state::err as rne;
        use crate::size::err as se;
        use crate::tty::err as te;
        use crate::write::err as we;

        // ReadKey error codes
        assert_eq!(ke::READ_KEY_NOT_A_TTY, 1001);
        assert_eq!(ke::READ_KEY_RAW_MODE, 1002);
        assert_eq!(ke::READ_KEY_EOF, 1003);
        assert_eq!(ke::READ_KEY_INTERRUPTED, 1004);
        assert_eq!(ke::READ_KEY_PANIC, 1005);
        assert_eq!(ke::READ_KEY_INVALID_STATE, 1006);

        // TerminalSize error codes
        assert_eq!(se::TERMINAL_SIZE_NOT_A_TTY, 2001);
        assert_eq!(se::TERMINAL_SIZE_IOCTL, 2002);

        // IsTerminal error codes
        assert_eq!(te::IS_TERMINAL_INVALID_STREAM, 2101);
        assert_eq!(te::IS_TERMINAL_BUILD_VALUE, 2102);

        // RawMode error codes
        assert_eq!(re::RAW_MODE_NOT_A_TTY, 3001);
        assert_eq!(re::RAW_MODE_ALREADY_ACTIVE, 3002);
        assert_eq!(re::RAW_MODE_NOT_ACTIVE, 3003);
        assert_eq!(re::RAW_MODE_ENTER_FAILED, 3004);
        assert_eq!(re::RAW_MODE_LEAVE_FAILED, 3005);

        // ReadEvent error codes
        assert_eq!(ee::READ_EVENT_NOT_IN_RAW_MODE, 4001);
        assert_eq!(ee::READ_EVENT_NOT_A_TTY, 4002);
        assert_eq!(ee::READ_EVENT_READ_FAILED, 4003);
        assert_eq!(ee::READ_EVENT_EOF, 4004);
        assert_eq!(ee::READ_EVENT_INTERRUPTED, 4005);
        assert_eq!(ee::READ_EVENT_PANIC, 4006);
        assert_eq!(ee::READ_EVENT_RESIZE_INIT_FAILED, 4007);

        // Write error codes (TMB-016)
        assert_eq!(we::WRITE_FAILED, 5001);
        assert_eq!(we::WRITE_BUILD_VALUE, 5002);
        assert_eq!(we::WRITE_PANIC, 5003);

        // Renderer error codes (TMB-020 / Phase 8)
        assert_eq!(rne::RENDERER_INVALID_ARG, 6001);
        assert_eq!(rne::RENDERER_OUT_OF_BOUNDS, 6002);
        assert_eq!(rne::RENDERER_INVALID_SIZE, 6003);
        assert_eq!(rne::RENDERER_BUILD_VALUE, 6004);
        assert_eq!(rne::RENDERER_PANIC, 6005);
    }

    /// Cross-platform error name contract: the Taida-side error type names
    /// must follow the `{API prefix}{Suffix}` convention documented in
    /// TM_DESIGN.md. This test locks the mapping.
    #[test]
    fn error_name_convention_lock() {
        // These are the exact Taida-side error type names returned by the
        // addon. Adding or renaming requires updating TM_DESIGN.md.
        let expected = [
            // IsTerminal
            "IsTerminalInvalidStream",
            "IsTerminalBuildValue",
            // TerminalSize
            "TerminalSizeNotATty",
            "TerminalSizeIoctl",
            // ReadKey
            "ReadKeyNotATty",
            "ReadKeyRawMode",
            "ReadKeyEof",
            "ReadKeyInterrupted",
            "ReadKeyPanic",
            "ReadKeyInvalidState",
            // RawMode
            "RawModeNotATty",
            "RawModeAlreadyActive",
            "RawModeNotActive",
            "RawModeEnterFailed",
            "RawModeLeaveFailed",
            // ReadEvent
            "ReadEventNotInRawMode",
            "ReadEventNotATty",
            "ReadEventReadFailed",
            "ReadEventEof",
            "ReadEventInterrupted",
            "ReadEventPanic",
            "ReadEventResizeInitFailed",
            // Windows-only (capability init)
            "TerminalSizeUnsupported",
            "ReadKeyUnsupported",
            // Write (TMB-016)
            "WriteFailed",
            "WriteBuildValue",
            "WritePanic",
            // Renderer (TMB-020 / Phase 8)
            "RendererInvalidArg",
            "RendererOutOfBounds",
            "RendererInvalidSize",
            "RendererBuildValue",
            "RendererPanic",
        ];
        // Verify no duplicates.
        let mut sorted = expected.to_vec();
        sorted.sort();
        for i in 0..sorted.len() - 1 {
            assert_ne!(
                sorted[i],
                sorted[i + 1],
                "duplicate error name: {}",
                sorted[i]
            );
        }
        // Count is the contract — adding a new error must update this.
        assert_eq!(expected.len(), 32);
    }
}
