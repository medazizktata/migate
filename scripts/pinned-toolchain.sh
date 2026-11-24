# Sourced by scripts/check.sh and .githooks/pre-commit from the repository root: sets $cargo to the toolchain
# rust-toolchain.toml pins, with clippy and rustfmt from the same build, or exits.
# shellcheck shell=bash
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
