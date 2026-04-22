#!/usr/bin/env bash
# scripts/compare-bench-baseline.sh
#
# TM-8g informational compare: print a markdown table comparing the
# current criterion measurement against the committed
# benches/baseline.json. **Never fails** — this is a *report*
# (intended for $GITHUB_STEP_SUMMARY in CI). The hard gate is the
# absolute budget enforced by scripts/check-bench-budget.sh.
#
# Why committed JSON instead of a downloadable artifact?
# - fork PRs cannot read other-run artifacts with secrets-scoped tokens.
# - artifact retention defaults expire (90 days) so PRs against a
#   long-quiet main would have no baseline.
# - committed baseline makes regression / improvement reviewable in
#   the PR diff itself when an intentional re-baseline happens.
#
# Usage:
#   scripts/compare-bench-baseline.sh [criterion_dir]
#
# Robustness contract: this script is wired into a `if: always()` CI
# step. It must never abort the build, even if jq output is malformed
# or estimates files are corrupt. We deliberately do NOT use
# `set -e`/`pipefail`; instead each parse path checks its result and
# falls back to "skipped" rows.

CRITERION_DIR="${1:-target/criterion}"
BASELINE_JSON="${BASELINE_JSON:-benches/baseline.json}"

skip() {
  echo "_(compare skipped: $1)_"
  exit 0
}

if [[ ! -d "$CRITERION_DIR" ]]; then
  skip "no criterion output at \`$CRITERION_DIR\` — bench step did not run"
fi

if [[ ! -f "$BASELINE_JSON" ]]; then
  skip "no baseline at \`$BASELINE_JSON\`"
fi

if ! command -v jq >/dev/null 2>&1; then
  skip "\`jq\` missing"
fi

# Validate baseline JSON parses at all before any non-trivial work.
if ! jq -e . "$BASELINE_JSON" >/dev/null 2>&1; then
  skip "baseline.json is not valid JSON"
fi

WARN_PCT=$(jq -r '.regression_warn_pct // 15' "$BASELINE_JSON" 2>/dev/null)
NOTE_PCT=$(jq -r '.improvement_note_pct // 5' "$BASELINE_JSON" 2>/dev/null)
CAPTURED=$(jq -r '.captured // "unknown"' "$BASELINE_JSON" 2>/dev/null)

# Coerce to sane defaults if anything came back empty / non-numeric.
[[ "$WARN_PCT" =~ ^[0-9]+(\.[0-9]+)?$ ]] || WARN_PCT=15
[[ "$NOTE_PCT" =~ ^[0-9]+(\.[0-9]+)?$ ]] || NOTE_PCT=5
[[ -n "$CAPTURED" ]] || CAPTURED="unknown"

echo "## Bench compare vs committed baseline"
echo ""
echo "Baseline captured \`$CAPTURED\`. Regression > ${WARN_PCT}% prints _warning_, improvement > ${NOTE_PCT}% prints _note_. **This step never fails the build** — the absolute-budget gate (\`check-bench-budget.sh\`) is the source of truth."
echo ""
echo "| Bench | Baseline (ns) | Current (ns) | Δ | Status |"
echo "|---|---:|---:|---:|---|"

# Read bench list with defensive fallback.
mapfile -t BENCHES < <(jq -r '.benches | keys[]?' "$BASELINE_JSON" 2>/dev/null)

if [[ "${#BENCHES[@]}" -eq 0 ]]; then
  echo "| — | — | — | — | _no benches in baseline.json_ |"
  echo ""
  echo "_(compare ended: empty bench list)_"
  exit 0
fi

for bench in "${BENCHES[@]}"; do
  base=$(jq -r --arg b "$bench" '.benches[$b].median_ns // empty' "$BASELINE_JSON" 2>/dev/null)
  if [[ -z "$base" || ! "$base" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
    echo "| \`$bench\` | _missing_ | — | — | skipped (bad baseline) |"
    continue
  fi

  est="$CRITERION_DIR/$bench/new/estimates.json"
  if [[ ! -f "$est" ]]; then
    echo "| \`$bench\` | $base | — | — | missing |"
    continue
  fi

  current=$(jq -r '.median.point_estimate // empty' "$est" 2>/dev/null)
  if [[ -z "$current" || ! "$current" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
    echo "| \`$bench\` | $base | _bad measurement_ | — | skipped |"
    continue
  fi

  current_int=$(printf "%.0f" "$current" 2>/dev/null || echo "?")
  delta_pct=$(awk -v c="$current" -v b="$base" 'BEGIN { if (b == 0) print "n/a"; else printf "%+.2f", ((c - b) / b) * 100 }' 2>/dev/null)
  [[ -n "$delta_pct" ]] || delta_pct="?"

  status=$(awk -v c="$current" -v b="$base" -v warn="$WARN_PCT" -v note="$NOTE_PCT" 'BEGIN {
    if (b == 0) { print "n/a"; exit }
    pct = ((c - b) / b) * 100
    if (pct > warn)       { print "warning (regression)" }
    else if (pct < -note) { print "note (improvement)"   }
    else                  { print "ok"                   }
  }' 2>/dev/null)
  [[ -n "$status" ]] || status="?"

  echo "| \`$bench\` | $base | $current_int | ${delta_pct}% | $status |"
done

echo ""
echo "_To re-baseline intentionally: run benches locally, copy median.point_estimate from \`target/criterion/<bench>/new/estimates.json\` into \`benches/baseline.json\`, document the change in \`CHANGELOG.md\`._"

exit 0
