#!/usr/bin/env bash
# scripts/check-bench-budget.sh
#
# TM-8g hard gate: fail if any of the 5 budgeted benches in
# benches/renderer_perf.rs exceeds its absolute production budget.
#
# Why absolute budgets and not 15% relative regression?
# GitHub-hosted runners are noisy (±15% jitter is common), so a
# relative gate produces false positives. The absolute budgets are
# anchored to the Hachikuma R-2 acceptance criteria — anything that
# blows them up is a real regression (think O(N²) creep).
#
# Why hard-coded budgets (not loaded from benches/baseline.json)?
# baseline.json captures *observed* performance for the informational
# compare. If both the bench whitelist and the budgets lived there,
# a "re-baseline" PR could silently relax the hard gate or remove a
# bench from coverage. Keeping the policy (this script) separate from
# the observation (baseline.json) makes a budget change an explicit,
# code-review-visible diff in `scripts/check-bench-budget.sh`.
#
# The relative comparison vs benches/baseline.json is run separately
# by scripts/compare-bench-baseline.sh as an *informational* report
# (no fail).
#
# Usage:
#   scripts/check-bench-budget.sh [criterion_dir]
#
# Default criterion_dir is target/criterion. Reads each whitelisted
# bench's new/estimates.json (criterion v0.5 layout) and compares
# median.point_estimate (ns) against the hard-coded budget below.
#
# Exits 1 on any over-budget bench, 0 otherwise.
set -euo pipefail

CRITERION_DIR="${1:-target/criterion}"

if [[ ! -d "$CRITERION_DIR" ]]; then
  echo "ERROR: criterion output directory not found: $CRITERION_DIR" >&2
  echo "Hint: run 'cargo bench --bench renderer_perf -- --noplot' first." >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq is required but not installed" >&2
  exit 2
fi

# ── Hard-coded policy ──────────────────────────────────────────────
# Bench name → absolute budget in nanoseconds.
# Anchored to the TMB-020 acceptance criteria recorded in
# benches/renderer_perf.rs (// ## Budget). Changing any value here
# requires an explicit, reviewable diff to this file.
#
# render_full_scaling/* and render_ops_120_writes are intentionally
# excluded — they are sanity bench groups without an acceptance
# criterion. They still run via cargo bench but are not gated.
BUDGETS=(
  "buffer_write_120chars_120x40:500000"        # 500 µs
  "compose_pane_40rows_120x40:5000000"         # 5 ms
  "render_full_120x40:5000000"                 # 5 ms
  "render_frame_identical_120x40:100000"       # 100 µs
  "render_frame_one_cell_diff_120x40:2000000"  # 2 ms
)

printf "%-44s %14s %14s %12s %s\n" "BENCH" "MEDIAN (ns)" "BUDGET (ns)" "USED %" "STATUS"
printf -- "----------------------------------------------------------------------------------------------------\n"

failed=0
missing=0
for entry in "${BUDGETS[@]}"; do
  bench="${entry%%:*}"
  budget="${entry##*:}"
  est="$CRITERION_DIR/$bench/new/estimates.json"
  if [[ ! -f "$est" ]]; then
    printf "%-44s %14s %14s %12s %s\n" "$bench" "—" "$budget" "—" "MISSING"
    missing=$((missing + 1))
    continue
  fi
  median=$(jq -r '.median.point_estimate' "$est")
  median_int=$(printf "%.0f" "$median")
  pct=$(awk -v m="$median" -v b="$budget" 'BEGIN { printf "%.2f", (m / b) * 100 }')
  if awk -v m="$median" -v b="$budget" 'BEGIN { exit (m > b) ? 0 : 1 }'; then
    printf "%-44s %14s %14s %12s %s\n" "$bench" "$median_int" "$budget" "$pct" "OVER BUDGET"
    failed=$((failed + 1))
  else
    printf "%-44s %14s %14s %12s %s\n" "$bench" "$median_int" "$budget" "$pct" "ok"
  fi
done

printf -- "----------------------------------------------------------------------------------------------------\n"

if [[ $missing -gt 0 ]]; then
  echo "ERROR: $missing budgeted bench(es) missing from $CRITERION_DIR" >&2
  exit 2
fi

if [[ $failed -gt 0 ]]; then
  echo "FAIL: $failed bench(es) exceeded their absolute budget" >&2
  exit 1
fi

echo "PASS: all ${#BUDGETS[@]} budgeted benches within absolute budget"
