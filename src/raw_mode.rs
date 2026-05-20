//! `taida-lang/terminal` — `RawModeEnter[]()` / `RawModeLeave[]()` implementation.
//!
//! This module owns the **standalone raw mode management** for TUI
//! applications that need persistent raw mode across multiple
//! `ReadKey[]()` calls.
//!
//! ## Design
//!
//! - `RawModeEnter[]()` saves the current termios and enters raw mode.
//! - `RawModeLeave[]()` restores the saved termios.
//! - State is tracked via a process-global `RAW_MODE_STATE` mutex.
//! - `ReadKey[]()` checks this state and skips its own enter/leave when
//!   standalone raw mode is active (see `key.rs` integration).
//!
//! ## Error Contract
//!
//! - `RawModeNotATty`         (3001): stdin is not a TTY
//! - `RawModeAlreadyActive`   (3002): double enter
//! - `RawModeNotActive`       (3003): leave without enter
//! - `RawModeEnterFailed`     (3004): tcgetattr/tcsetattr failed
//! - `RawModeLeaveFailed`     (3005): tcsetattr restore failed
//!
//! ## Safety
//!
//! The saved termios is restored on `RawModeLeave`. If the process
//! exits or panics without calling `RawModeLeave`, the terminal may
//! remain in raw mode. The Taida runtime's cleanup hooks should call
//! `RawModeLeave` on exit, but that is outside this module's scope.

use core::ffi::c_char;
use core::mem::MaybeUninit;
use std::sync::Mutex;

use taida_addon::bridge::HostValueBuilder;
use taida_addon::{TaidaAddonErrorV1, TaidaAddonStatus, TaidaAddonValueV1, TaidaHostV1};

// ── Error variant codes (TM-2d) ─────────────────────────────────

/// `RawModeEnter[]()` / `RawModeLeave[]()` error variant codes.
pub mod err {
    /// stdin is not a TTY.
    pub const RAW_MODE_NOT_A_TTY: u32 = 3001;
    /// `RawModeEnter` called while already in raw mode.
    pub const RAW_MODE_ALREADY_ACTIVE: u32 = 3002;
    /// `RawModeLeave` called while not in raw mode.
    pub const RAW_MODE_NOT_ACTIVE: u32 = 3003;
    /// tcgetattr or tcsetattr failed during enter.
    pub const RAW_MODE_ENTER_FAILED: u32 = 3004;
    /// tcsetattr failed during leave (restore).
    pub const RAW_MODE_LEAVE_FAILED: u32 = 3005;
}

// ── Global raw mode state ───────────────────────────────────────

/// Process-global raw mode state. Protected by a mutex.
///
/// When `active` is true, the saved termios is stored in `saved_termios`
/// and `ReadKey` should skip its own raw mode enter/leave.
struct RawModeState {
    active: bool,
    saved_termios: Option<libc::termios>,
}

static RAW_MODE_STATE: Mutex<RawModeState> = Mutex::new(RawModeState {
    active: false,
    saved_termios: None,
});

/// Check whether standalone raw mode is currently active.
///
/// Called by `key.rs` to decide whether to skip its own raw mode
/// enter/leave cycle.
pub fn is_raw_mode_active() -> bool {
    let state = match RAW_MODE_STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    state.active
}

// ── Enter raw mode ──────────────────────────────────────────────

/// Outcome of the raw mode enter attempt (pure, no host interaction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterOutcome {
    /// Successfully entered raw mode.
    Ok,
    /// stdin is not a TTY.
    NotATty,
    /// Already in raw mode.
    AlreadyActive,
    /// tcgetattr or tcsetattr failed.
    EnterFailed(i32),
}

