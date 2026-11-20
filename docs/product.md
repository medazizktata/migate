# migate: the product

**A free, offline release gate for database migrations. Before you deploy, it runs the version of your app that is still
live against the database your new migrations produce, and stops the release if the old app would break.** It works with
the migration tool, database and hosting you already use.

Status (2026-11-19): partly built, not released. Built and tested on SQLite: the migration record and order checks, the
throwaway-database run, the old app's tests against the new schema, the schema comparison and lost-row counts. Each
capability below is marked **built** or **planned**; "Where it stands" at the end has the detail.

## The problem

**Most deploys briefly run two versions of your app against one database.** Kubernetes and Fly.io roll out gradually by
default; on Heroku the old release keeps serving while the release phase migrates; Vercel keeps old deployments serving
for 12 to 24 hours; Cloudflare Workers can roll out gradually. Migrations usually run first, so for a while the *old* code talks to the *new* schema.

If the new schema breaks the old code, production fails, even though every test passed. It happens at companies with
large, careful engineering teams:

- **Snowflake, December 2025:** "a backwards-incompatible database schema update"; old release packages "errantly
  referenced the updated fields". 13 hours, 10 of 23 regions.
- **GitHub, August 2025, twice:** a column "no longer in direct use" was dropped, but "our ORM continued to reference the
  dropped column". GitHub then temporarily blocked all column drops.
- **GitLab #5171, severity 1:** a migration left running Rails processes generating "SQL queries invalid with the new
  schema". GitLab's public tracker labels 101 of its production incidents as caused by a database migration.
- **Buildkite, 2026:** a column rename where an AI-assisted workflow added the safety override itself.

Around that core problem sit smaller, everyday ones: an applied migration edited after production ran it, a migration
merged from an old branch that runs out of order (or, with some tools, never runs at all), and production's migration
history drifting from what the repository says.

Sources and the full census of 41 incidents: `research/validation.md`.

## Why existing tools do not solve it

| What exists | What it does | What it misses |
|---|---|---|
| Migration linters (Squawk, MigrationPilot, strong_migrations, django-migration-linter) | Read the migration's SQL or code and flag risky statements | Each covers one database or one framework; reading text cannot see that the *old app* still uses a column, or that an *added* column breaks its queries |
| Your migration tool (Prisma, Drizzle, Payload, Rails, Django, Flyway) | Runs migrations; some keep checksums | Guards only its own history, often only with a warning; none checks the running app |
| Atlas | Schema management with lint and drift checks | Replaces your migration tool; its analysis of triggers, views and most databases needs a paid login |
| DbProof (2026, beta) | Runs your migrations on a copy of production's schema in CI | PostgreSQL and GitHub only, hosted, needs an account, and only warns about a dropped column |

Linting is crowded: hundreds of new "migration linter" repositories appeared in 2026. What nobody offers for free, for
any database and any migration tool, is the release decision itself: **will this migration break the version still
serving traffic, and does it match what production actually ran?**

## Who it is for

- **Product teams that deploy continuously** with a web framework and a migration tool: the people who own the outage
  when a migration breaks the running app, and who do not have a database platform team.
- **Platform and DevOps engineers** who want one gate for every team's stack instead of one linter per framework.
- **Teams that let AI agents write migrations.** Agents generate migrations and safety overrides quickly; the gate checks
  the effect, not the agent's confidence.

## What it does for you

**On every pull request, in CI (no production access):**
1. **Old app, new schema.** migate builds a throwaway database, runs your own migrate command to the pull request's
   version, then runs the still-live version's tests (or loads its models and runs its queries) against it. If the old
   app breaks, the gate fails, and the pull request comment says which test and which change. *Built for SQLite, with
   the tests (not yet the models-and-queries fallback); the tests are first run on the old version's own database and on
   one with its tables dropped, so a run that never touched the database is reported as "not checked". Planned: the
   pull request comment (today the end of the test output is shown) and the pre-boot mode Rails needs.*
2. **A fast schema check.** It compares the schema before and after and names risky changes: dropped or renamed columns,
   NOT NULL without a default, narrowed types, new constraints or triggers. For a dropped column it checks whether any
   live version still uses it, using each framework's own rules (in Rails, a column is in use unless it is listed in
   `ignored_columns`). *Built: the comparison and its rules. Planned: the live-use check for dropped columns.*
3. **The migration record.** It keeps a hash of every migration production has run. Editing or deleting one fails the
   gate; so does quietly rewriting the record. A comment-only edit is told apart from a real one. *Built.*
4. **Order.** A migration merged from an old branch is flagged, as an error where your tool would silently skip it.
   *Built.*

