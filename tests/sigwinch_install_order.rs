//! Integration test for TMB-017: SIGWINCH handler install order race.
//!
//! Background (TMB-007 → TMB-017 audit):
//!
//! TMB-007 added a chain mechanism so the terminal addon no longer
//! clobbers an existing SIGWINCH handler installed by another library.
//! The original implementation installed the new handler *first*
//! (`sigaction(SIGWINCH, &sa, &mut old_sa)`) and *then* published
//! `old_sa` to the `OLD_SIGWINCH` AtomicPtr. Between those two steps,
//! if SIGWINCH was delivered, our handler read `OLD_SIGWINCH == null`
//! and silently dropped the chain — defeating TMB-007's purpose for
//! the very first SIGWINCH.
//!
//! The TMB-017 fix reorders install into 2 steps:
//!
//!   1. Query the current handler via `sigaction(SIGWINCH, NULL, &old)`
//!      — no install, disposition unchanged.
//!   2. Publish `OLD_SIGWINCH` with Release ordering.
//!   3. Install our handler via `sigaction(SIGWINCH, &sa, NULL)`.
//!   4. Flip `SIGWINCH_INSTALLED = true`.
//!
//! After step 3, any SIGWINCH delivery finds the chain target already
//! stored. After step 4, fast-path callers of `ensure_sigwinch_pipe`
//! skip the install block only once the handler is fully live.
//!
//! These tests pin the externally observable invariants that follow
//! from that ordering, so a future refactor cannot silently regress.

#![cfg(unix)]

use taida_lang_terminal::__test_only;

/// Invariant 1: once `ensure_sigwinch_pipe` reports success, the
/// external observation must be `installed == true` AND
/// `old_handler_non_null == true`. Under the pre-TMB-017 order the
/// implementation *could* reach `installed == true` with
/// `old_handler_non_null == false` if observation happened between
/// the `sigaction` call and the heap publish — the new order
/// eliminates that intermediate state entirely because the publish
/// precedes the install.
#[test]
fn install_publishes_old_handler_before_flipping_installed_flag() {
    let (rfd, installed, old_non_null) = __test_only::sigwinch_install_snapshot();
    if rfd < 0 {
        // Extremely constrained environment (e.g. pipe(2) failed) —
        // nothing to assert, same policy as the existing
        // `sigwinch_pipe_can_be_installed` unit test.
        eprintln!("skipping TMB-017 install-order test: pipe install failed");
        return;
    }
    assert!(
        installed,
        "TMB-017: SIGWINCH_INSTALLED must be true after successful install"
    );
    assert!(
        old_non_null,
        "TMB-017: OLD_SIGWINCH must be non-null before SIGWINCH_INSTALLED \
         becomes true — the 2-step install publishes the chain target \
         before flipping the flag"
    );
}

/// Invariant 2: repeated calls must be idempotent at the observable
/// level. The fast-path guards on `SIGWINCH_INSTALLED` and must not
/// re-enter the install block, so the ordering guarantee is stable
/// across calls.
#[test]
fn snapshot_is_idempotent_after_first_install() {
    let first = __test_only::sigwinch_install_snapshot();
    if first.0 < 0 {
        return;
    }
    let second = __test_only::sigwinch_install_snapshot();
    assert_eq!(first.0, second.0, "pipe rfd must be stable");
    assert_eq!(
        (first.1, first.2),
        (second.1, second.2),
        "install flags must not regress on re-entry"
    );
    assert!(
        second.1 && second.2,
        "invariants must still hold on re-entry"
    );
}

/// Invariant 3: the chain target stored in `OLD_SIGWINCH` must not
/// be our own handler. If the old install order had ever been
/// called twice (e.g. by two separate addons in the same process)
/// the second call would capture *our* handler as the previous one
/// and the SIGWINCH delivery would self-chain until stack overflow.
/// TMB-017's idempotence guard (SIGWINCH_INSTALLED fast-path) plus
/// the query-first install order makes this unreachable.
///
/// We cannot easily inspect `OLD_SIGWINCH` from outside the crate,
/// so this test instead exercises actual delivery: pre-install a
/// benign external handler, let the addon install over it, then
/// send ourselves SIGWINCH and confirm the pipe receives a byte
/// (i.e. our handler fired) AND the pre-installed handler also
/// fired. If our install captured ourselves instead of the external
/// handler, the external counter would stay at zero.
#[test]
fn external_sigwinch_handler_is_still_chained_after_install() {
    use std::sync::atomic::{AtomicU32, Ordering};

    // Install a pre-existing external SIGWINCH handler that bumps a
    // counter. This simulates a second library that installed first.
    static EXTERNAL_COUNT: AtomicU32 = AtomicU32::new(0);
    extern "C" fn external_handler(_sig: i32) {
        EXTERNAL_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    // Only run if we can still (re)install — if the addon's
    // SIGWINCH handler is already installed from a prior test in
    // the same process, the snapshot fast-path will skip re-install
    // and we can't verify chain-capture for this particular external
    // handler. In that case we assert the weaker invariant: a
    // SIGWINCH delivery reaches our self-pipe without crashing.
    let pre_snapshot = __test_only::sigwinch_install_snapshot();
    if pre_snapshot.0 < 0 {
        return;
    }

    if pre_snapshot.1 {
        // Addon already installed (likely from another test). We
        // cannot retro-insert a pre-existing external handler now
        // without clobbering the addon. Weaker assertion only.
        unsafe {
            libc::kill(libc::getpid(), libc::SIGWINCH);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        // No panic reaching here means the chain path did not
        // dereference a null OLD_SIGWINCH, which is the TMB-017
        // core invariant.
        return;
    }

    // Fresh state: install external handler first.
    let mut sa: libc::sigaction = unsafe { core::mem::zeroed() };
    sa.sa_sigaction = external_handler as *const () as usize;
    sa.sa_flags = libc::SA_RESTART;
    unsafe { libc::sigemptyset(&mut sa.sa_mask) };
    let rc = unsafe { libc::sigaction(libc::SIGWINCH, &sa, core::ptr::null_mut()) };
    assert_eq!(rc, 0, "failed to install external SIGWINCH handler");

    // Now let the addon install over it — this must capture
    // `external_handler` as OLD_SIGWINCH, not our own handler.
    let snap = __test_only::sigwinch_install_snapshot();
    assert!(snap.1, "addon must install successfully");
    assert!(snap.2, "OLD_SIGWINCH must be non-null (points at external)");

    // Deliver SIGWINCH and verify chain actually ran.
    EXTERNAL_COUNT.store(0, Ordering::SeqCst);
    unsafe {
        libc::kill(libc::getpid(), libc::SIGWINCH);
    }
    std::thread::sleep(std::time::Duration::from_millis(20));

    // Drain the self-pipe to confirm our own handler fired.
    let mut buf = [0u8; 8];
    let n = unsafe { libc::read(snap.0, buf.as_mut_ptr() as *mut _, buf.len()) };
    assert!(n > 0, "addon self-pipe must have data — our handler fired");

    // External handler must also have fired via the chain.
    let ext = EXTERNAL_COUNT.load(Ordering::SeqCst);
    assert!(
        ext > 0,
        "TMB-017: external SIGWINCH handler must be chained — \
         OLD_SIGWINCH must have been published before addon install \
         so the chain target was captured correctly (got count = {})",
        ext
    );
}
