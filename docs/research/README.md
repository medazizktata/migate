# Research: the gap, the architecture, the stack

Date: 2026-11-19. Scope: **any migrator, any database, any host** (the owner's direction). D1 is one host of the SQLite
engine, the way RDS is one host of PostgreSQL; it may be among the first plug-ins, and it is not the product.

| File | What it answers |
|---|---|
| `gap.md` | Who else does this, where each stops, what users complain about, what would make this a better wheel, market size |
| `architecture.md` | Components, plug-in interfaces, the canonical schema model, the rule list, throwaway databases, hosts, the ledger |
| `stack.md` | Which language, measured: start-up, schema reading, throwaway Postgres, drivers, parsers, distribution |
| `validation.md` | Is the problem real and weighty: 41 classified incidents, GitLab's 101 migration incidents, the case against, kill criteria |
| `blind-review.md` | A context-free agent's review of the M0 decisions, what was verified, and the owner's decisions after it |
| `first-pass.md` | The first round (landscape, 21 migrators' history tables), with corrections at the top |
| `bench/` | The benchmark and probe sources behind the numbers, and the raw results |

**How this was made.** Three research agents ran in parallel (gap, architecture, stack), each told to label claims V
(read the primary source or ran it), R (relayed) or U (unknown), and to calibrate every check both ways. The main session
then re-checked what the conclusions rest on; each report ends with a "Checked by the main session" section listing
exactly what was re-run and what is relayed.

## 0. Validation verdict (`validation.md`)

Kill criteria were fixed before searching; an incident census and a devil's advocate then ran in parallel.

- **The problem is real and has weight.** 41 classified public incidents (2015-2026); GitLab alone labels 101 of its
  production incidents as caused by a database migration. Demand for free checks is flat-to-growing.
- **The costly, repeated class is breaking the app version still running:** Snowflake (13 h, 10 of 23 regions), GitHub
  twice in August 2025, Deno, Buildkite, Val Town, GitLab.
- **In those incidents the drop was intended; the running code still referenced the column.** A schema diff plus a
  contract marker would have let GitHub's and Buildkite's migrations through. The check must ask "is the old app still
  using this?"
- **Most migration incidents are locks, timeouts and load** (7 of 10 at GitHub), which a schema diff cannot see.
- **Edited and misordered migrations are friction more than outages.** One company postmortem (Tuist 2026).
- **Small but new: AI agents writing migrations** (Buildkite's override was added by an LLM).

So the design changes proposed in `validation.md` §3 lead the product with the N-1 gate, make it code-aware, harden
contract markers, handle locks by integrating existing linters plus production checks, and make the order rule per
migrator. The owner approved them on 2026-11-19, with the corrections from `blind-review.md`, which also made a
mixed-version test the primary check (section 6).

## 1. The gap: real, but narrower than "a migration linter"

- **No open, free tool gives all five protections for any migrator, engine and host** (P1 frozen, P2 in order, P3
  matching production, P4 not breaking the running app, P5 one ordered release).
- **Linting is saturated and a frozen ledger is becoming common.** 821 repositories from 15 GitHub searches, most created
  in 2026 with 0-5 stars; new entrants get tens to a few thousand npm downloads a month against squawk-cli's 7.4M.
  Another regex linter would not be a better wheel.
- **What stays open is the release decision:**
  - does the pull request's migration set match what production actually ran (P1-P3)?
  - will its schema change break the app version still serving traffic (P4), unless a contract step with a stated reason
    allows it?
- **Nobody answers that for any engine.** Atlas's open inspectors cannot see triggers or views (checked in source);
  Prisma's `migrate diff` is blind to both (its docs); migrations-safe says the N-1 app is "Not covered" (its docs);
  DbProof, the closest entrant, is PostgreSQL and GitHub only, hosted, needs an account, and flags a dropped column as a
  warning (its site).
- **Market weight favours an engine-neutral identity.** PostgreSQL 55.6%, MySQL 40.5%, SQLite 37.5%, SQL Server 30.1%
  of developers (Stack Overflow 2025). Payload's D1 adapter is about 1/22 of its Postgres adapter in downloads.

**Positioning:** a free, offline release gate that stops a migration from breaking the app version still running, or
from drifting from what production ran, for the migrator and database you already use.

## 2. What makes it a better wheel

1. **An N-1 gate from a schema diff**, triggers, views and constraints included, found by running the user's own migrate
   command on a throwaway database, not by reading SQL text.
2. **Guards the migrator instead of replacing it** (Atlas, pg-schema-diff and others replace it).
3. **A read-only comparison with production's own history table, for any migrator** (today only Flyway and Liquibase do
   this, each for its own table, plus DbProof for Postgres).
4. **Contract markers that must name the object and give a reason**, with stale markers failing. strong_migrations
   declined this (issue #257); none of six linters studied requires a reason.
5. **No account, works offline, any CI.** Atlas's paid features and DbProof both need an account.
6. **Explains why and names the expand/contract fix**, as strong_migrations does.
7. **Honest blind spots:** a reader that cannot see some object kind reports "not checked", never "no change".

Leave lock-and-slowness linting to Squawk and similar tools, and ingest their SARIF output.

## 3. The architecture

- **A small core** (ledger, canonical schema model, diff, classifier, reporters, release runner) and **three kinds of
  plug-in**, kept apart: the **engine** (read a schema, start an empty copy), the **host** (reach production's history,
  take a restore point) and the **migrator** (history query, order model, migrate command).
- **Engines and hosts send SQL through a transport** (a driver URL, a command such as `wrangler d1 execute --json`, or
  HTTP), so a new host is configuration, not code.
- **Migrators are 10-20 line declarative files**; the order model comes from a closed set in the core (lexical,
  timestamp, graph for Django/Alembic/Sqitch, single version, and a flag for migrators that skip silently).
- **Out-of-tree plug-ins are subprocesses speaking JSON on stdin and stdout**, as protoc and sqlc plug-ins are.
- **Our own canonical schema model**, published as JSON Schema, with each reader declaring what it can see.
- **The gate:** one throwaway database, two checkouts (merge base, then the PR head), a schema read after each, a diff,
  and about 40 permanent rule IDs scored on four axes (breaks the old app, locks, loses data, may fail on existing rows).
- **The ledger:** sha256 over line-ending-normalised files, append-only against the base branch, with a tool-owned
  production table as the authority. Production credentials never reach pull-request CI.
- **Outputs:** exit codes (3 means "could not check"), JSON, SARIF, one updated PR comment, a GitHub Action.

## 4. The stack: Rust, with Go a close second

- **Performance is not the deciding factor.** Measured on this Mac, a full Postgres check takes about 1.0-2.5 s, almost
  all of it the throwaway database and the user's migrate command. Rust or Go versus Node changes the total by about
  0.1 s (3-10 %). The real savings are design choices: one database for both migrate runs, Postgres started from a cloned
  template, no `npx`.
- **Rust wins on the parts that differ:**
  - **sqlparser-rs** is the only single SQL parser covering Postgres, MySQL, SQLite and SQL Server (checked: dialect
    files for all four, Apache-2.0). SQLite and D1 offer no out-of-process statement capture, so their data migrations
    must be found by parsing.
  - **Free distribution to every ecosystem:** cargo-dist and maturin ship one binary through npm, pip and Homebrew, as
    Squawk and ruff do. Go's GoReleaser needs its paid Pro edition for npm (checked on its docs page).
  - **Start-up and size are a tie with Go:** about 5 ms and about 8 MB with drivers for all four engines.
- **Go is right if** SQL parsing stays secondary or Postgres-only and easier contributing matters more (pure-Go drivers
  cross-compile from one machine).
- **TypeScript is right only if** in-process PGlite or Miniflare is required, or rules must be JavaScript. PGlite was
  measured no faster than a real Postgres (1.4-1.5 s against 1.4 s for a cloned native cluster).
- **Rules and migrator adapters as data files** keep contributing easy whatever the language.

**Correction to this session's earlier advice:** TypeScript was recommended earlier on 2026-11-19, on the premise that
the first users run Node and that in-process emulators would help. Both premises fail for an any-database tool: most of
its users are not on Node, the user's migrate command runs as a separate process anyway, and PGlite is not faster.

## 5. Not verified (state plainly)

- **No real migrator has been run in a throwaway database yet** (Payload, Prisma, Rails, Django). Only Postgres with a
  toy migrator and node-pg-migrate are proven. This is the top risk of the whole design.
- **MySQL, MariaDB and SQL Server** are designed on paper and not measured (no local binaries; no image pulls).
- **Production D1:** whether it accepts the table-valued pragma functions, and whether Payload's own D1 migrations run
  atomically.
- **Times come from one busy Mac**, not a Linux CI runner.
- **Competitor behaviour** is from their READMEs, docs and source, not from running them.
- **npm download counts for 2026 are 2-20x the 2025 numbers, for unknown reasons**; they are used only as ratios.

## 6. Decisions

**Taken by the owner (2026-11-19), after a blind review (`blind-review.md`):**
- **Independent and pluggable into any system.** No vendor is a dependency of the core.
- **The general solution first.** A GFS-dedicated feature (for example a rehearsal on a `gfs clone <source> <target>`
  full copy, upstream GFS PR #201, still open) comes only once the core works for most databases, as a fork or an
  optional adapter.
- **The primary check is a mixed-version test:** migrate a throwaway database to the new head, then run every live
  version's tests (or at least load its models and run its queries) against it. The schema diff is the fast first pass,
  the fallback where a team has no tests, and the source of the PR comment. A rehearsal on a production copy is the main
  defence against data loss.
- **The design changes in `validation.md` §3 are approved, with the blind review's corrections** (`blind-review.md` §2):
  per-ORM reference rules (Rails: in use unless in `ignored_columns`), all live versions, rollback compatibility,
  comment-only edits judged per migrator, Squawk imported from JSON/GitLab output (it has no SARIF), timeouts for
  Postgres and MySQL only, head-only history comparison for Alembic and golang-migrate, plus index-usage, silent-skip,
  row-count, cascade-rebuild and multi-tenant checks.
- **Rust**, shipped through npm, pip, Homebrew and a GitHub Action.
- **Apache-2.0; no revenue,** pure open source (sustainability noted as a risk).
- **Engine order:** SQLite (including D1) first as the build and dogfood target, PostgreSQL next (both in the first
  release; Postgres leads the public launch), then MySQL/MariaDB, then SQL Server; MongoDB gets the ledger and history
  layers only.
- **Name: `migate`.** Free on npm, crates.io and Homebrew; taken on PyPI, so the pip package is `migate-cli`.

The repository folder was renamed from `migration-guard` to `migate` the same day.

**Ideas from the discussion, not yet in `architecture.md`:**
- **Read-only production checks at release:** for each risky change, one count through the read-only connection the
  tool already uses (rows where a new NOT NULL column is NULL, duplicates for a new UNIQUE, orphans for a new foreign
  key, the longest value for a narrowed type, the catalog's row estimate for lock risk). Counts only; no data leaves
  production; time-limited.
- **Three levels of sandbox:** an empty throwaway database plus the production checks by default; an opt-in rehearsal on
  a copy of production (`pg_dump`/`pg_restore`, a restored host snapshot, a Neon branch, later a GFS full clone); the
  report says which level produced each finding.
- **Hexagonal edges around a functional core:** ports only where two real implementations or a test double exist;
  migrators stay data files; a CI check keeps database drivers out of the core crate.

## 7. Next steps that cut the most risk

1. Run three real migrators (Payload, Prisma, Django or Rails) through the two-checkout sandbox on SQLite and Postgres.
2. Measure Postgres, MySQL and SQL Server throwaway start-up on a Linux GitHub runner.
3. Ship P1-P3 first: they need no sandbox and work for every engine on day one.
