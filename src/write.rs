//! `taida-lang/terminal` — `Write[](bytes: Str) → Int` implementation.
//!
//! This module provides the TUI-oriented, unbuffered-at-user-boundary
//! write path for the terminal package. It was introduced by
//! **TMB-016** (`.dev/TM_BLOCKERS.md`) to unblock real TUI applications:
//! the Taida-side `stdout()` builtin always appends `\n` on every push
//! (row-oriented I/O), so ANSI escape streams — cursor moves, partial
//! redraws, sprite placement — cannot be sent through it without
//! corrupting the frame boundary.
//!
//! ## Contract
//!
//! - **Signature**: `Write[](bytes: Str) → Int`
//! - **Behavior**: issue `io::stdout().write_all(bytes) + flush()`; the
//!   returned `Int` is exactly the number of bytes handed to `write_all`
//!   (which, by the `write_all` contract, equals the length of `bytes`
//!   on success).
//! - **No trailing newline, no buffering beyond the std runtime**. The
//!   caller is responsible for frame composition (ANSI escapes,
//!   cursor moves, newlines).
//! - **non-TTY (pipe / redirect) is a success path**, not an error.
//!   ANSI escapes in redirected streams are the caller's responsibility.
//! - **Panics are caught** and mapped to `WritePanic`, matching the
//!   policy used by `readKey` / `readEvent`.
//!
//! ## Error contract
//!
//! Any I/O error from `write_all` / `flush` (EPIPE, EIO, short-write
//! exhaustion, ...) surfaces as a deterministic `WriteFailed`
//! (`WRITE_FAILED = 5001`). Silent fallback is forbidden
//! (`TM_DESIGN.md` Signal / Capability Ownership).
//!
//! ## Layer boundary (`TM_DESIGN.md`)
//!
//! This is Layer A: it performs the actual syscall. The Taida-side
//! facade (`taida/terminal.td`) only aliases this entry under the
//! public name `Write`. ANSI escape composition stays in Layer B
//! (`taida/ansi.td` / `taida/style.td`).

use std::io::{self, Write};

use taida_addon::bridge::{HostValueBuilder, borrow_arg};
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

/// `Write[]()` error codes. Band `5xxx` is new with TMB-016 and does
/// not collide with any of the existing error bands
/// (1xxx ReadKey, 2xxx TerminalSize/IsTerminal, 3xxx RawMode, 4xxx ReadEvent).
pub mod err {
    /// `io::stdout().write_all` or `.flush()` failed.
    pub const WRITE_FAILED: u32 = 5001;
    /// The addon could not allocate the return `Int` via the host.
    pub const WRITE_BUILD_VALUE: u32 = 5002;
    /// A panic escaped the write path. Caught via `catch_unwind` so the
    /// addon never unwinds across the FFI boundary.
    pub const WRITE_PANIC: u32 = 5003;
}

/// Platform-shared implementation body. Takes the raw bytes and performs
/// the actual `write_all + flush`.
///
/// Separated from the addon dispatcher so unit tests can drive the IO
/// path without walking the host builder / FFI glue.
fn write_all_to_stdout(bytes: &[u8]) -> io::Result<usize> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(bytes)?;
    handle.flush()?;
    Ok(bytes.len())
}

/// Addon entry for `Write[](bytes: Str) → Int`.
///
/// Shared across Unix and Windows — the `io::stdout()` path is platform
/// agnostic and the `panic::catch_unwind` barrier is sufficient on both
/// targets. This is why `write.rs` is compiled on all supported
/// platforms rather than being split into `unix` / `windows`.
pub fn write_impl(
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
    let Some(payload) = arg.as_str() else {
        return TaidaAddonStatus::UnsupportedValue;
    };

    // Catch any unexpected panic from `write_all` / lock acquisition so
    // the FFI boundary never unwinds. We deliberately do NOT treat
    // "stdout is a pipe / redirect" as a panic — on Unix a broken pipe
    // surfaces as `io::Error::BrokenPipe` from `write_all` and is
    // reported through `WriteFailed` below.
    let bytes = payload.as_bytes();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| write_all_to_stdout(bytes)));

    match result {
        Ok(Ok(n)) => {
            let value = builder.int(n as i64);
            if value.is_null() {
                let e = builder.error(
                    err::WRITE_BUILD_VALUE,
                    "WriteBuildValue: failed to allocate Int return value",
                );
                if !out_error.is_null() {
                    unsafe { *out_error = e };
                }
                return TaidaAddonStatus::Error;
            }
            if !out_value.is_null() {
                unsafe { *out_value = value };
            }
            TaidaAddonStatus::Ok
        }
        Ok(Err(io_err)) => {
            let e = builder.error(
                err::WRITE_FAILED,
                &format!(
                    "WriteFailed: stdout write_all/flush failed: {} ({:?})",
                    io_err,
                    io_err.kind()
                ),
            );
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
        Err(_panic) => {
            let e = builder.error(
                err::WRITE_PANIC,
                "WritePanic: stdout write path panicked (caught at FFI boundary)",
            );
            if !out_error.is_null() {
                unsafe { *out_error = e };
            }
            TaidaAddonStatus::Error
        }
    }
}

// ── Unit tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_error_codes_are_frozen() {
        // These are part of the cross-platform capability error
        // contract (`TM_DESIGN.md`). Changing them is a breaking
        // change to the addon surface.
        assert_eq!(err::WRITE_FAILED, 5001);
        assert_eq!(err::WRITE_BUILD_VALUE, 5002);
        assert_eq!(err::WRITE_PANIC, 5003);
    }

    #[test]
    fn write_impl_arity_mismatch_when_args_missing() {
        // Arity check must short-circuit before any host callback or
        // syscall happens.
        let status = write_impl(
            core::ptr::null(),
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn write_impl_arity_mismatch_when_too_many_args() {
        let status = write_impl(
            core::ptr::null(),
            core::ptr::null(),
            2,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn write_impl_invalid_state_when_host_null() {
        // With correct arity but a null host pointer, dispatch must
        // report InvalidState rather than dereference.
        let status = write_impl(
            core::ptr::null(),
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn write_all_to_stdout_returns_byte_count_for_empty_payload() {
        // Empty write is a valid no-op: the caller may compose a frame
        // that happens to be 0 bytes (e.g. a conditional redraw). The
        // returned count must be 0 and stdout must not be closed.
        let n = write_all_to_stdout(b"").expect("empty write must succeed");
        assert_eq!(n, 0);
    }

    #[test]
    fn write_all_to_stdout_returns_byte_count_for_ansi_escape() {
        // The primary TMB-016 use case: push an ANSI sequence (no
        // trailing newline) and get back the exact byte count. The
        // `cargo test` stdout is captured by libtest, so this still
        // exercises the real `io::stdout` path without spamming the
        // terminal.
        let payload = b"\x1b[2J\x1b[H"; // clear screen + cursor home
        let n = write_all_to_stdout(payload).expect("ANSI write must succeed");
        assert_eq!(n, payload.len());
        assert_eq!(n, 7);
    }

    #[test]
    fn write_all_to_stdout_handles_utf8_multibyte() {
        // Byte count must reflect the raw UTF-8 byte length, NOT the
        // character / grapheme count. This matches the Str ABI (UTF-8
        // bytes) and is the contract documented on `Write[]()`.
        let payload = "あいう"; // 3 chars × 3 bytes = 9 bytes
        let n = write_all_to_stdout(payload.as_bytes()).expect("UTF-8 write must succeed");
        assert_eq!(n, 9);
        assert_eq!(n, payload.len());
    }
}
