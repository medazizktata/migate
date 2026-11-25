# migate

A free, offline release gate for database migrations that works with **any migrator, any database and any host**. Its
primary check runs the version of your app that is still live against the database after your new migrations, so a
migration that would break the running app is stopped before it ships. It also stops a migration from changing after
production ran it, from running out of order or being silently skipped, and from disagreeing with production's own
history. Offline means migate itself needs no network and sends nothing anywhere; the commands you give it run as
they would on your machine, so an install in `setup` still downloads what your lockfile names, and a framework's
telemetry still reports home (Payload's does on every `payload migrate`, unless its config sets `telemetry: false`).

Status: early, not released. Works today on SQLite projects (Cloudflare D1's local mode included) and on PostgreSQL
projects, with a throwaway local server. `docs/product.md`
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

## Install

From a clone, on macOS or Linux, with Rust 1.89 or newer:

```sh
cargo install --locked --path crates/cli
```

This puts `migate` in `~/.cargo/bin`. Each GitHub release also carries a Linux binary.

## Use

```sh
cargo build --release            # the binary is target/release/migate (or under $CARGO_TARGET_DIR)
migate check --migrator drizzle --base origin/main
migate ledger record <migration-id>   # after production ran it
migate history --history-command "<cmd>"   # at release: production's history against the repo and the ledger
```

At release, before migrating, `migate history` compares what production's history table records with the repository
and the ledger: a migration production ran that the repository lacks (MG010), one no release recorded (MG011), and one
the ledger records that production lacks (MG012). The release job reads production with its own read-only
credentials; migate only runs the command and reads one migration per line, and hides the credentials in any URL it
prints. Any client that prints the migrator's history query one row per line works (`psql`, `sqlite3`, or
`wrangler d1 execute --remote` with its JSON reduced to names); for PostgreSQL and Payload:

```sh
migate history --history-command 'psql "$PROD_URL" -XAtc "SELECT name FROM payload_migrations WHERE batch <> -1"'
# Cloudflare D1, through Wrangler's own client (tested with --local; not yet run against a production database):
migate history --history-command 'npx wrangler d1 execute DB --remote --json \
  --command "SELECT name FROM payload_migrations WHERE batch <> -1" \
  | jq -r "if type == \"array\" then .[0].results[].name else error(.error.text) end"'
```

Wrangler reports a failure as a JSON object rather than rows, so the filter turns it into Wrangler's own message.
D1's `database_id` must be the real one: a committed placeholder reaches no database (Cloudflare error 7404).

`migate check` always checks the migration files against the ledger and the order rules (no database needed). With a
`[sandbox]` section in `migate.toml` it also migrates a throwaway SQLite database at the base branch and at HEAD, reports
breaking schema changes, and runs the base version's tests against the migrated database:

```toml
migrator = "drizzle"
base = "origin/main"

[sandbox]
env = { DATABASE_URL = "file:{db}" }   # the variable your project reads; {db} is the throwaway database's path
setup = "pnpm install --frozen-lockfile"   # run in each checkout first
seed = "pnpm run seed"                 # optional: rows, so a migration that loses rows or values is caught
test = "pnpm test"                     # the base version's tests: run on base's own database, at head, and on an
                                       # emptied copy, so a run that never used the database is "not checked"
readback = "pnpm run readback"         # optional: the base version reads every row the head version's tests wrote,
                                       # as after a rollback (MG812); load each row with the old code's models
```

The old version's tests are the primary check, and the schema rules still decide on their own: passing tests do not
clear a schema error, since a suite that only reads a table cannot vouch for a trigger that rejects writes to it. A
constraint on a column the same change adds, empty in every existing row after the migration, is a note.

A dropped or renamed column the version still serving traffic uses is an error no contract marker can accept
(MG212): stop using it in one release, drop it in the next. Each migrator's file says how its code names a column, and
what that cannot see:

| Migrator | The old code uses a column when | It cannot see |
|---|---|---|
| Rails | it is not in the model's `ignored_columns` (Rails loads every column) | nothing: it errs toward "used" |
| Prisma | `schema.prisma` has a field named like it or `@map`ped to it | raw SQL (`$queryRaw`) |
| Drizzle | a schema file naming the table has it as a string | raw SQL, a column named in another file than its table |
| Django | `models.py` has the field (a foreign key's without `_id`) or a `db_column` | `extra()`, raw SQL, models outside `models.py` |
| Payload | a collection or global has the field (camelCase) | nested groups and arrays (their columns carry a prefix) |
| Wrangler D1, Supabase, dbmate | the app's source has its name in a file that also names its table | SQL built at run time; a name shared with another table can mislead it |

Lost data needs rows to be seen, so set `seed` when the tests delete their own rows. A rebuilt table whose children
cascade on delete is an error even without rows (MG104) when the tool renames a copy into place, as Drizzle does
(drizzle-orm#4938); Django rebuilds that way too, but only with foreign keys off, so it is exempt. A tool that drops
the table and creates it again under its own name leaves no trace without rows: Rails 8.1 on SQLite does this, and its
`change_column_null` deleted a parent's cascading children in our test, so with Rails only a seed shows it (MG105).
A SQL migration that empties a table outright (a `DELETE` with no `WHERE`, also inside a `WITH`, or a `TRUNCATE`) is an
error from its text (MG107), rows or not, in a check with a `[sandbox]` section; a table the same migration creates is
exempt, and a marker accepts a deliberate emptying.

An intended breaking change (the contract step of expand/contract) is accepted by a comment in the new migration, naming
one rule and one object, with a reason:

```sql
-- migate: contract drop-column users.legacy -- the app stopped using it in v42
ALTER TABLE users DROP COLUMN legacy;
```

The finding stays in the report as a note. A marker with no reason, one that matches nothing, or one that matches
several objects (`users.legacy` when two schemas each drop one: write `public.users.legacy`) is an error (MG901), and
nothing accepts a failing old-version test.

Triggers a migration adds or changes are always errors (MG503, MG504): nothing migate can see proves the running app
never writes what a new trigger refuses. Not the rows the database already holds, either: a migration that fills a new
column and then requires it passes every stored row and refuses every insert from the old app. When stored rows would
already be refused, the error says how many. Each trigger error prints the marker that accepts it, ready to copy; the
reason is the reviewer's answer to the one question that matters:

```sql
-- migate: contract add-trigger users.need_tenant -- the old app has sent a tenant on every insert since v41
```

The marker is read in any comment style, so a TypeScript migration (Payload, Drizzle) uses `//`.

A project that already keeps its own record of deployed migrations, as `{name: sha256 of the file}`, can have migate
read and extend that record instead of keeping a second ledger. That format holds no code-only hash, so any edit to a
recorded migration is MG001, a comment-only one included. A project that already marks its contract steps its own way
can have migate take that marker too: a migration holding `// CONTRACT: <why>` accepts its findings at known lines, as
one `migate: contract` marker per finding would. A failing old-version test, a failed run and the migration record are
never accepted.

```toml
ledger = "db/deployed.json"
ledger_format = "sha256"
[sandbox]
contract_marker = "CONTRACT"
```

For Cloudflare D1, migate runs `wrangler d1 migrations apply --local` into its own folder and finds the database
there; it only needs the database's name from your wrangler configuration:

```toml
migrator = "wrangler-d1"
[sandbox]
database_name = "app"
test = "pnpm test"
```

That is for Wrangler's own SQL migrations. An app whose ORM writes the migrations (Payload, Drizzle) and runs them on
D1 names its ORM's migrator instead, and usually its own migrate script: the bundled Payload command,
`npx --no payload migrate` at the repository root, finds nothing in a monorepo where Payload is installed in one app.
This set-up ran on such an app (Payload on D1, pnpm workspaces):

```toml
migrator = "payload"
out_of_order = "block"                 # when your own runner refuses a late migration
[sandbox]
shared = "apps/web/.wrangler"          # where the app keeps its local D1, inside the checkout
database = "{dir}/shared/state/v3/d1/miniflare-D1DatabaseObject/*.sqlite"
migrate = "pnpm db:migrate"            # the project's own script, run at the repository root
setup = "pnpm install --frozen-lockfile"
seed = "pnpm run seed"                 # a suite that deletes its own rows leaves nothing to show lost data
test = "pnpm --filter api test:int"
```

For PostgreSQL, migate starts a throwaway server from the machine's own Postgres (found on PATH, in the usual install
folders, or at `MIGATE_PG_BIN`) and stops it when the run ends or is cancelled. Tested with PostgreSQL 15, 16, 17
and 18:

```toml
[sandbox]
engine = "postgres"
env = { DATABASE_URL = "{url}" }   # postgresql://migate@127.0.0.1:<free port>/migate
test = "pnpm test"
```

The schema diff covers tables, columns, keys, constraints, indexes, views, triggers, routines, enums and sequences.
Grants are compared too: a role that loses a privilege on an object still there is an error (MG211), since the base
version fails there if it connects as that role. A change to anything else (a policy, a domain, an exclusion
constraint, a partition and the like) is named as not checked (MG801): only the base version's tests judged it. Not
even they judge row-level security: they connect as the sandbox's superuser, which no policy restricts, unless they
switch to another role.

Production may run an older engine than the sandbox. Set `target_version` under `[sandbox]` (`"3.37.2"` for SQLite,
`"15"` for PostgreSQL): a SQL migration using syntax that version lacks is an error (MG701), PostgreSQL runs on that
major when it is installed, and the run says what it could not check on production's version (MG806). On SQLite, a
migration in another language (TypeScript, Python, Ruby) is checked through the schema it leaves: a table, view, index
or trigger it creates or changes. A statement that leaves no trace there, such as `ALTER TABLE ... DROP COLUMN`, is
reported as not checked (MG801), not passed.

Cloudflare D1 neither publishes its SQLite version nor lets a query read it: `sqlite_version()` is outside its function
allowlist. Probe a feature instead: `select 1_000` succeeds from SQLite 3.46.0 on. D1 refuses functions outside that
allowlist (`json_pretty()`, `if()`) whatever its version; local Wrangler enforces the same list, so a sandbox that
migrates through Wrangler fails on them (MG803) as production would.

migate's checkouts are fresh, so the files your `.gitignore` keeps out (`.env`, `.dev.vars`) are not there, and a suite
that needs them fails before checking anything. Write them in `setup`, which runs first in each checkout, from the
project's committed example or the script your CI uses; never copy your own, which hold real secrets:

```toml
setup = "cp apps/web/.env.example apps/web/.env && pnpm install --frozen-lockfile"
```

Test frameworks that build their own test database must be told to use the migrated one, or migate cannot judge the
run:

- **Django:** `test = "python manage.py test --keepdb"`, with `DATABASES["default"]["TEST"]["NAME"]` set to the
  migrated database (`migate` on Postgres, the `{db}` file on SQLite). Without `--keepdb` Django rebuilds the database
  from the old migrations and the run proves nothing.
- **Rails:** `DATABASE_URL` set to `{url}`, `PARALLEL_WORKERS=1` (workers rebuild their own databases) and
  `config.active_record.maintain_test_schema = false` in the test environment (otherwise Rails reloads the old
  `schema.rb` over the migrated database).

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
- Rust is pinned in `rust-toolchain.toml` (1.93.1). `scripts/check.sh` and the pre-commit hook run on that toolchain
  whatever the PATH says (`scripts/pinned-toolchain.sh`): they put rustup's tools first and refuse a cargo, clippy or
  rustfmt of another build, since a newer clippy passes what CI's fails. `just doctor` shows what your shell resolves.
- `just` lists the usual commands: `just check`, `just test`, `just lint`, `just fmt`. Run `just hooks` once per clone to
  enable the hooks in `.githooks`: pre-commit (format, clippy, docs, tests) and commit-msg (the message rule in
  `CLAUDE.md`). Skip it if your global `core.hooksPath` already calls a repository's `.githooks`.
- The code follows `docs/architecture-decisions/0001-hexagonal-core.md`: a pure core, ports added only when needed.
- `bash scripts/check.sh` runs every check, the same way CI does: rustfmt, clippy (lints and type checks, `-D warnings`),
  rustdoc, tests, the core dependency rule, cargo-deny (advisories, licences, sources), shellcheck, actionlint and typos.
  A missing tool fails the run; `MIGATE_ALLOW_MISSING=1` runs what is installed.
- CI (`.github/workflows/ci.yml`) runs that script, checks the minimum Rust version (1.89) and runs the PostgreSQL
  tests on 15 to 18; its actions are pinned to commit SHAs. It runs on GitHub's runners, and a run that fails there runs
  again on a self-hosted runner (`docs/ci-runner.md`).
- Every rule migate enforces gets a case it must fail and one it must pass, and is shown failing by deliberate sabotage
  before it is trusted.

**Known local issue (development Mac):** `/opt/homebrew/bin` comes before rustup's `~/.cargo/bin` on the PATH, so a
plain `cargo clippy` in a shell runs Homebrew's newer clippy. The checks and the hook are not affected (they pin the
toolchain themselves); commands typed by hand are. TASK-42 holds the history.

## Next
- The first release, 0.1.0 (`docs/releasing.md`): the work so far is tried on real projects (`docs/end-to-end-testing/`);
  what remains is building the release binary.
