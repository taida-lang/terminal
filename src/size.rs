//! `taida-lang/terminal` — `TerminalSize[]()` implementation.
//!
//! This module owns the **size query contract** mandated by
//! `RC2_DESIGN.md` Section B-1 / D and `RC2_BLOCKERS.md` RC2B-203.
//!
//! ## Invariants
//!
//! 1. Return pack is `@(cols: Int, rows: Int)` with field order locked
//!    as `cols` → `rows`. Renaming, reordering, or adding fields
//!    requires an ABI bump.
//! 2. `cols` and `rows` are **always ≥ 1** on success. If either
//!    `ws_col == 0` or `ws_row == 0`, the call must fail with
//!    `TerminalSizeIoctl` rather than surface a zero value.
//! 3. **Silent fallback is forbidden.** `(80, 24)` and any other
//!    "sensible default" are explicitly disallowed by RC2B-203. Every
//!    failure path must return a deterministic error variant.
//! 4. Non-TTY stdout is detected **before** the ioctl syscall via
//!    `isatty(STDOUT_FILENO)`. Pipes and redirected stdout map to
//!    `TerminalSizeNotATty`, not to a successful response.
//! 5. ioctl / syscall failure (including the `ws_col == 0` /
//!    `ws_row == 0` degenerate success case) maps to
//!    `TerminalSizeIoctl`.
//!
//! ## Error variant codes
//!
//! Wire format: `u32` carried in `TaidaAddonErrorV1::code`. The host
//! surfaces them as deterministic variant names on the Taida side.
//! Renumbering requires an ABI bump (RC2 v1 lock).
//!
//! The `#[cfg(unix)]` gate lives on the `mod size;` declaration in
//! `lib.rs`; we don't shadow it with a redundant inner attribute.

use core::ffi::c_char;
use core::mem::MaybeUninit;

use taida_addon::bridge::HostValueBuilder;
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

// ── Error variant codes (v1 lock) ────────────────────────────────
//
// Numeric codes are part of the surface and pinned by the unit test
// `terminal_size_error_codes_are_frozen_v1` below. Renumbering
// requires an ABI bump. These live in their own range (2xxx) to keep
// them distinct from the `ReadKey[]()` codes (1xxx) in `key::err`.

/// `TerminalSize[]()` error variant codes (Section D of RC2_DESIGN.md).
pub mod err {
    /// Stdout is not a TTY (pipe / file / redirected).
    pub const TERMINAL_SIZE_NOT_A_TTY: u32 = 2001;
    /// ioctl(TIOCGWINSZ) failed, or returned a degenerate
    /// `ws_col == 0 || ws_row == 0` response.
    pub const TERMINAL_SIZE_IOCTL: u32 = 2002;
}

// ── Outcome enum (pure; no host interaction) ─────────────────────
//
// Split out so unit tests can exercise the decision tree without
// needing a host capability table. The public entry point translates
// this into the host-side error / value build.

/// Result of probing the terminal size. The success variant carries
/// `(cols, rows)` as the already-validated `(>= 1, >= 1)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeOutcome {
    /// `(cols, rows)`, both guaranteed ≥ 1.
    Ok { cols: i64, rows: i64 },
    /// stdout is not a TTY.
    NotATty,
    /// ioctl failed; `errno` captured for the error message.
    IoctlFailed(i32),
    /// ioctl returned 0 rows/cols — treated as failure per RC2B-203.
    DegenerateZero { ws_col: u16, ws_row: u16 },
}

// ── The syscall path ─────────────────────────────────────────────