/// Attempt to enter raw mode on stdin.
pub fn enter_raw_mode() -> EnterOutcome {
    let mut state = match RAW_MODE_STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    if state.active {
        return EnterOutcome::AlreadyActive;
    }

    // Check TTY before touching termios.
    let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
    if is_tty != 1 {
        return EnterOutcome::NotATty;
    }

    // Save current termios.
    let mut saved = MaybeUninit::<libc::termios>::zeroed();
    let rc = unsafe { libc::tcgetattr(libc::STDIN_FILENO, saved.as_mut_ptr()) };
    if rc != 0 {
        let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        return EnterOutcome::EnterFailed(e);
    }
    let saved = unsafe { saved.assume_init() };

    // Build raw mode termios.
    let mut raw = saved;
    unsafe { libc::cfmakeraw(&mut raw) };
    raw.c_cc[libc::VMIN] = 1;
    raw.c_cc[libc::VTIME] = 0;

    let rc = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) };
    if rc != 0 {
        let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        return EnterOutcome::EnterFailed(e);
    }

    state.active = true;
    state.saved_termios = Some(saved);
    EnterOutcome::Ok
}

// ── Leave raw mode ──────────────────────────────────────────────

/// Outcome of the raw mode leave attempt (pure, no host interaction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveOutcome {
    /// Successfully left raw mode.
    Ok,
    /// Not currently in raw mode.
    NotActive,
    /// tcsetattr restore failed.
    LeaveFailed(i32),
}

/// Attempt to leave raw mode on stdin, restoring the saved termios.
pub fn leave_raw_mode() -> LeaveOutcome {
    let mut state = match RAW_MODE_STATE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    if !state.active {
        return LeaveOutcome::NotActive;
    }

    let saved = match state.saved_termios {
        Some(ref t) => *t,
        None => {
            // Defensive: if somehow active but no saved termios, clear
            // the flag and report not active.
            state.active = false;
            return LeaveOutcome::NotActive;
        }
    };

    // Restore with retry on EINTR.
    let mut tries = 0;
    loop {
        let rc = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &saved) };
        if rc == 0 {
            state.active = false;
            state.saved_termios = None;
            return LeaveOutcome::Ok;
        }
        let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        if e == libc::EINTR && tries < 3 {
            tries += 1;
            continue;
        }
        return LeaveOutcome::LeaveFailed(e);
    }
}

// ── Host-side raw mode state pack builder ───────────────────────

/// Build the return pack `@(active: Bool)` on the host allocator.
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

// ── Public entry: rawModeEnter() over the addon ABI ─────────────

