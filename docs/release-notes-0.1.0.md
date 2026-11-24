# 0.1.0 release notes (draft)

The words for the `CHANGELOG.md` section that `just release 0.1.0` creates (step 3 of `releasing.md`): that command
lists commit subjects, which for a first release are too many to read. Replace its list with this, and remove this
file in the release commit.

## migate 0.1.0

The first release of migate, an offline release gate for database migrations: it blocks a change whose migrations
would break the version still serving traffic, or lose data, before it deploys.

**What it checks.**
- **Without a database**, from the repository and its history: a migration production already ran that was edited
  or removed (with a ledger), migrations out of order for the project's migration tool, a dependency graph with
  several heads, two migrations sharing a version (MG001-MG007).
- **With a `[sandbox]`**: migate migrates a throwaway database to the base branch and to the change, then runs the
  base version's own tests against the migrated database (the primary check: what the running version does, not
  what a diff suggests), compares the two schemas (drops, renames, NOT NULL, narrowing, constraints, views,
  triggers, routines, enums), counts rows and values lost, flags a migration that empties a table outright, checks
  whether the base version's code still uses a dropped column, and reads syntax newer than production's engine. Every
  rule, 48 in all, is listed in `crates/core/src/rules.rs`.
- **At release**, `migate history` compares production's migration history with the repository and the ledger.
- An intended breaking change is accepted with a `migate: contract` comment that must give a reason; the evidence
  rules (a failing test, a failed migration) accept no marker.

**Engines and tools.** SQLite (including Cloudflare D1, locally) and PostgreSQL 15 to 18. Migration tools, as data
files: Payload, Prisma, Drizzle, Wrangler D1, Rails, Django, Supabase and dbmate. A binary for x86-64 Linux.

**How it was tested.** Beyond its test suite, every check was tried on real projects (`end-to-end-testing/`): a
Payload app on D1 over 36 cases, sample apps for each tool on both engines, real PostgreSQL projects, documenso run
with its own API tests on four of its changes (both breaks it shipped caught, both safe changes passed), and the history
of eight open-source projects; findings on projects without runnable tests were labelled by independent reviewers.
Where tests decide, the latest runs had no missed break and one false alarm: a CHECK added to an existing column
that the old version's writes satisfy (a known limit, below).

**Known limits.**
- The sandbox needs the project's own migrate command, and the base version's tests to run offline against a database
  migate starts.
- A schema rule cannot know what the running app writes: a CHECK added to an existing column, or a dropped table that
  only mirrors another, is reported and needs a marker.
- A routine whose JSON result changes shape, a DELETE inside a PostgreSQL `DO` block, and revoked privileges are not
  detected. A row-level security change is named (MG801) but not judged: the tests run as a superuser.
- No Windows or macOS binary yet; MySQL and SQL Server are not supported.