/// Probe the terminal size via `isatty` + `ioctl(TIOCGWINSZ)`.
///
/// Pure in the sense that it has no dependency on the host callback
/// table — callers translate the `SizeOutcome` into a host-side value
/// or error pack.
///
/// ## Contract
///
/// - stdout must be a TTY; otherwise returns `NotATty`
/// - `ioctl` syscall failure → `IoctlFailed(errno)`
/// - `ws_col == 0 || ws_row == 0` → `DegenerateZero` (silent fallback
///   to `(80, 24)` is forbidden)
/// - Success → `Ok { cols, rows }` with both fields ≥ 1
pub fn probe_terminal_size() -> SizeOutcome {
    // Step 1: non-TTY detection **before** the ioctl. The design says
    // the only permitted path is stdout-is-a-TTY. Pipes, files, and
    // redirections must map to a deterministic `NotATty` error — no
    // silent fallback and no "try ioctl anyway to see if the kernel
    // guesses something sensible".
    //
    // SAFETY: libc::isatty is a pure syscall that inspects STDOUT_FILENO;
    // it does not retain any state.
    let is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) };
    if is_tty != 1 {
        return SizeOutcome::NotATty;
    }

    // Step 2: ioctl(TIOCGWINSZ). The kernel fills a `winsize` struct
    // with `ws_row`, `ws_col`, `ws_xpixel`, `ws_ypixel` — we only
    // consume the first two.
    let mut ws = MaybeUninit::<libc::winsize>::zeroed();
    // SAFETY: ioctl with TIOCGWINSZ expects a `*mut winsize`. We pass
    // a freshly-zeroed (but correctly aligned / sized) slot; the
    // kernel fully populates it on success and we only read from it
    // after checking the return code.
    let rc = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
    if rc != 0 {
        // RC2.6B-022: use std::io::Error for cross-platform errno
        // retrieval (__errno_location is Linux glibc only; macOS
        // uses __error, etc.).
        let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        return SizeOutcome::IoctlFailed(e);
    }
    // SAFETY: ioctl succeeded → kernel populated `ws` fully.
    let ws = unsafe { ws.assume_init() };

    // Step 3: reject degenerate zeros. The design explicitly forbids
    // surfacing a `(0, *)` or `(*, 0)` pair as success. Some kernels
    // return 0 for the winsize when the terminal has been resized to
    // nothing (e.g. a detached pty waiting for SIGWINCH); we treat
    // those as an ioctl-level failure rather than fabricate a value.
    if ws.ws_col == 0 || ws.ws_row == 0 {
        return SizeOutcome::DegenerateZero {
            ws_col: ws.ws_col,
            ws_row: ws.ws_row,
        };
    }

    SizeOutcome::Ok {
        cols: i64::from(ws.ws_col),
        rows: i64::from(ws.ws_row),
    }
}

// ── Host-side pack builder ───────────────────────────────────────

/// Build the return pack `@(cols: Int, rows: Int)` on the host
/// allocator. Returns a null pointer if any sub-value allocation
/// fails; the caller is responsible for surfacing a deterministic
/// error in that case.
fn build_pack(builder: &HostValueBuilder<'_>, cols: i64, rows: i64) -> *mut TaidaAddonValueV1 {
    let host = builder.as_raw();
    // SAFETY: `host` is the validated pointer captured from
    // `terminal_init`; the callback table is non-null (checked by
    // `HostValueBuilder::from_raw`).
    let cols_v = unsafe { ((*host).value_new_int)(host, cols) };
    let rows_v = unsafe { ((*host).value_new_int)(host, rows) };

    if cols_v.is_null() || rows_v.is_null() {
        for v in [cols_v, rows_v] {
            if !v.is_null() {
                // SAFETY: v was just allocated via value_new_int on
                // this host and hasn't been handed to anyone else.
                unsafe { ((*host).value_release)(host, v) };
            }
        }
        return core::ptr::null_mut();
    }

    // Field order is part of the v1 lock: cols, then rows.
    let cols_name = c"cols";
    let rows_name = c"rows";
    let names: [*const c_char; 2] = [cols_name.as_ptr(), rows_name.as_ptr()];
    let values: [*mut TaidaAddonValueV1; 2] = [cols_v, rows_v];
    crate::pack_util::pack_or_release(builder, &names, &values)
}

// ── Public entry: terminal_size() over the addon ABI ─────────────

