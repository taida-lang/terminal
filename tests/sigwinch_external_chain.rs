//! TMB-017 review follow-up (2026-04-21): strong-path integration
//! test for SIGWINCH handler chain.
//!
//! ── Why this lives in its own file ──────────────────────────────
//!
//! cargo compiles each `tests/*.rs` file into a separate integration
//! test binary. Tests inside the same binary share a process and
//! therefore share SIGWINCH disposition: once any test calls
//! `ensure_sigwinch_pipe()` (directly or via a probe), the addon
//! handler is installed process-wide for the rest of that binary's
//! lifetime, and a subsequent test cannot observe or set up the
//! "no addon installed yet" precondition required for the strong
//! path below.
//!
//! Moving this test into its own binary guarantees it runs in a
//! fresh process with `SIGWINCH_INSTALLED == false` and a default
//! SIGWINCH disposition. That makes the "external handler first →
//! addon install over it → SIGWINCH delivery → chain assertion"
//! sequence reliably reachable on success environments.
//!
//! ── What this test pins ─────────────────────────────────────────
//!
//! This is the **strong path** TMB-017 requires:
//!
//!   1. Assert no addon handler is installed yet (pure probe).
//!   2. Install an external SIGWINCH handler that bumps a counter —
//!      simulating a second library that registered *before* the
//!      addon.
//!   3. Trigger the addon's `ensure_sigwinch_pipe()` via the
//!      side-effectful install snapshot.
//!   4. Send SIGWINCH to the process.
//!   5. Verify **both** happen at the same time:
//!      - the addon's self-pipe receives a byte (our handler fired),
//!      - the external counter increments (the chain to the
//!        previous handler actually ran).
//!
//! Failure of either assertion means TMB-007's chain contract is
//! broken — either we never reached our own handler (addon install
//! failed), or `OLD_SIGWINCH` was null/wrong at the moment of
//! delivery (install-order race, TMB-017's core regression).
//!
//! ── Skip policy ─────────────────────────────────────────────────
//!
//! On success environments (Linux / macOS with working `sigaction`
//! and `pipe`), this test is never silently skipped: the pure-probe
//! precondition is asserted hard, and any install or delivery
//! failure is surfaced as an explicit assertion failure.
//!
//! The only legitimate skip branch is "pipe(2) / sigaction failed
//! entirely" (heavily sandboxed CI), which matches the policy of
//! the existing `sigwinch_pipe_can_be_installed` unit test. Even
//! in that branch the skip is loud (eprintln) so it never hides
//! behind a silent pass.

#![cfg(unix)]

use std::sync::atomic::{AtomicU32, Ordering};
use taida_lang_terminal::__test_only;

// External SIGWINCH counter. `extern "C" fn` handler bumps this via
// atomic ops to keep the signal-handler body async-signal-safe.
static EXTERNAL_COUNT: AtomicU32 = AtomicU32::new(0);

extern "C" fn external_sigwinch_handler(_sig: i32) {
    EXTERNAL_COUNT.fetch_add(1, Ordering::Relaxed);
}

