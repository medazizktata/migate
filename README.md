# migate

A free, offline release gate for database migrations that works with **any migrator, any database and any host**. Its
primary check runs the version of your app that is still live against the database after your new migrations, so a
migration that would break the running app is stopped before it ships. It also stops a migration from changing after
production ran it, from running out of order or being silently skipped, and from disagreeing with production's own
history.

Status: research and decisions done, nothing built yet. (Earlier documents call it `migration-guard`, its working name.)
Licence: Apache-2.0 (`LICENSE`). Start here:

- `docs/product.md`: the product: the problem, who it is for, what it does, what it is not, and where it stands.
- `docs/research/README.md`: the summary: the gap, the validation, the architecture, the stack, what is not verified,
  and the decisions taken.
- `docs/research/validation.md`: is the problem real and weighty (41 classified incidents, kill criteria, the case
  against).
- `docs/research/blind-review.md`: a context-free review of the decisions, and the decisions taken after it.
- `docs/research/gap.md`, `architecture.md`, `stack.md`: the full reports; `docs/research/bench/`: the benchmark sources
  and validation data.
- `docs/research/first-pass.md`: the first round (landscape, 21 migrators' history tables), with corrections at the top.
- `docs/transcript.md`: the conversation this project came from, with the first round's research reports in full.
- `docs/prior-art/the Payload-on-D1 app/`: the single-repo version it grew out of (the Payload-on-D1 app, Payload on Cloudflare D1), with its ADR.

## Direction (the owner's, 2026-11-19)
- The goal is any database and any host, reached through the architecture: the ledger and production-history layers
  work for every engine from the first release; breaking-change detection is a plug-in per engine (a throwaway database
  plus a schema reader), and reaching production is a plug-in per host.
- D1 is one host of the SQLite engine, the way RDS is one host of PostgreSQL. SQLite is the first engine to build and
  dogfood; PostgreSQL ships in the same first release and leads the public launch.
- Independent and pluggable into any system; no vendor is a dependency of the core. The general solution comes first;
  a GFS-dedicated feature only once it works for most databases.
- Rust; Apache-2.0; pure open source, no revenue.

## Development
- Rust is pinned in `rust-toolchain.toml` (1.93.1, what rustup already has on the development Mac). `just doctor` shows
  which cargo, rustc, clippy and rustfmt your shell really uses; they should all match the pin (see "Known local issue").
- `just` lists the usual commands: `just check`, `just test`, `just lint`, `just fmt`. Run `just hooks` once per clone to
  enable the pre-commit hook (format, clippy, tests).
- The code follows `docs/architecture-decisions/0001-hexagonal-core.md`: a pure core, ports added only when needed.
- `bash scripts/check.sh` runs every check, the same way CI does: rustfmt, clippy (lints and type checks, `-D warnings`),
  rustdoc, tests, the core dependency rule, cargo-deny (advisories, licences, sources), shellcheck, actionlint and typos.
  A missing tool fails the run; `MIGATE_ALLOW_MISSING=1` runs what is installed.
- CI (`.github/workflows/ci.yml`) runs that script and checks the minimum Rust version (1.85); its actions are pinned to
  commit SHAs.
- Every rule migate enforces gets a case it must fail and one it must pass, and is shown failing by deliberate sabotage
  before it is trusted.

**How Rust resolves on the development Mac (fixed 2026-11-19):** rustup's default is Homebrew's Rust, linked as a
toolchain (`rustup toolchain link homebrew /opt/homebrew/opt/rust`), and `~/.local/bin` holds rustup proxies for `rustc`,
`rustdoc`, `rustfmt` and clippy next to the cargo shim. So inside a repository with a `rust-toolchain.toml` every tool
follows the pin, and elsewhere every tool is Homebrew's. `just doctor` should show the pinned version four times.

## Next
- Both de-risking spikes are done (`docs/research/spike-7.md`, `docs/research/spike-31.md`): real migrators run in the
  throwaway-database flow, and the mixed-version test catches what a schema diff cannot (Rails needs the old version
  booted before the migration). The first release's build tasks are next.