/// Implementation backing the addon `terminalSize` entry point. The C
/// ABI wrapper in `lib.rs` forwards `args_len` / `out_value` /
/// `out_error` directly to here after the host has captured the host
/// pointer via `terminal_init`.
///
/// ## Returned status
///
/// - `Ok` — `out_value` carries a freshly allocated
///   `@(cols: Int, rows: Int)` pack with both fields ≥ 1
/// - `ArityMismatch` — caller passed args (function arity is 0)
/// - `InvalidState` — `host_ptr` is null (init not yet called)
/// - `Error` — one of the deterministic variants below was written to
///   `out_error`:
///   * `TerminalSizeNotATty` (code 2001) — stdout is not a TTY
///   * `TerminalSizeIoctl`   (code 2002) — ioctl failed or returned
///     a degenerate `ws_col == 0 || ws_row == 0` pair
pub fn terminal_size_impl(
    host_ptr: *const TaidaHostV1,
    args_len: u32,
    out_value: *mut *mut TaidaAddonValueV1,
    out_error: *mut *mut TaidaAddonErrorV1,
) -> TaidaAddonStatus {
    if args_len != 0 {
        return TaidaAddonStatus::ArityMismatch;
    }
    if host_ptr.is_null() {
        return TaidaAddonStatus::InvalidState;
    }

    // Build the host wrapper up front. Unlike `read_key` this path
    // does not touch global mutable state (no termios, no inflight
    // guard), so we don't need `catch_unwind` — the entire body is
    // made of infallible pointer arithmetic + two syscalls. Any
    // panic inside a host callback is the host's bug, not ours.
    //
    // SAFETY: host_ptr was checked non-null above; the caller
    // (lib.rs → terminal_size) re-validated the host pointer captured
    // by `terminal_init`.
    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    match probe_terminal_size() {
        SizeOutcome::Ok { cols, rows } => {
            let pack = build_pack(&builder, cols, rows);
            if pack.is_null() {
                let err = builder.error(
                    err::TERMINAL_SIZE_IOCTL,
                    "TerminalSizeIoctl: failed to build return pack",
                );
                if !out_error.is_null() {
                    // SAFETY: caller's slot is either null or a
                    // writable `*mut *mut TaidaAddonErrorV1`.
                    unsafe { *out_error = err };
                }
                return TaidaAddonStatus::Error;
            }
            if !out_value.is_null() {
                // SAFETY: caller's slot is a writable
                // `*mut *mut TaidaAddonValueV1`.
                unsafe { *out_value = pack };
            }
            TaidaAddonStatus::Ok
        }
        SizeOutcome::NotATty => {
            let err = builder.error(
                err::TERMINAL_SIZE_NOT_A_TTY,
                "TerminalSizeNotATty: stdout is not a TTY",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        SizeOutcome::IoctlFailed(e) => {
            let msg = format!("TerminalSizeIoctl: ioctl(TIOCGWINSZ) failed (errno {})", e);
            let err = builder.error(err::TERMINAL_SIZE_IOCTL, &msg);
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        SizeOutcome::DegenerateZero { ws_col, ws_row } => {
            let msg = format!(
                "TerminalSizeIoctl: kernel returned ws_col={} ws_row={} \
                 (zero rejected, no (80, 24) fallback)",
                ws_col, ws_row
            );
            let err = builder.error(err::TERMINAL_SIZE_IOCTL, &msg);
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
    }
}

// ── Unit tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Error code lock ──────────────────────────────────────

    #[test]
    fn terminal_size_error_codes_are_frozen_v1() {
        // Renumbering these requires an ABI bump. The host surfaces
        // the numeric code as a deterministic variant name, so
        // flipping the wire value without a bump would silently
        // break host-side matching.
        assert_eq!(err::TERMINAL_SIZE_NOT_A_TTY, 2001);
        assert_eq!(err::TERMINAL_SIZE_IOCTL, 2002);
        // The two codes must also live in a different range from the
        // ReadKey[]() codes (1xxx) so the host can tell them apart
        // even in log-only contexts where the addon name is lost.
        // These are compile-time checks (the values are `const`), so
        // we use `const { assert!(..) }` to avoid clippy's
        // `assertions_on_constants` lint.
        const _: () = assert!(err::TERMINAL_SIZE_NOT_A_TTY >= 2000);
        const _: () = assert!(err::TERMINAL_SIZE_IOCTL >= 2000);
    }

    // ── probe_terminal_size under cargo test ─────────────────
    //
    // `cargo test` typically pipes stdout through a test harness, so
    // STDOUT_FILENO is not a TTY. That means every CI / CLI run of
    // the test suite exercises the `NotATty` path deterministically.
    //
    // If a developer somehow runs `cargo test` from an interactive
    // shell with stdout attached to a real terminal, the probe will
    // produce `Ok { cols, rows }` instead — we branch on both
    // outcomes rather than pin one, so the test is stable across
    // environments.

    #[test]
    fn probe_under_cargo_test_yields_deterministic_outcome() {
        let is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) };
        match probe_terminal_size() {
            SizeOutcome::NotATty => {
                assert_ne!(is_tty, 1, "NotATty must only fire when isatty(stdout) != 1");
            }
            SizeOutcome::Ok { cols, rows } => {
                assert_eq!(is_tty, 1, "Ok must only fire when stdout is a TTY");
                assert!(cols >= 1, "cols must be >= 1 on success, got {}", cols);
                assert!(rows >= 1, "rows must be >= 1 on success, got {}", rows);
            }
            SizeOutcome::IoctlFailed(e) => {
                // Legal on exotic pty setups; the error is still a
                // deterministic variant, which is all the contract
                // asks for.
                assert!(e != 0, "IoctlFailed must carry a nonzero errno");
            }
            SizeOutcome::DegenerateZero { ws_col, ws_row } => {
                // Extremely unlikely under `cargo test`, but legal;
                // we still treat it as failure per the design.
                assert!(
                    ws_col == 0 || ws_row == 0,
                    "DegenerateZero must carry a zero dimension"
                );
            }
        }
    }

    #[test]
    fn probe_non_tty_stdout_yields_not_a_tty() {
        // Under `cargo test`, stdout is not a TTY. This test pins
        // the expected outcome. A developer running with stdout
        // attached to a real terminal will see the test skip
        // rather than fail, so the suite stays stable.
        let is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) };
        if is_tty == 1 {
            eprintln!("skipping non-TTY probe test: stdout is a TTY in this run");
            return;
        }
        assert_eq!(
            probe_terminal_size(),
            SizeOutcome::NotATty,
            "probe_terminal_size under a non-TTY stdout must map to NotATty"
        );
    }

    // ── Silent fallback forbidden ────────────────────────────

    #[test]
    fn no_silent_fallback_to_eighty_by_twentyfour() {
        // RC2B-203 root-cause test. The whole point of Phase 3 is
        // that `(80, 24)` is never fabricated as a silent fallback.
        // If this assertion ever needs to relax, RC2B-203 is being
        // reopened.
        //
        // We exercise this structurally: under `cargo test` stdout
        // is a pipe, so `probe_terminal_size` must return an error
        // variant, not a `(80, 24)` success.
        let is_tty = unsafe { libc::isatty(libc::STDOUT_FILENO) };
        if is_tty == 1 {
            eprintln!("skipping silent-fallback probe: stdout is a TTY in this run");
            return;
        }
        match probe_terminal_size() {
            SizeOutcome::Ok { cols, rows } => {
                panic!(
                    "non-TTY stdout must not produce an Ok outcome \
                     (silent fallback forbidden); got cols={} rows={}",
                    cols, rows
                );
            }
            SizeOutcome::NotATty
            | SizeOutcome::IoctlFailed(_)
            | SizeOutcome::DegenerateZero { .. } => {
                // All three are legal "not silently fabricated"
                // outcomes.
            }
        }
    }

    // ── Entry point pre-conditions (no host needed) ─────────

    #[test]
    fn terminal_size_impl_arity_mismatch_when_args_given() {
        // Arity mismatch is short-circuited before we touch any host
        // state, so we can drive it with a null host pointer.
        let status = terminal_size_impl(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn terminal_size_impl_invalid_state_when_host_null() {
        // With zero args but a null host, we must report InvalidState
        // without touching stdout / ioctl.
        let status = terminal_size_impl(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }
}