**At release (with a read-only production credential), all planned:**
5. **Does production match the repository?** It reads your tool's own history table and stops on any difference.
6. **Will it fail on real data?** For each risky change it asks production one bounded, read-only question: how many rows
   would violate the new NOT NULL, are there duplicates for the new UNIQUE, is the index you are dropping still used.
   Only counts leave production.
7. **Optional rehearsal** on a copy of production, for teams that can make one: catches data loss and measures how long
   the migration really takes.

**When a breaking change is intended,** you mark it as the "contract" step of an expand/contract change, naming the
object and giving a reason. The override is listed in the pull request comment for a human to see, and it can never
cover a column that a live version still uses. *Built, for the throwaway-database run: a comment in a new migration,
`-- migate: contract drop-column users.legacy -- unused since v42`, turns that one finding into a note; no reason, or no
matching finding, fails (MG901); a failing old-version test can never be accepted. Planned: the pull request comment
that lists accepted contracts.*

An illustrative pull request comment (the format is not built yet):

```
migate: 1 error, 1 warning                                   engine: postgres 17 · migrator: rails

✗ MG212  orders.legacy_code is dropped, but the live version still uses it
          (not in ignored_columns at main@4f2a1c). Ship ignored_columns first, deploy, then drop.
✗ MG810  live version's tests fail against the migrated database:
          test_orders_with_archive — PG::UndefinedColumn: column "legacy_code" does not exist
! MG004  20251231000000_late_branch sorts before the last migration production ran; Rails will run it out of order.
Contracts accepted: none
```

## How it fits into your workflow

1. **Day one, no database needed:** add migate to CI. It records what production ran and checks edits and order. This
   works for every database from the first release.
2. **Then the pull-request gate:** point it at your test command. It runs your old app against the new schema in a
   throwaway database (SQLite in a file, Postgres in a few seconds).
3. **Then the release step:** give the release job a read-only credential for the history comparison and the production
   counts.
4. **Optionally, rehearsal** on a copy of production.

Each step is useful on its own; none requires the next.

## Large databases

**The main check does not touch production data.** The old app's tests run against a throwaway database built from your
migrations, empty or filled by your tests' own fixtures. A 5 TB production database and a 5 MB one give the same
throwaway schema; only a very large schema or a long migration history makes that step slower. The reverse direction
(the new app on the old schema, when code ships before its migration) and the rollback check run the same way.

Size matters in three places, and migate handles each without copying production:

| Question | Why size matters | What migate does |
|---|---|---|
| Will the migration fail on existing rows (NOT NULL on a column with NULLs, UNIQUE with duplicates)? | Counting violations means scanning big tables | Bounded, time-limited, read-only queries at release; statistics before scans (Postgres keeps a NULL fraction per column), sampling, or a read replica. A timeout reports "not checked", never "fine". |
| Will it lock the table or run for hours? | Locks and rewrites grow with table size | The catalog's row estimate (no scan) turns a risky change on a big table from a warning into an error; migrations must set a lock timeout; lock findings from Squawk are imported. Running big changes online is the job of tools like gh-ost, pt-online-schema-change or pgroll. |
| Will it lose or corrupt data (backfills, table rebuilds)? | A full copy for a rehearsal is impractical at terabytes | The rehearsal stays optional and uses a copy-on-write clone (a Neon branch, an Aurora clone, a filesystem snapshot) or a sampled subset. Without one, data-changing statements are to be flagged for human review (planned). Built today: row counts before and after the migrations on the throwaway database, holding what a `seed` command and the old tests wrote, catch rows a migration deletes and values it blanks without deleting a row (a cascade-deleting table rebuild, an
`ON DELETE SET NULL` cascade and an `UPDATE ... = NULL` included). |

**One trap migate refuses to fall into:** a backfill (`UPDATE ... SET ...`) is instant on an empty throwaway database and
can take hours in production. "It ran fast in the sandbox" is never read as safe; data-changing statements are to be
weighed against production's row estimates (planned). Projects with many databases (one per tenant) get the read-only checks per database,
within a time budget.

Not yet measured: all tests so far used small schemas and little data. A spike on tables with tens of millions of rows is
planned before large-database support is claimed.

## Principles

- **It guards your migration tool; it does not replace it.** You keep Prisma, Drizzle, Payload, Rails, Django, Flyway or
  whatever you use; migate runs your own command.
- **Any database, any host.** Support is a plug-in per database and per host, not a promise for one platform. SQLite
  (including Cloudflare D1) and PostgreSQL come first; MySQL/MariaDB and SQL Server follow.
- **Free, offline, no account.** One binary, Apache-2.0. Your schema and data never leave your machines. No paid tier is
  planned.
- **Honest output.** When it cannot check something (a test run that never touched the database, an object kind a reader
  cannot see), it says "not checked", never "fine".
- **Every rule earns its place.** Each rule has a case it must fail and one it must pass, and is shown failing by
  deliberate sabotage before it ships.

