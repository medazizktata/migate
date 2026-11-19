# migate

A free, offline release gate for database migrations that works with **any migrator, any database and any host**. Its
primary check runs the version of your app that is still live against the database after your new migrations, so a
migration that would break the running app is stopped before it ships. It also stops a migration from changing after
production ran it, from running out of order or being silently skipped, and from disagreeing with production's own
history.

Status: early, not released. Works today on SQLite projects, Cloudflare D1's local mode included. `docs/product.md`
marks what is built and what is planned. (Earlier documents call it `migration-guard`, its working name.)
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

## Use

```sh
cargo build --release            # the binary is target/release/migate (or under $CARGO_TARGET_DIR)
migate check --migrator drizzle --base origin/main
migate ledger record <migration-id>   # after production ran it
```

`migate check` always checks the migration files against the ledger and the order rules (no database needed). With a
`[sandbox]` section in `migate.toml` it also migrates a throwaway SQLite database at the base branch and at HEAD, reports
breaking schema changes, and runs the base version's tests against the migrated database:

```toml
migrator = "drizzle"
base = "origin/main"

[sandbox]
env = { DATABASE_URL = "file:{db}" }   # the variable your project reads; {db} is the throwaway database's path
setup = "npm ci"                       # run in each checkout first
seed = "npm run seed"                  # optional: rows, so a migration that loses them is caught
test = "npm test"                      # the base version's tests: run on base's own database, at head, and on an
                                       # emptied copy, so a run that never used the database is "not checked"
```

An intended breaking change (the contract step of expand/contract) is accepted by a comment in the new migration, naming
one rule and one object, with a reason:

```sql
-- migate: contract drop-column users.legacy -- the app stopped using it in v42
ALTER TABLE users DROP COLUMN legacy;
```

The finding stays in the report as a note. A marker with no reason, or one that matches nothing, is an error (MG901),
and nothing accepts a failing old-version test.

For Cloudflare D1, migate runs `wrangler d1 migrations apply --local` into its own folder and finds the database
there; it only needs the database's name from your wrangler configuration:

```toml
migrator = "wrangler-d1"
[sandbox]
database_name = "app"
test = "npm test"
```

Migrate commands that use `npx` never download a package the project does not have.

Exit codes: 0 clean, 2 errors found, 3 could not check. `--format json` or `--format sarif` for machines.

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
  enable the hooks in `.githooks`: pre-commit (format, clippy, docs, tests) and commit-msg (the message rule in
  `CLAUDE.md`). Skip it if your global `core.hooksPath` already calls a repository's `.githooks`.
- The code follows `docs/architecture-decisions/0001-hexagonal-core.md`: a pure core, ports added only when needed.
- `bash scripts/check.sh` runs every check, the same way CI does: rustfmt, clippy (lints and type checks, `-D warnings`),
  rustdoc, tests, the core dependency rule, cargo-deny (advisories, licences, sources), shellcheck, actionlint and typos.
  A missing tool fails the run; `MIGATE_ALLOW_MISSING=1` runs what is installed.
- CI (`.github/workflows/ci.yml`) runs that script and checks the minimum Rust version (1.89); its actions are pinned to
  commit SHAs.
- Every rule migate enforces gets a case it must fail and one it must pass, and is shown failing by deliberate sabotage
  before it is trusted.

**Known local issue (development Mac):** `/opt/homebrew/bin` comes before rustup's `~/.cargo/bin` on the PATH, so cargo
resolves to the pinned 1.93.1 while rustc, clippy and rustfmt resolve to Homebrew's 1.99 (`just doctor` shows it).
Builds work, but local lints can differ from CI. A fix was applied and then undone by the owner on 2026-11-19 (TASK-42
has both): rustup's default set to Homebrew's Rust linked as a toolchain, plus rustup proxies for rustc, rustdoc, rustfmt
and clippy in `~/.local/bin`.

## Next
- Both de-risking spikes are done (`docs/research/spike-7.md`, `docs/research/spike-31.md`): real migrators run in the
  throwaway-database flow, and the mixed-version test catches what a schema diff cannot (Rails needs the old version
  booted before the migration). The first release's build tasks are next.
