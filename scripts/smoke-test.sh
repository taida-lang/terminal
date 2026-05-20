#!/usr/bin/env bash
# smoke-test.sh -- Run facade smoke test via the Taida interpreter.
#
# This script sets up a temporary workspace with a staged copy of the
# terminal package (including the addon cdylib), then runs
# examples/smoke_test.td through `taida` to verify all facade .td
# files parse, export, and execute correctly.
#
# The original worktree is never modified — all staging happens in a
# temp directory that is cleaned up on exit.
#
# Usage:
#   ./scripts/smoke-test.sh          # uses `taida` from PATH
#   TAIDA=./target/release/taida ./scripts/smoke-test.sh  # custom binary
#
# Exit code 0 = PASS, 1 = FAIL.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PACKAGE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TAIDA="${TAIDA:-taida}"

# Verify taida is available.
if ! command -v "$TAIDA" &>/dev/null; then
  echo "ERROR: taida not found. Install with: cargo install --git https://github.com/taida-lang/taida.git taida" >&2
  exit 1
fi

# Detect platform-specific cdylib name.
case "$(uname -s)" in
  Darwin*) LIB_NAME="libtaida_lang_terminal.dylib" ;;
  MINGW*|MSYS*|CYGWIN*) LIB_NAME="taida_lang_terminal.dll" ;;
  *) LIB_NAME="libtaida_lang_terminal.so" ;;
esac

# Find the cdylib. Prefer the current Cargo build output so local ignored
# artifacts under native/ cannot mask code changes.
LIB_PATH=""
for candidate in \
  "$PACKAGE_DIR/target/debug/$LIB_NAME" \
  "$PACKAGE_DIR/target/release/$LIB_NAME" \
  "$PACKAGE_DIR/native/$LIB_NAME"; do
  if [ -f "$candidate" ]; then
    LIB_PATH="$candidate"
    break
  fi
done
if [ -z "$LIB_PATH" ]; then
  echo "ERROR: addon cdylib ($LIB_NAME) not found. Run 'cargo build' first." >&2
  exit 1
fi

# Create temporary workspace with a staged package copy.
# This avoids writing anything to the original worktree.
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

STAGED_PKG="$WORK_DIR/.taida/deps/taida-lang/terminal"
mkdir -p "$STAGED_PKG/native" "$STAGED_PKG/taida"

# Copy facade sources and the cdylib into the staged package.
cp "$PACKAGE_DIR"/taida/*.td "$STAGED_PKG/taida/"
cp "$PACKAGE_DIR/native/addon.toml" "$STAGED_PKG/native/"
cp "$PACKAGE_DIR/native/addon.lock.toml" "$STAGED_PKG/native/" 2>/dev/null || true
cp "$LIB_PATH" "$STAGED_PKG/native/$LIB_NAME"
cp "$PACKAGE_DIR/packages.tdm" "$STAGED_PKG/"

echo '<<<@a.4' > "$WORK_DIR/packages.tdm"
cp "$PACKAGE_DIR/examples/smoke_test.td" "$WORK_DIR/main.td"

# Run the smoke test.
echo "Running facade smoke test..."
OUTPUT="$("$TAIDA" "$WORK_DIR/main.td" 2>&1)" || {
  echo "FAIL: taida exited with non-zero status" >&2
  echo "$OUTPUT" >&2
  exit 1
}

echo "$OUTPUT"

# Verify PASS marker.
if echo "$OUTPUT" | grep -q 'PASS:all_smoke_tests'; then
  echo ""
  echo "PASS: facade smoke test succeeded"
  exit 0
else
  echo ""
  echo "FAIL: PASS:all_smoke_tests marker not found in output" >&2
  exit 1
fi