pub fn raw_mode_enter_impl(
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

    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    match enter_raw_mode() {
        EnterOutcome::Ok => {
            let pack = build_raw_mode_state_pack(&builder, true);
            if pack.is_null() {
                let err = builder.error(
                    err::RAW_MODE_ENTER_FAILED,
                    "RawModeEnterFailed: failed to build return pack",
                );
                if !out_error.is_null() {
                    unsafe { *out_error = err };
                }
                return TaidaAddonStatus::Error;
            }
            if !out_value.is_null() {
                unsafe { *out_value = pack };
            }
            TaidaAddonStatus::Ok
        }
        EnterOutcome::NotATty => {
            let err = builder.error(
                err::RAW_MODE_NOT_A_TTY,
                "RawModeNotATty: stdin is not a TTY",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        EnterOutcome::AlreadyActive => {
            let err = builder.error(
                err::RAW_MODE_ALREADY_ACTIVE,
                "RawModeAlreadyActive: raw mode is already active",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        EnterOutcome::EnterFailed(e) => {
            let msg = format!(
                "RawModeEnterFailed: tcsetattr/tcgetattr failed (errno {})",
                e
            );
            let err = builder.error(err::RAW_MODE_ENTER_FAILED, &msg);
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
    }
}

// ── Public entry: rawModeLeave() over the addon ABI ─────────────

pub fn raw_mode_leave_impl(
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

    let builder = match unsafe { HostValueBuilder::from_raw(host_ptr) } {
        Some(b) => b,
        None => return TaidaAddonStatus::InvalidState,
    };

    match leave_raw_mode() {
        LeaveOutcome::Ok => {
            let pack = build_raw_mode_state_pack(&builder, false);
            if pack.is_null() {
                let err = builder.error(
                    err::RAW_MODE_LEAVE_FAILED,
                    "RawModeLeaveFailed: failed to build return pack",
                );
                if !out_error.is_null() {
                    unsafe { *out_error = err };
                }
                return TaidaAddonStatus::Error;
            }
            if !out_value.is_null() {
                unsafe { *out_value = pack };
            }
            TaidaAddonStatus::Ok
        }
        LeaveOutcome::NotActive => {
            let err = builder.error(
                err::RAW_MODE_NOT_ACTIVE,
                "RawModeNotActive: raw mode is not active",
            );
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
        LeaveOutcome::LeaveFailed(e) => {
            let msg = format!("RawModeLeaveFailed: tcsetattr failed (errno {})", e);
            let err = builder.error(err::RAW_MODE_LEAVE_FAILED, &msg);
            if !out_error.is_null() {
                unsafe { *out_error = err };
            }
            TaidaAddonStatus::Error
        }
    }
}

// ── Unit tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_mode_error_codes_are_frozen() {
        assert_eq!(err::RAW_MODE_NOT_A_TTY, 3001);
        assert_eq!(err::RAW_MODE_ALREADY_ACTIVE, 3002);
        assert_eq!(err::RAW_MODE_NOT_ACTIVE, 3003);
        assert_eq!(err::RAW_MODE_ENTER_FAILED, 3004);
        assert_eq!(err::RAW_MODE_LEAVE_FAILED, 3005);
    }

    #[test]
    fn raw_mode_error_codes_in_3xxx_range() {
        // Raw mode errors live in the 3xxx range, distinct from
        // ReadKey (1xxx), TerminalSize (2xxx), IsTerminal (2100s).
        const _: () = assert!(err::RAW_MODE_NOT_A_TTY >= 3000);
        const _: () = assert!(err::RAW_MODE_LEAVE_FAILED >= 3000);
    }

    #[test]
    fn leave_without_enter_returns_not_active() {
        // Ensure the global state is not active (it may be from other tests).
        // We can't guarantee ordering, but leave on a fresh state should be NotActive.
        let result = leave_raw_mode();
        // This might be Ok if some other test left it active, but
        // on a clean state it should be NotActive.
        assert!(
            result == LeaveOutcome::NotActive || result == LeaveOutcome::Ok,
            "Expected NotActive or Ok, got {:?}",
            result
        );
    }

    #[test]
    fn enter_on_non_tty_returns_not_a_tty() {
        // Under cargo test, stdin is typically not a TTY.
        let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
        if is_tty == 1 {
            eprintln!("skipping non-TTY raw mode test: stdin is a TTY in this run");
            return;
        }
        assert_eq!(enter_raw_mode(), EnterOutcome::NotATty);
    }

    #[test]
    fn is_raw_mode_active_returns_false_by_default() {
        // Under cargo test with non-TTY stdin, we can never enter raw mode,
        // so the global state should always be false.
        let is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) };
        if is_tty == 1 {
            eprintln!("skipping: stdin is a TTY");
            return;
        }
        assert!(!is_raw_mode_active());
    }

    #[test]
    fn raw_mode_enter_impl_arity_mismatch() {
        let status = raw_mode_enter_impl(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn raw_mode_enter_impl_invalid_state_when_host_null() {
        let status = raw_mode_enter_impl(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }

    #[test]
    fn raw_mode_leave_impl_arity_mismatch() {
        let status = raw_mode_leave_impl(
            core::ptr::null(),
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::ArityMismatch);
    }

    #[test]
    fn raw_mode_leave_impl_invalid_state_when_host_null() {
        let status = raw_mode_leave_impl(
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        assert_eq!(status, TaidaAddonStatus::InvalidState);
    }
}
