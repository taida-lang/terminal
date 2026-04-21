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
//!
//! ── Review follow-up (2026-04-21) ───────────────────────────────
//!
//! The original v1 of this test file attempted the "strong path"
//! (external handler → addon install → SIGWINCH delivery → chain
//! assertion) inside `external_sigwinch_handler_is_still_chained_after_install`
//! but guarded on `sigwinch_install_snapshot()` — which itself calls
//! `ensure_sigwinch_pipe()` and therefore installed the addon handler
//! *before* the test could pre-install its external handler. The
//! `if pre_snapshot.1 { return; }` branch was always taken on success
//! environments, making the strong path unreachable.
//!
//! The fix is split across two binaries:
//!
//!   - **This file** keeps the observation-only invariants (tests 1/2)
//!     using the side-effectful `sigwinch_install_snapshot()` probe.
//!     These two tests legitimately *want* the install to happen — they
//!     pin ordering invariants that can only be observed post-install.
//!   - **`tests/sigwinch_external_chain.rs`** is a dedicated binary
//!     containing only the strong path. Because cargo compiles each
//!     `tests/*.rs` file into a separate binary, that file's single
//!     test runs in a fresh process where SIGWINCH disposition is
//!     guaranteed uninstalled at entry, so the external-handler-first
//!     sequence is reliably reachable.

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

/// Invariant 3 (weaker): the pure probe does not mutate state.
///
/// This pins the new test probe contract introduced in the review
/// follow-up: `sigwinch_pure_probe()` must *only* load the atomics
/// and must never install the addon handler as a side effect.
/// Calling it should be observationally equivalent to a no-op, so
/// two back-to-back calls return the same tuple, and its return
/// value at steady state matches what `sigwinch_install_snapshot`
/// observes post-install (installed=true, old_non_null=true).
#[test]
fn pure_probe_has_no_install_side_effect() {
    // First, ensure the addon is installed by using the install
    // probe — the pure probe below must agree with the state the
    // install probe left behind.
    let snap = __test_only::sigwinch_install_snapshot();
    if snap.0 < 0 {
        return;
    }

    let probe_a = __test_only::sigwinch_pure_probe();
    let probe_b = __test_only::sigwinch_pure_probe();
    assert_eq!(
        probe_a, probe_b,
        "TMB-017 review: sigwinch_pure_probe must be idempotent \
         (pure read of atomics, no install side effect)"
    );
    assert!(
        probe_a.0 && probe_a.1,
        "TMB-017 review: after install, pure probe must report \
         installed=true AND old_non_null=true"
    );
}
