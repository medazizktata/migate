#!/usr/bin/env bash
# Every quality check, the same locally and in CI. Runs them all, prints a summary, exits non-zero if any failed.
# A missing tool is a failure, not a skip: a check that did not run must not look green. To run what is installed
# anyway, set MIGATE_ALLOW_MISSING=1 (the summary still lists what was missing).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# The pinned toolchain (rust-toolchain.toml) is the one CI runs. A Rust outside rustup (a system or Homebrew one)
# ignores the pin, and cargo finds its subcommands (clippy, fmt) on PATH, so rustup's must come first: otherwise even
# rustup's cargo runs a newer clippy that passes what CI's fails. Then each tool must be the pinned build.
pinned=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)
if [ -z "${CARGO:-}" ] && [ -x "$HOME/.cargo/bin/cargo" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
  export PATH
fi
cargo=${CARGO:-cargo}
if [ "$("$cargo" --version 2>/dev/null | awk '{print $2}')" != "$pinned" ]; then
  echo "check: $cargo is $("$cargo" --version 2>/dev/null), but rust-toolchain.toml pins $pinned: install it with rustup, or set CARGO" >&2
  exit 1
fi
# Clippy is numbered 0.1.<minor> of its Rust; rustfmt carries the same build hash as the clippy beside it.
clippy=$("$cargo" clippy --version 2>/dev/null)
minor=$(echo "$pinned" | cut -d. -f2)
build=$(echo "$clippy" | sed -n 's/.*(\([0-9a-f]*\) .*/\1/p')
if [ "$(echo "$clippy" | awk '{print $2}')" != "0.1.$minor" ] || [ -z "$build" ] \
  || ! "$cargo" fmt --version 2>/dev/null | grep -q -F "($build "; then
  echo "check: clippy is ${clippy:-missing} and rustfmt $("$cargo" fmt --version 2>/dev/null), not the pinned $pinned build" >&2
  exit 1
fi
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
