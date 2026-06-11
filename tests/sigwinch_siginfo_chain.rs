//! SA_SIGINFO chain regression test.
//!
//! ── What this pins ───────────────────────────────────────────────
//!
//! A library that installed its SIGWINCH handler with `SA_SIGINFO`
//! *before* the addon must keep working after the addon chains over
//! it. Such handlers receive `(sig, info, ucontext)` and are entitled
//! to dereference `info` — kernel delivery guarantees it is non-null.
//!
//! The addon's handler used to chain with `info = null`, so the very
//! first terminal resize crashed the whole process (a Rust
//! predecessor handler hits the null-deref panic → non-unwinding
//! abort; a C one segfaults). The fix installs the addon handler with
//! `SA_SIGINFO` and forwards the kernel's real pointers verbatim.
//!
//! If the fix regresses, this test does not merely fail an assertion:
//! the dereference below crashes the test process, which cargo
//! reports loudly.
//!
//! ── Why this lives in its own file ───────────────────────────────
//!
//! Same reason as `sigwinch_external_chain.rs`: tests in one binary
//! share the process-wide SIGWINCH disposition, and the strong path
//! needs the "external handler installed before the addon" initial
//! state, which is only guaranteed in a fresh process.

#![cfg(unix)]

use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use taida_lang_terminal::__test_only;

static EXTERNAL_COUNT: AtomicU32 = AtomicU32::new(0);
static LAST_SI_SIGNO: AtomicI32 = AtomicI32::new(-1);

extern "C" fn external_siginfo_handler(
    _sig: i32,
    info: *mut libc::siginfo_t,
    _ucontext: *mut libc::c_void,
) {
    // The whole point: dereference `info` like any real SA_SIGINFO
    // consumer (Go runtime, profilers, sanitizers). With the old
    // null-forwarding chain this crashes the process.
    let signo = unsafe { (*info).si_signo };
    LAST_SI_SIGNO.store(signo, Ordering::Relaxed);
    EXTERNAL_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[test]
fn sa_siginfo_external_handler_chains_with_real_siginfo() {
    // ── Step 1: fresh-process precondition ───────────────────────
    let (pre_installed, pre_old) = __test_only::sigwinch_pure_probe();
    assert!(
        !pre_installed && !pre_old,
        "addon SIGWINCH handler must not be installed at test entry \
         (installed={pre_installed}, old_non_null={pre_old})"
    );

    // ── Step 2: install the external SA_SIGINFO handler FIRST ────
    let mut sa: libc::sigaction = unsafe { core::mem::zeroed() };
    sa.sa_sigaction = external_siginfo_handler
        as extern "C" fn(i32, *mut libc::siginfo_t, *mut libc::c_void)
        as usize;
    sa.sa_flags = libc::SA_RESTART | libc::SA_SIGINFO;
    unsafe { libc::sigemptyset(&mut sa.sa_mask) };
    let rc = unsafe { libc::sigaction(libc::SIGWINCH, &sa, core::ptr::null_mut()) };
    if rc != 0 {
        eprintln!(
            "skipping SA_SIGINFO chain test: sigaction failed (rc={rc}, errno={})",
            std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)
        );
        return;
    }

    // ── Step 3: let the addon install over it ────────────────────
    let snap = __test_only::sigwinch_install_snapshot();
    if snap.0 < 0 {
        eprintln!("skipping SA_SIGINFO chain test: addon pipe install failed");
        return;
    }
    assert!(snap.1, "addon must report SIGWINCH_INSTALLED=true");
    assert!(
        snap.2,
        "OLD_SIGWINCH must be non-null — the SA_SIGINFO external \
         handler should have been captured as the chain target"
    );

    // ── Step 4: deliver a real SIGWINCH ──────────────────────────
    EXTERNAL_COUNT.store(0, Ordering::SeqCst);
    LAST_SI_SIGNO.store(-1, Ordering::SeqCst);
    let kill_rc = unsafe { libc::kill(libc::getpid(), libc::SIGWINCH) };
    assert_eq!(kill_rc, 0, "kill(getpid, SIGWINCH) must succeed");
    std::thread::sleep(std::time::Duration::from_millis(50));

    // ── Step 5a: addon self-pipe received a byte ─────────────────
    let mut buf = [0u8; 16];
    let n = unsafe { libc::read(snap.0, buf.as_mut_ptr() as *mut _, buf.len()) };
    assert!(
        n > 0,
        "addon self-pipe must have a byte after SIGWINCH (n={n}) — \
         the addon handler did not fire"
    );

    // ── Step 5b: the external handler ran AND saw real siginfo ──
    let ext = EXTERNAL_COUNT.load(Ordering::SeqCst);
    assert!(
        ext > 0,
        "external SA_SIGINFO handler must have been chained (count={ext})"
    );
    let signo = LAST_SI_SIGNO.load(Ordering::SeqCst);
    assert_eq!(
        signo,
        libc::SIGWINCH,
        "chained handler must receive the kernel's real siginfo — \
         si_signo should be SIGWINCH, got {signo}"
    );
}
