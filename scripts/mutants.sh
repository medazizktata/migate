#!/usr/bin/env bash
# Mutation testing with cargo-mutants on one crate (default migate-core). Each mutant is a small deliberate bug; one
# the tests do not catch marks behaviour no test checks. Extra arguments go to cargo-mutants, for example
# `--test-workspace=true` (run every crate's tests, so the CLI tests can catch an engine's mutants) or `--file <path>`.
#
# The build must happen inside cargo-mutants' own copy of the tree. A cargo wrapper that exports a shared
# CARGO_TARGET_DIR made two early runs build unmutated code and report nothing real, so the variable is unset, the
# toolchain's own cargo is called, and the baseline log is checked afterwards.
set -euo pipefail

crate="${1:-migate-core}"
shift || true
out="${MUTANTS_OUT:-$PWD/mutants.out-dir}"
jobs="${MUTANTS_JOBS:-2}"

command -v cargo-mutants >/dev/null || { echo "cargo-mutants is not installed: cargo install cargo-mutants" >&2; exit 1; }
cargo_bin="$(rustup which cargo)" # the toolchain rust-toolchain.toml pins, not a wrapper earlier on PATH
mkdir -p "$out"

status=0
env -u CARGO_TARGET_DIR CARGO="$cargo_bin" PATH="$(dirname "$cargo_bin"):$PATH" \
  cargo-mutants mutants -p "$crate" -j "$jobs" --output "$out" "$@" || status=$?

log="$out/mutants.out/log/baseline.log"
[ -f "$log" ] || { echo "mutants: no baseline log at $log; the run did not start" >&2; exit 1; }
outside="$(grep -o -- '--out-dir [^ ]*' "$log" | grep -v 'cargo-mutants-' || true)"
if [ -n "$outside" ]; then
  echo "mutants: INVALID run, the baseline built outside cargo-mutants' copy, so mutants were never compiled:" >&2
  echo "$outside" | head -3 >&2
  exit 1
fi
count() { [ -f "$out/mutants.out/$1" ] && wc -l < "$out/mutants.out/$1" | tr -d ' ' || echo 0; }
echo "mutants ($crate): caught $(count caught.txt), missed $(count missed.txt), timeout $(count timeout.txt), unviable $(count unviable.txt)"
[ "$(count missed.txt)" = 0 ] || sed 's/^/  missed: /' "$out/mutants.out/missed.txt"
exit "$status"
