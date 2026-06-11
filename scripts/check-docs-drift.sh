#!/usr/bin/env bash
# scripts/check-docs-drift.sh
#
# CI gate: ensure that `docs/api.md` is the verbatim output of
# `taida doc generate ./taida`. If a developer modifies a `///@` doc-comment
# in any `taida/*.td` file but forgets to regenerate `docs/api.md`, this
# script fails the build with a clear diff.
#
# Usage:
#   scripts/check-docs-drift.sh
#
# Exit code:
#   0 - docs/api.md is up to date
#   1 - drift detected (diff printed to stdout)
#   2 - taida CLI not found (CI environment misconfigured)
#
# Binary selection mirrors scripts/smoke-test.sh: `TAIDA` overrides,
# otherwise `taida` from PATH (the smoke job's `cargo install` step
# puts it there). Local runs against a stale globally-installed taida
# misreport drift, so always pass TAIDA when validating locally.

set -euo pipefail

TAIDA="${TAIDA:-taida}"

if ! command -v "$TAIDA" >/dev/null 2>&1; then
  echo "::error::taida CLI not found ($TAIDA). Install it via cargo first or set TAIDA." >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

EXPECTED="$REPO_ROOT/docs/api.md"
ACTUAL="$(mktemp -t terminal-api-XXXXXX.md)"
trap 'rm -f "$ACTUAL"' EXIT

if [[ ! -f "$EXPECTED" ]]; then
  echo "::error::docs/api.md is missing. Run 'taida doc generate ./taida > docs/api.md' and commit." >&2
  exit 1
fi

# Generate fresh markdown from current taida/*.td sources
"$TAIDA" doc generate ./taida >"$ACTUAL"

if diff -u "$EXPECTED" "$ACTUAL"; then
  echo "docs/api.md is up to date with taida/*.td"
  exit 0
else
  echo "" >&2
  echo "::error::docs/api.md drift detected." >&2
  echo "" >&2
  echo "The committed docs/api.md does not match 'taida doc generate ./taida'." >&2
  echo "Regenerate locally and commit:" >&2
  echo "  taida doc generate ./taida > docs/api.md" >&2
  echo "  git add docs/api.md && git commit -m 'docs: regenerate api.md'" >&2
  exit 1
fi