#[test]
fn external_handler_is_chained_on_real_sigwinch_delivery() {
    // ── Step 1: pure-probe precondition ─────────────────────────
    //
    // This binary must start with the addon uninstalled. If another
    // test in this file ever runs before this one, this assertion
    // will catch it — we want the strong path to be unambiguously
    // the "first install" case.
    let (pre_installed, pre_old) = __test_only::sigwinch_pure_probe();
    assert!(
        !pre_installed,
        "TMB-017 strong path: addon SIGWINCH handler must not be \
         installed yet at test entry (got installed={}, old_non_null={}). \
         If this trips, a prior call in this binary invoked \
         ensure_sigwinch_pipe() — the strong path cannot run.",
        pre_installed, pre_old
    );
    assert!(
        !pre_old,
        "TMB-017 strong path: OLD_SIGWINCH must be null at test entry \
         (no prior install published it)"
    );

    // ── Step 2: install the external handler FIRST ───────────────
    //
    // This simulates another library (e.g. ncurses, a TUI helper)
    // that registered a SIGWINCH handler before our addon was
    // loaded. The TMB-007 + TMB-017 contract says the addon must
    // capture this handler as OLD_SIGWINCH and chain to it on every
    // SIGWINCH delivery.
    let mut sa: libc::sigaction = unsafe { core::mem::zeroed() };
    sa.sa_sigaction = external_sigwinch_handler as *const () as usize;
    sa.sa_flags = libc::SA_RESTART;
    unsafe { libc::sigemptyset(&mut sa.sa_mask) };
    let rc = unsafe { libc::sigaction(libc::SIGWINCH, &sa, core::ptr::null_mut()) };
    if rc != 0 {
        // Sandboxed CI cannot install signal handlers. Loud skip,
        // not silent pass — the invariant is "either we ran the
        // strong path and it passed, or we surfaced a concrete
        // reason we could not run it".
        eprintln!(
            "skipping TMB-017 strong path: sigaction(SIGWINCH) for external handler failed (rc={}, errno={})",
            rc,
            std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)
        );
        return;
    }

    // Sanity: pure probe still reports "not installed by addon" —
    // installing an external handler must not flip the addon flag.
    let (mid_installed, _) = __test_only::sigwinch_pure_probe();
    assert!(
        !mid_installed,
        "TMB-017 strong path: external sigaction must not affect \
         addon's SIGWINCH_INSTALLED flag"
    );

    // ── Step 3: let the addon install over the external handler ─
    //
    // This is the moment the 2-step install order (query old →
    // publish OLD_SIGWINCH → install new) matters. If the order
    // regressed, a SIGWINCH delivered *between* install and
    // publish would fire our handler with OLD_SIGWINCH=null and
    // silently drop the chain.
    let snap = __test_only::sigwinch_install_snapshot();
    if snap.0 < 0 {
        eprintln!("skipping TMB-017 strong path: addon ensure_sigwinch_pipe failed (pipe install)");
        return;
    }
    assert!(
        snap.1,
        "TMB-017 strong path: addon must report SIGWINCH_INSTALLED=true"
    );
    assert!(
        snap.2,
        "TMB-017 strong path: OLD_SIGWINCH must be non-null — the addon \
         should have captured our external handler as the chain target"
    );

    // ── Step 4: deliver a real SIGWINCH ──────────────────────────
    //
    // Reset the external counter so we measure only the delivery
    // triggered below (not any stray pre-install SIGWINCH).
    EXTERNAL_COUNT.store(0, Ordering::SeqCst);
    let kill_rc = unsafe { libc::kill(libc::getpid(), libc::SIGWINCH) };
    assert_eq!(
        kill_rc, 0,
        "TMB-017 strong path: kill(getpid, SIGWINCH) must succeed"
    );

    // Give the signal a moment to propagate. 50ms is generous for
    // a self-delivered signal on any loaded CI.
    std::thread::sleep(std::time::Duration::from_millis(50));

    // ── Step 5a: addon self-pipe must have received a byte ──────
    //
    // This proves our handler fired. If the addon install had
    // failed silently, this would read 0 bytes and we'd know.
    let mut buf = [0u8; 16];
    let n = unsafe { libc::read(snap.0, buf.as_mut_ptr() as *mut _, buf.len()) };
    assert!(
        n > 0,
        "TMB-017 strong path: addon self-pipe must have at least one \
         byte after SIGWINCH (got n={}, errno={}) — our handler did \
         not fire",
        n,
        std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)
    );

    // ── Step 5b: external counter must also have incremented ────
    //
    // This is the core TMB-007 / TMB-017 invariant: the chain
    // target was correctly published *before* our handler went
    // live, so when SIGWINCH fired the handler successfully
    // chained to the pre-existing external handler.
    let ext = EXTERNAL_COUNT.load(Ordering::SeqCst);
    assert!(
        ext > 0,
        "TMB-017 strong path: external SIGWINCH handler must have \
         been chained — OLD_SIGWINCH was published before addon \
         install so the external handler should fire on every \
         SIGWINCH delivery (got count = {})",
        ext
    );
}