## What it is not

- **Not a migration tool:** it never writes migrations or changes how yours run.
- **Not a lock linter:** most migration incidents are locks and timeouts on large tables. migate checks that migrations
  set a lock timeout (Postgres, MySQL), uses production row estimates, and imports findings from Squawk rather than
  re-implementing them. It does not promise zero-lock migrations.
- **Not a zero-downtime executor:** tools like pgroll perform expand/contract for you; migate checks that you did it.
- **Not a backup or rollback tool:** at release it prints the restore point and the exact restore command; it does not
  restore for you.
- **Not for schemaless stores:** MongoDB gets the migration record and history checks only.

## What makes it a better wheel

| | Linters | Atlas | DbProof | migate |
|---|---|---|---|---|
| Runs the *old app* against the new schema | no | no | no | **yes** |
| Catches an *added* column breaking the old app's queries (GitLab #5171) | no | no | no | **yes** |
| Works with your existing migration tool | yes | no (replaces it) | 4 tools | **yes (data files per tool)** |
| Databases | one each | several (most paid) | PostgreSQL | **any, by plug-in** |
| Account or login | no | for most checks | yes | **no** |
| Overrides need a reason | no | no | no | **yes** |
| Compares with production's own history | no | paid | yes | **yes, read-only** |

## Supported first

- **Databases:** SQLite (including Cloudflare D1), then PostgreSQL, both in the first release; MySQL/MariaDB and SQL
  Server after.
- **Migration tools:** Payload, Prisma, Drizzle, Wrangler D1, Rails, Django. Each is a short data file, so adding one is
  usually a file, not code.
- **CI:** any CI that can run a binary; a GitHub Action, SARIF for GitHub code scanning, and one pull request comment
  that is updated, not repeated.
- **Install:** one binary through npm, pip (`migate-cli`), Homebrew and a GitHub Action.

## How we will know it works

- **It catches the real patterns:** the GitHub, GitLab #5171 and CircleCI patterns are reproduced as fixtures and must
  fail; safe changes must pass. (Shown in a spike on Django, Rails and a real Payload app.)
- **Few false alarms:** a safe contract step (a column the old code no longer uses) must pass; every false alarm found
  becomes a fixture.
- **It stays fast enough:** up to three runs of the old version's tests per live version (on its own database first,
  after the migration, and on the database with its tables dropped, so a run that never used the database is caught).
  Measured at 3.5 to 6 minutes on a real Payload app whose integration suite takes about 97 s, seconds on small apps;
  the schema check is under a second. Cutting the two calibration runs is open work.
- **People keep it on:** adoption, and how often teams disable a rule, once it is public.

## Risks we know about

- **Getting noticed.** The space is crowded with new tools; trust and adoption are the hard part, not capability.
- **Teams need tests that touch the database.** The old-app test is only as good as the tests it runs; without them,
  migate falls back to the schema check and model probes and says so.
- **Rails needs a special run** (the old app booted before the migration), which no test runner does out of the box.
- **One maintainer, many integrations.** Keeping migration tools as data files and the core small is the defence.
- **Only partly tested so far:** MySQL, SQL Server, production D1, real CI runners and large databases.

## Where it stands (2026-11-19)

| Part | State |
|---|---|
| Research: the gap, validation against 41 incidents, a blind review of the decisions | done (`research/`) |
| Spikes: real migration tools in a throwaway database; the old-app test on Django, Rails and Payload | done (`research/spike-7.md`, `research/spike-31.md`) |
| Migration record and order rules, six migration-tool definitions | built and tested in the core |
| `migate check` and `migate ledger record`: the migration record and order checks on a real repository, no database needed | built and tested (shown on a clone of a real Payload app) |
| The SQLite schema reader (and a throwaway SQLite database), producing the shared schema model | built and tested; several database files and deferrable foreign keys included |
| The schema diff and 23 breaking-change rules (drops, NOT NULL, narrowing, defaults, constraints, views, triggers, enums) | built and tested in the core |
| `migate check` with `[sandbox]`: migrate a throwaway SQLite database at base and HEAD, diff, run the base version's tests | built and tested (fresh mode; Rails needs the pre-boot mode, not built) |
| Test runs proven to use the database (MG813); rows lost by the migrations (MG105) | built and tested |
| A blind test drive on real Django, Rails, Drizzle, Prisma and D1 projects | done (`research/test-drive-1.md`); its findings are being fixed |
| The PostgreSQL engine: a throwaway local server per run and a catalog reader, behind an engine plug-in interface | built and tested on PostgreSQL 18 (partitions and extension objects skipped) |
| The pre-boot runner, production checks, PR comment | planned (backlog) |
| First public release | not yet |

Name: migate. Licence: Apache-2.0. Language: Rust.
