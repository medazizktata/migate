# Short names for the usual commands. `just` lists them.

default:
    @just --list

# Every check, as CI runs it
check:
    bash scripts/check.sh

# Tests only
test:
    cargo test --workspace

# Lints and type checks
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Format the code
fmt:
    cargo fmt --all

# Enable the pre-commit hook for this clone
hooks:
    git config core.hooksPath .githooks

# Show which Rust tools this shell really uses (they should all match rust-toolchain.toml)
doctor:
    @grep channel rust-toolchain.toml
    @cargo --version
    @cargo rustc -p migate-core --lib -q -- -V
    @cargo clippy --version
    @cargo fmt --version

# Mutation testing on one crate (default migate-core); extra arguments go to cargo-mutants. See scripts/mutants.sh.
mutants crate="migate-core" *args:
    bash scripts/mutants.sh {{crate}} {{args}}
