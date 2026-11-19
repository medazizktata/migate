#!/usr/bin/env bash
# Every quality check, the same locally and in CI. Runs them all, prints a summary, exits non-zero if any failed.
# A missing tool is a failure, not a skip: a check that did not run must not look green. To run what is installed
# anyway, set MIGATE_ALLOW_MISSING=1 (the summary still lists what was missing).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

cargo=${CARGO:-cargo}
failed=() missing=() passed=()

step() { # step <name> <tool-to-require> <command...>
  local name=$1 tool=$2; shift 2
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "=== $name: MISSING ($tool not installed)"
    missing+=("$name")
    return
  fi
  echo "=== $name"
  if "$@"; then passed+=("$name"); else failed+=("$name"); fi
}

step "format (rustfmt)"           "$cargo"     "$cargo" fmt --all --check
step "lint + type check (clippy)" "$cargo"     "$cargo" clippy --workspace --all-targets --locked -- -D warnings
step "docs (rustdoc)"             "$cargo"     env RUSTDOCFLAGS="-D warnings" "$cargo" doc --workspace --no-deps --locked
step "tests"                      "$cargo"     "$cargo" test --workspace --locked
step "core dependency rule"       "$cargo"     env CARGO="$cargo" bash scripts/check-core-deps.sh
step "dependencies (cargo-deny)"  cargo-deny  cargo-deny --log-level error check
step "shell scripts (shellcheck)" shellcheck  shellcheck scripts/*.sh
step "workflows (actionlint)"     actionlint  actionlint
step "spelling (typos)"           typos       typos

echo
echo "passed:  ${#passed[@]}"
[ ${#missing[@]} -gt 0 ] && echo "MISSING: ${missing[*]}"
[ ${#failed[@]} -gt 0 ] && echo "FAILED:  ${failed[*]}"
if [ ${#failed[@]} -gt 0 ]; then exit 1; fi
if [ ${#missing[@]} -gt 0 ] && [ "${MIGATE_ALLOW_MISSING:-0}" != 1 ]; then exit 1; fi
exit 0
