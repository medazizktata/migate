# Transcript: generalising the migration guard to any database

Exported from Claude Code session a58d04c9 on 2026-11-19, starting at the question that started this project. Tool calls and their raw output are left out; the user's messages, Claude's replies and the research agents' reports are kept.

## User

<pasted_content id="4c6f">
Why no tool can be plugged in. Our migrations are Payload's TypeScript files. Payload records only each one's name and batch, and D1 is reachable only through Cloudflare's API.
- Flyway and Liquibase keep checksums, but they need a direct database driver (JDBC), which D1 doesn't offer.
- Prisma Migrate and Drizzle Kit would replace Payload's migrator, against our earlier decision (ADR 0011).
- Atlas is the closest. It has an integrity file and checks for destructive changes, and it supports SQLite. But it reads plain SQL files, I found no D1 support, and its migration linter has needed a paid plan since v0.38.
- Squawk is PostgreSQL only.
</pasted_content id="4c6f">

 can a tool be developed that is plug and play for any db, any target infra ?

## Claude

Yes, mostly. A tool can work with any migrator, any database and any hosting if it's built in layers, with only a thin part tied to each database. "Any database" stops being fully true at the step that judges whether a change is safe.

**Layer 1: works everywhere unchanged.** This is what we built for this repo, and it needs no database access at all:
- record each migration's content hash when production runs it, and fail if a recorded file changes or disappears;
- refuse a new migration dated before the last one production ran;
- one ordered release command.

It only reads migration files and a record kept in git. It doesn't care whether a migration is SQL, TypeScript (Payload, Knex, Drizzle) or anything else.

**Layer 2: comparing with production's own history.** Every migrator keeps a table of what it ran under a different name: Payload's `payload_migrations`, Supabase's `schema_migrations`, Prisma's `_prisma_migrations`, Flyway's `flyway_schema_history`, Wrangler's `d1_migrations`. An adapter is one query plus one way to run it: `wrangler d1 execute` for D1, `psql` for Neon or RDS, a URL for others. A few lines per migrator.

**Layer 3: judging whether a migration breaks the live app.** This is the part tied to each database, and the hard part. Reading the SQL text, as Atlas, Squawk and my regex do, breaks on TypeScript migrations and on every SQL dialect. A sturdier design:
1. Run the new migrations on a throwaway copy of the schema: SQLite in memory, or Postgres in a container.
2. Read the schema before and after (`sqlite_master`/`PRAGMA` for SQLite, `information_schema` for Postgres and MySQL).
3. Classify the difference: a dropped or renamed column, a narrowed type, a new NOT NULL column with no default, a rebuilt table.

That judges the effect, not the text, so it works however the migration is written. Each database engine needs one schema reader. SQLite, Postgres and MySQL cover nearly every real project.

**What no tool can promise:**
- Data changes (an `UPDATE` that rewrites values) can't be judged safe automatically.
- Schemaless stores like MongoDB have no schema to compare.
- Rename detection is a guess: a dropped column next to a new one looks like a rename.
- Whether a change is safe also depends on what the running code reads, which needs knowledge of the app.

**Is it worth building?** From the research, Atlas is the closest existing product, but its safety linter is paid since v0.38 and it reads SQL files only. I found no open tool that works with any migrator, so there seems to be a real gap.

Rough effort:

| Part | Size |
|---|---|
| Layer 1 | Exists (about 80 lines here); a few days to make it a package with a config file |
| Layer 2 | A few hours per migrator |
| Layer 3 for SQLite (us) and Postgres (to prove it generalises) | 1–2 weeks |

My recommendation: extract what we have into a standalone package with the adapter interface. Then add the schema-diff check for SQLite first, since it would replace my regex here, and Postgres second. Want me to start? If so, should it live in this repo as a package, or in its own repository so other projects can use it?

## User

You should research this even more. I'm thinking about branching it to be a separate project.

## Claude

I'll split the research across two agents running in parallel, so each can go deep:
- **A, the landscape:** every existing tool and platform feature that protects migrations, what each covers, its licence and price, how much it's used, and where the gaps are.
- **B, technical feasibility:** how each migrator records what it ran, how to read each database's schema, how to detect breaking changes reliably, how to spin up throwaway databases, and how such a tool would plug into CI.

I'll then combine both into one report with a proposed project shape.

## Claude

Both research agents are running:
- **A, the landscape:** about 20 tools and platform features (Atlas, Bytebase, Flyway, Liquibase, Squawk, pgroll, Supabase, PlanetScale, Neon and others), what each protects, licences and prices, adoption, practitioners' complaints, and where the open gaps are.
- **B, technical feasibility:** how about 20 migrators record what they ran, schema reading per database, detecting breaking changes reliably (parsing SQL versus comparing schemas), throwaway databases, packaging and CI. It will also judge whether the small check I built here generalises.

When they report, I'll check their key claims and combine them into one report with a proposed project: scope of a first version, architecture, licence, and what would make it worth building.

## Research agent report

#### Design assessment: a generic tool that guards migrations in production

Legend: **V** means I read the primary source or doc myself. **R** means relayed from a secondary source or from memory. **U** means unknown. I modified nothing. The one thing I ran was a read-only Node probe in the scratchpad that imports the local check's functions (in the session's scratchpad).

##### 0. The local data point (the origin project's own migration checks)

What is sound: freeze by content hash, order by lexical timestamp, a history comparison at release time, and one ordered release.

What I measured with the probe (V; each case was checked against a case that fires):
- **The ledger can be rehashed in the same pull request.** I edited an applied migration with an honest ledger and got 1 problem. I then updated its hash in the ledger file in the same change and got **0 problems** from CI and **0** from the release's `historyProblems`. The reason is that the history check compares names only, and `payload_migrations` stores no hash. Production would then silently differ from the repo.
- **What the regex misses** (0 problems each): an `ADD … NOT NULL` with no default, `DROP VIEW`, a new trigger that rejects the old app's writes, and dynamic SQL (`sql.raw('DROP '+T)`).
- **A false positive:** an arrow-function `up` gives a slice start of -1, so the slice includes `down()` (line 38-40) and the check flags a safe migration.
- The hash covers the TypeScript source, not the SQL it emits. The release step records every file, assuming everything ran, instead of reading production's table after the migrate step.

**How well it generalises:** the ideas carry over. The regex detector and the trust in a committed ledger do not.

##### 1. Migration history tables

| Migrator | History table (columns) | Hash or statements stored | Out-of-order and edits | Source |
|---|---|---|---|---|
| Flyway | `flyway_schema_history` (installed_rank, version, description, type, script, checksum, installed_by, installed_on, execution_time, success) | checksum, an integer (CRC32 per R) | `validate` fails on a mismatch; `repair` resets; out-of-order off by default (R) | [docs](https://documentation.red-gate.com/fd/flyway-schema-history-table-273973417.html) V for behaviour; columns R |
| Liquibase | `DATABASECHANGELOG` (ID, AUTHOR, FILENAME, DATEEXECUTED, ORDEREXECUTED, EXECTYPE, MD5SUM, …, DEPLOYMENT_ID) | MD5SUM of the changeset, not the file | Halts on a mismatch; escapes are `validCheckSum`, `runOnChange`, `clear-checksums` | [table](https://docs.liquibase.com/concepts/tracking-tables/databasechangelog-table.html), [checksums](https://docs.liquibase.com/concepts/changelogs/changeset-checksums.html) V |
| Sqitch | `sqitch.changes` (change_id, script_hash SHA-1, …), plus events, tags, dependencies | yes | Plan order plus declared dependencies | `lib/App/Sqitch/Engine/pg.sql:36-53` V |
| Rails | `schema_migrations(version)` | no | Pending means "not in the table", so out-of-order migrations run | `activerecord/lib/active_record/migration.rb:1521-1523` V |
| Django | `django_migrations(id, app, name, applied)` | no | Dependency graph; `InconsistentMigrationHistory` | `recorder.py:32-40`, `loader.py:344-361` V |
| Alembic | `alembic_version(version_num)`, heads only, no history | no | Revision graph | `alembic/ddl/impl.py:161-175` V |
| Knex | `knex_migrations(id, name, batch, migration_time)`, plus a lock table | no | Errors if an applied file is missing ("directory is corrupt") | `table-creator.js:34-50`, `Migrator.js:734` V |
| TypeORM | `migrations(id, timestamp, name)` | no | The out-of-order check is **commented out** | `packages/typeorm/src/migration/MigrationExecutor.ts:232-234` V |
| Sequelize (umzug) | `SequelizeMeta(name)` | no | — | `umzug/src/storage/sequelize.ts:47-54` V |
| Prisma | `_prisma_migrations` (id, checksum, migration_name, started_at, finished_at, logs, rolled_back_at, applied_steps_count) | checksum | On an edited file, `dev` errors but `deploy` **only warns** | `sql-schema-connector/src/flavour/sqlite.rs:196-198`; [docs](https://www.prisma.io/docs/orm/prisma-migrate/understanding-prisma-migrate/migration-histories) V |
| Drizzle Kit | `__drizzle_migrations(id, hash, created_at)` | sha256 of the SQL, **never compared** | Applies only `folderMillis > last created_at`, so an older-dated migration is **silently skipped forever** | `drizzle-orm/src/migrator.ts:52`, `pg-core/dialect.ts:88-107` V |
| Payload | `payload_migrations(name, batch)`; batch -1 means a dev push | no | Runs whatever has not run | `packages/payload/src/database/migrations/migrationsCollection.ts` V; order R (the origin project's notes) |
| Supabase CLI | `supabase_migrations.schema_migrations(version, name, statements text[])` | **statements** | Two-pointer reconciliation of remote and local versions | `apps/cli/src/command-internal/migration-history.ts:29-58,167` V |
| Wrangler D1 | `d1_migrations(id, name UNIQUE, applied_at)` | no | Pending means "file not in the table", so out-of-order migrations run | `wrangler/src/d1/migrations/helpers.ts:146-152,241-250` V |
| golang-migrate | `schema_migrations(version, dirty)`, current version only | no | A lower version added later never runs (R) | `database/postgres/postgres.go:355-374` V |
| goose | `goose_db_version(id, version_id, is_applied, tstamp)` | no | **Refuses** out-of-order unless `-allow-missing` | `internal/dialects/postgres.go:20-23`; [README](https://github.com/pressly/goose) V |
| dbmate | `schema_migrations(version)` | no | Applies out-of-order unless `--strict` | [README](https://github.com/amacneil/dbmate) V |
| Atlas | `atlas_schema_revisions` (hash columns R), plus an `atlas.sum` Merkle file in the repo | yes | `--exec-order linear` (the default) fails on out-of-order | [apply](https://atlasgo.io/versioned/apply), [integrity](https://atlasgo.io/concepts/migration-directory-integrity) V |
| Ecto | `schema_migrations(version bigint, inserted_at)` | no | Runs pending migrations (R) | `ecto_sql/lib/ecto/migration/schema_migration.ex:10-34` V |
| Laravel | `migrations(id, migration, batch)` | no | — | `DatabaseMigrationRepository.php:50-82` V |
| EF Core | `__EFMigrationsHistory(MigrationId, ProductVersion)` | no | Pending means "not in the applied set" | `HistoryRepository.cs:31`, `Migrations/Internal/Migrator.cs:523-534` V |

All of these can be read with one `SELECT` on the table. Of the 21 migrators, only 7 record a hash or the statements: Flyway, Liquibase, Sqitch, Prisma, Drizzle, Atlas and Supabase. The other 14 need the tool's own ledger.

The table also shows that "pending" means something different in each migrator: lexical order, semantic version, graph, or a single current version. **Out-of-order checking cannot be one generic rule.** It needs a per-migrator model. For graph-based migrators (Django, Alembic, Sqitch) it should be a graph-consistency check.

##### 2. Running another tool's migrations on a throwaway database

The generic mechanism is a config entry such as `migrate: "<cmd>"` with `DATABASE_URL={shadow}` injected.

What makes this hard:
- **Migrations written in code** (Payload, TypeORM, Knex, Django, Rails, EF) need the app's runtime, config and dependencies. For example, Payload migrations import `@payloadcms/db-d1-sqlite` and run through `payload migrate`.
- **D1** is reachable only over an HTTP API or locally through Miniflare (`wrangler d1 migrations apply --local`). The location of the local SQLite file is an implementation detail (R).
- **Permissions:** Prisma's own shadow database needs `CREATEDB` or a pre-made `shadowDatabaseUrl` ([docs](https://www.prisma.io/docs/orm/prisma-migrate/understanding-prisma-migrate/shadow-database) V).

Throwaway engines:
- SQLite in memory.
- PGlite: Apache-2.0 and PostgreSQL licence, under 3 MB gzipped ([pglite.dev](https://pglite.dev/) V).
- embedded-postgres for Go (MIT).
- Testcontainers (MIT) or Docker MySQL.

**Startup times: U.** I measured nothing. They should be measured on the CI runner before anyone relies on them.

A precedent for the approach: eugene's `trace` mode runs a migration in a transaction against a real Postgres and reports the locks it takes ([repo](https://github.com/kaaveland/eugene) V).

##### 3. Reading the schema

| Engine | Introspection | Reusable library (licence) |
|---|---|---|
| SQLite | `sqlite_schema` plus `pragma_table_xinfo`, `index_list`, `index_xinfo`, `foreign_key_list`. Partial-index `WHERE` clauses, triggers and generated expressions exist only in the stored `sql` text, so parse it. | Atlas `sql/sqlite` (Apache-2.0; uses `JOIN pragma_table_list()`, `inspect.go:656-661` V); sqlite-utils (Apache-2.0); Kysely introspection (MIT, tables and columns only, R) |
| D1 | The same PRAGMAs are supported and `sqlite_master` can be queried ([docs](https://developers.cloudflare.com/d1/sql-api/sql-statements/) V). Reached through `wrangler d1 execute --json` or the REST API. Whether **table-valued pragma functions work on D1 is U**, so Atlas's inspector may not run over D1 unchanged. | As for SQLite |
| libSQL/Turso | SQLite-compatible (R) | As for SQLite |
| PostgreSQL | `pg_catalog`: `pg_class`, `pg_attribute`, `pg_constraint`, `pg_index`, `pg_trigger`, `pg_proc`, `pg_enum`. `information_schema` misses indexes and triggers in detail. | Atlas `sql/postgres` (Apache-2.0); stripe/pg-schema-diff (MIT); supabase/pg-delta (PostgreSQL licence); migra (Unlicense, deprecated); libpg_query (BSD-3) |
| MySQL/MariaDB | `information_schema` `COLUMNS`, `STATISTICS`, `TABLE_CONSTRAINTS`, `TRIGGERS`; `SHOW CREATE TABLE` | Atlas `sql/mysql` (Apache-2.0) |
| SQL Server | `sys.tables`, `sys.columns`, `sys.indexes`, `sys.foreign_keys` | **No SQL Server driver in Atlas's open repo.** `sql/` holds only mysql, postgres and sqlite (V). sqlglot (MIT) parses only. |

Licences were checked via the GitHub API (V). The parsers are sqlglot (MIT), sqlparser-rs (Apache-2.0) and node-sql-parser (Apache-2.0); Prisma's engines are Apache-2.0.

**Atlas's analyzers are open source.** `sql/sqlcheck/{destructive,incompatible,datadepend,condrop}` are in the Apache-2.0 repo (V). Only the `migrate lint` *command* moved to Pro in v0.38 ([lint docs](https://atlasgo.io/versioned/lint) V; [releases](https://github.com/ariga/atlas/releases) R). The origin project rejected Atlas for that reason, but its library is still reusable.

Engine traps (all V):
- SQLite `ADD COLUMN NOT NULL` needs a non-NULL default.
- SQLite `DROP COLUMN` fails if the column is indexed or referenced by a trigger or view.
- Anything else needs the 12-step table rebuild. SQLite 3.53 adds `ALTER COLUMN SET/DROP NOT NULL` ([SQLite docs](https://www.sqlite.org/lang_altertable.html)).

##### 4. Detecting changes that break the running app

**What I would do instead of the regex:** diff two shadow databases.
1. Migrate a shadow database to the base branch, introspect it, and take snapshot A.
2. Apply the pull request's pending migrations, introspect again, and take snapshot B.
3. Diff A against B.

This works for any migrator (it runs through the user's command), sees dynamic SQL, and covers views, triggers and functions.

**Rename versus drop-plus-add does not need to be distinguished for this gate.** Both break the old version of the app. The difference matters only for the message and for data loss, and parsing an explicit `RENAME` statement covers that.

Use SQL parsing (sqlglot, pg_query, sqlparser-rs) only as a secondary signal:
- data migrations (`UPDATE`, `DELETE`, backfills): flag them for review, because a schema diff cannot see them;
- lock and slowness hints.

Breaking for the old app:
- a dropped or renamed table, column or view;
- a narrowed type;
- a new `NOT NULL` column without a default (old inserts fail);
- **new CHECK, UNIQUE or FK constraints, or new triggers that reject the old app's writes.** The brief's list omits these, and they matter in a repo whose rules live in triggers.

Slow or locking but not breaking:
- **Postgres:** `ALTER TABLE` defaults to ACCESS EXCLUSIVE. A type change rewrites the table. `SET NOT NULL` scans the table unless a valid CHECK constraint proves it. Use `NOT VALID` then `VALIDATE` ([PG docs](https://www.postgresql.org/docs/current/sql-altertable.html) V).
- **MySQL 8.4:** a type change is COPY-only with no concurrent DML; add and drop column can be INSTANT ([MySQL docs](https://dev.mysql.com/doc/refman/8.4/en/innodb-online-ddl-operations.html) V).
- **SQLite and D1 rebuilds:** whether Payload or D1 runs the copy in one transaction is **U**. If it does not, the window between `DROP TABLE` and the rename breaks the live app.

Literature and prior art:
- Parallel change (expand, migrate, contract) by Danilo Sato, 2014; "Most database refactorings follow the parallel change pattern" ([Fowler](https://martinfowler.com/bliki/ParallelChange.html) V).
- strong_migrations (MIT): the `ignored_columns` → deploy → drop sequence, and `safety_assured` ([repo](https://github.com/ankane/strong_migrations) V).
- Squawk (Apache-2.0, Postgres only) ([rules](https://squawkhq.com/docs/rules) V).
- pgroll (Apache-2.0): versioned views, triggers for dual writes, then `complete` or `rollback`; Postgres 14 and later ([xata](https://xata.io/blog/pgroll-expand-contract) R).

##### 5. Packaging and integration

**Recommendation: one Go binary that orchestrates over Atlas's open packages** (inspect, diff, sqlcheck).
- Ship a GitHub Action and an npm or pip wrapper.
- Cost: there is no SQL Server, and pg_query_go needs cgo.
- Alternatives: a TypeScript build gains PGlite and Miniflare in-process but loses breadth. A Python build gains sqlglot but has a heavy runtime.

**Config shape:**

```
engine: sqlite|d1|postgres|mysql
migrator: payload|prisma|…
dir:
migrate: "<cmd using $DATABASE_URL>"
history: {table, query}
ledger: path
contract_marker:
```

**The record of what production ran: use both sources.**
- The committed ledger is the CI-side cache. It must be **append-only against the base branch** (`git show origin/main:ledger`). That closes the rehash bypass above.
- The authority is a tool-owned table in production, `<tool>_ledger(name, sha256, commit, released_at)`, written by the release and read at release time with a **read-only** credential.
- Production credentials stay out of pull-request CI. Use GitHub environment-scoped secrets, mask them, and never echo connection URLs (R).
- For the 7 migrators that store hashes or statements, call their own `validate` or compare their column instead.

##### 6. A minimal first version

1. The freeze ledger with append-only verification (works for any migrator).
2. Per-migrator pending and order models for Payload, Prisma, Drizzle, Wrangler D1, Rails and Django.
3. The shadow-diff gate on SQLite/D1 and Postgres (PGlite or a container).
4. The history comparison at release.

MySQL and SQL Server come later.

Tests:
- Golden fixtures per migrator: a real repo of 3–4 migrations, with one known-bad case per rule.
- A matrix of engines.
- A sabotage run for every rule: show it red on the bad fixture, then green once restored.

##### Main technical risks, ranked

1. **Running arbitrary migrators against a shadow database.** Code migrations need the app runtime; Prisma needs `CREATEDB`; D1 needs Miniflare. This is the whole generality claim and the most work.
2. **Trust in the ledger.** The rehash bypass gave 0 problems in the probe, and 14 of 21 migrators store no hash.
3. **Silent skips in the migrators themselves.** Drizzle skips older migrations forever (`dialect.ts:94-99`), TypeORM's check is commented out, and golang-migrate keeps a single version. A single generic order rule gives false confidence.
4. **D1 and libSQL introspection over HTTP.** Support for pragma functions is U, and Atlas relies on `pragma_table_list()`.
5. **False negatives in text detection.** Triggers, views and dynamic SQL gave 0 problems in the probe. The schema diff fixes this, but not data migrations.
6. **Atlas licence drift.** The lint command is already Pro, and the open core could shrink further.

This brief pushed me toward breadth (21 migrators) and toward a critique of the local regex. Its no-execution rule kept me from measuring shadow-database startup, D1 pragma support and transaction behaviour, so those remain unknown. I also did not verify the Flyway and Atlas column lists or the CRC32 claim against source.

## Claude

Agent B is back, with one finding that matters for this repo right away: the migration freeze I built can be bypassed. I'll reproduce its main claims myself before relying on them:

## Research agent report

### Production migration safety tools: landscape as of 2026-11-19

**How to read the labels.** V means I read the primary source (docs, pricing page, source code, GitHub API). R means I'm relaying a secondary source. U means unknown. All star counts come from the GitHub API on 2026-11-19. All download counts cover 2026-10-19 to 2026-11-17.

**Depth of coverage.**
- **In depth (V):** Atlas, Flyway, Liquibase, Bytebase, Sqitch (`check` only), Supabase CLI, Wrangler D1, Drizzle, Payload, Prisma, strong_migrations, django-migration-linter, Squawk, Eugene, MigrationPilot, Skeema, pgroll, migra, PlanetScale, Neon schema diff.
- **Briefly:** Reshape, onwardpg, pg-schema-diff, gh-ost, pt-online-schema-change, Turso, and newer repos (sqlite-schema-gate, orm-lens, diesel-guard).
- **Not verified:** Supabase and Neon branching pricing, and Sqitch's engine list.

Protections: **P1** an applied migration can't be edited or removed · **P2** out-of-order runs are refused · **P3** production history is compared with the repo before release · **P4** changes that break the running app are refused unless marked as a contract step · **P5** one ordered release.

#### Comparison table

| Tool | P1 | P2 | P3 | P4 (method) | P5 | Engines / hosting | Licence, price |
|---|---|---|---|---|---|---|---|
| Atlas | atlas.sum checksum (own directory) V | `non_linear` analyser, reports only V | registry/drift, Pro V | BC101-104, DS, MF: parses SQL plus a dev database; **Pro only since v0.38** V | Pro pipelines | PG, MySQL, SQLite, libSQL/Turso guide; **no D1 page in atlasgo.io sitemap** V | Community edition is Apache and has no `migrate lint`; Pro $9/dev/mo + $59/CI project/mo V (2026-11-19) |
| Flyway | CRC32 check plus "applied but not resolved locally"; validateOnMigrate=true V | outOfOrder=false V | `validate` against its own table V | Enterprise "policy checks" V | no | JDBC engines; no D1 | Community Apache 2.0; Enterprise by quote; Teams closed to new buyers since 2025-05-14 R |
| Liquibase | checksum mismatch halts V | U | drift in "Change Intelligence" (Secure Team and up) V | Secure policy checks V | no | JDBC | **FSL-1.1-ALv2 from 5.0 (2025-09-30)** V; Secure by quote, services package required V |
| Sqitch | `sqitch check` compares SHA1 of deploy scripts V | plan order R | `check` V | no | no | several (U) | MIT, free |
| Bytebase | U | U | U | `schema.backward-compatibility` rule: statement analysis, MySQL/TiDB/PG/OceanBase; no SQLite V | approvals on Enterprise | own console | MIT plus enterprise dirs V; Pro $20/user/mo V |
| Supabase CLI | **edits not detected** (compares timestamps only) V | refuses older local files unless `--include-all`; refuses remote versions missing locally V (source) | `migration list` V | no | no | Supabase PG | MIT |
| Prisma Migrate | `deploy` only **warns** on a modified migration V | U | no drift check in `deploy` V | no | no | PG, MySQL, SQLite, D1 via adapter | Apache |
| Drizzle (stable) | hash stored but never compared V (source) | applies only migrations newer than the last `created_at`, so an **older merged migration is silently skipped** V | no | no | no | PG, MySQL, SQLite, D1 | Apache |
| Drizzle v1 beta.16 (2026-03-05) | U | reads all migrations, allows out-of-order with a "commutativity check" V | no | no | no | same | same |
| Payload | none: tracks name and batch V (source) | runs any file not in the table V | no | no | no | PG, Mongo, SQLite, D1 | MIT |
| Wrangler D1 | none: `d1_migrations(id,name,applied_at)` V (source) | applies any unapplied name V | no | no | Time Travel bookmarks, 30 days R | D1 only | Apache |
| strong_migrations | no | no | no | yes: hooks Rails migration methods at runtime; `safety_assured` is the contract marker V | no | PG, MySQL, MariaDB | MIT |
| django-migration-linter | no | no | no | yes, "backward incompatible" (Django migrations only) V; mechanism R | no | Django back ends | Apache |
| Squawk | no | no | no | partial: parses SQL, lock rules plus `ban-drop-column`; `squawk-ignore` marker V | no | PG | Apache |
| Eugene | no | no | no | partial: parses SQL, plus `trace` runs the SQL in a transaction on real PG V | no | PG | MIT |
| MigrationPilot | no | "sequence analysis" V | no | 112 rules, libpg-query parser, detects 14 migration frameworks, `--from-command` V | no | PG | MIT; Org plan $499/yr V |
| pgroll / Reshape | n/a | n/a | n/a | runs expand/contract by serving old and new schema versions V | partial | PG 14+ / PG 12+ | Apache / MIT |
| Skeema | n/a (declarative) | n/a | diffs live DB V | lint rules plus unsafe-DDL guard V | no | MySQL, MariaDB | Apache; Premium $1,238–$4,988/yr V |
| PlanetScale deploy requests | n/a | serial queue V | diffs schema against production V | warns on data loss; 30-minute revert V | yes | **Vitess only** V (Postgres support: no deploy requests, R) | Vitess from $39/mo V |
| Neon schema diff | no | no | branch diff with a PR-comment Action; **no gating** V | no | no | Neon PG | platform feature |

**Context only:**
- **migra:** deprecated; last release 2022 V.
- **gh-ost (13,654★) and pt-online-schema-change (1,571★):** MySQL online-DDL executors. They don't track history or check compatibility.
- **Turso:** I found no migration protections. Multi-DB Schemas are deprecated for new users (R). Its blog lists "Turso is joining Supabase", dated 2026-10-02 (R, listing only).

**Adoption (V):**

| Tool | Stars | Recent release | Downloads |
|---|---|---|---|
| Bytebase | 14,721 | — | — |
| Flyway | 10,210 | — | — |
| Atlas | 8,832 | v1.4.0, 2026-11-17 | — |
| pgroll | 6,630 | — | — |
| Liquibase | 5,644 | — | — |
| strong_migrations | 4,462 | — | 88.6M gem downloads in total |
| Sqitch | 3,178 | — | — |
| Reshape | 1,858 | — | — |
| Squawk | 1,271 | v2.73.0, 2026-11-15 | squawk-cli 7.4M npm/month |
| django-migration-linter | 624 | — | 4.48M PyPI/month |
| Eugene | 66 | last release 2026-03-11 | — |
| MigrationPilot | 10 | — | 3,693 npm/month |

#### Gaps

1. **Cloudflare D1 (HTTP API only, no wire protocol).** No free tool gives P1–P4. Wrangler, Drizzle and Payload all run against D1, and I confirmed in their source that none compares checksums or refuses out-of-order runs. Wrangler and Payload run older files silently; stable Drizzle skips them silently. Atlas has no D1 page, and Flyway and Liquibase connect over JDBC. The only D1-specific tools are near-zero-star repos from 2026: sqlite-schema-gate (created 2026-11-18) and d1-migration-guard (its README returned 404).
2. **Breaking-change checks on SQLite.** Atlas has SQLite checks (LT101, MF103), but linting now needs a paid Pro login. Squawk, Eugene, MigrationPilot and Bytebase's compatibility rule cover only PG or MySQL.
3. **TypeScript migrators with no SQL file** (Payload, Knex, TypeORM). Linters that parse SQL need the SQL captured first. MigrationPilot's `--from-command` comes closest, but it is PG-only.
4. **P1–P3 across migrators.** Each migrator checks only its own history table. Atlas imports just five formats: golang-migrate, goose, Flyway, Liquibase and dbmate (V).

**Is there an open tool that works with any migrator?** I found none that provides P1–P5. Partial coverage:
- MigrationPilot: P4 only, PG only.
- Atlas: P1–P5, but only for its own and five imported formats, and P4 is paid.

The "contract" escape hatch exists only inside single ecosystems: `safety_assured` (Rails), linter ignore lists (Django), `squawk-ignore` (Squawk).

#### What practitioners complain about

- **Dropping a column while old instances still run.** In Deno's 2021 outage, a migration dropped a column and replaced it while the new code was rolling out. Old instances that still referenced the column kept crashing (V: https://deno.com/blog/2021-07-27-outage-post-mortem). Relayed accounts of the same pattern: Buildkite (https://www.buildkitestatus.com/incidents/8nd3cm8kqlnn) and a Rails write-up (https://smsohan.github.io/blog/2019/03/18/ruby-on-rails-database-migration-and-downtime).
- **Edited migrations pass in production.** Prisma's `migrate deploy` only warns, on every run (V: https://www.prisma.io/docs/orm/prisma-migrate/understanding-prisma-migrate/migration-histories). There are also false checksum mismatches caused by CRLF line endings (R: https://lightrun.com/answers/prisma-prisma-the-migration-20210501202953_name_of_previous_migration-was-modified-after-it-was-applied).
- **Migration tracking itself breaks.** Drizzle beta.13–15 re-applied migrations on every run after a format conversion, which led to a redesign (V: https://github.com/drizzle-team/drizzle-orm/releases/tag/v1.0.0-beta.16). A multi-branch ordering issue is also reported (R: https://github.com/drizzle-team/drizzle-orm/issues/1221).
- **Half-applied D1 migrations.** People recover by hand-inserting rows into `d1_migrations`. This is weak evidence, from community guides only (R: https://playbooks.com/skills/jezweb/claude-skills/d1-migration).
- **ORMs generate destructive SQL without warning.** Prisma issue #24180 (R: https://github.com/prisma/prisma/issues/24180).
- **Paywalled linting.** A post titled "Atlas paywalled their migration linter" (R, written by MigrationPilot's author, so conflicted: https://dev.to/mickelsamuel/atlas-paywalled-their-migration-linter-here-are-your-free-alternatives-4god).

#### Commercial context

Who pays today:
- Atlas Pro: $9/dev/mo + $59/project/mo, plus $39 per extra target (V, https://atlasgo.io/pricing).
- Bytebase Pro: $20/user/mo (V, https://www.bytebase.com/pricing/).
- Skeema Premium: $1,238–$4,988/yr, flat per company (V, https://www.skeema.io/download/).
- MigrationPilot Org: $499/yr (V, README).
- Liquibase Secure and Flyway Enterprise: by quote only (V).
- PlanetScale: deploy requests come bundled with Vitess hosting (V).

Features moved behind licences or paywalls:
- **Atlas:** `migrate lint` became Pro-only in v0.38. Docs (V, https://atlasgo.io/versioned/lint); date 2025-10-30 relayed (R).
- **Liquibase:** Community moved to FSL, which is not OSI-approved, from 5.0 on 2025-09-30 (V, https://www.liquibase.com/blog/liquibase-community-for-the-future-fsl).
- **Flyway:** policy checks and drift detection are Enterprise-only (V, https://www.red-gate.com/products/flyway/editions). Teams is closed to new buyers (R, ComponentSource).

#### Three reasons for and against building it

**For:**
1. **The D1/SQLite plus TypeScript-migrator corner is verifiably unserved.** Wrangler, stable Drizzle and Payload check neither checksums nor order. Atlas has no D1 page, and its SQLite checks are paid. Payload's D1 adapter alone has 194k npm downloads a month.
2. **Incumbents are moving these checks behind paywalls.** That covers Atlas lint, Liquibase's licence and Flyway's policy checks. Squawk-cli's 7.4M monthly downloads show demand for free linting.
3. **P1–P3 are cheap and portable.** They are the proven Flyway, Liquibase and Sqitch design: names plus hashes against a history table. They need only a read of the history table, which is reachable even through D1's HTTP API.

**Against:**
1. **The space is crowded and attention is the bottleneck.** I sampled the first 100 GitHub results for "migration linter" (of 239 in total). 61 were created in 2026, and only one (orm-lens, 112★) has 50 or more stars.
2. **"Any engine" P4 is large.** It needs per-dialect parsing or a shadow database. Every established P4 tool picked a single engine (Squawk, Eugene, MigrationPilot) or a single ORM (Rails, Django).
3. **Platform owners are closing gaps themselves.** Drizzle v1 is redesigning tracking, the Supabase CLI already refuses out-of-order runs, and Turso is reported to be joining Supabase. A third-party guard could be overtaken.

**Bias note.** The brief's "any migrator, any engine, any host" framing pushed me to hunt for universal gaps and weight D1 heavily. It discouraged me from treating ecosystem tools such as strong_migrations and the Django linter as "good enough", which for their own users they arguably are.

**Not checked:** how Liquibase and Prisma handle out-of-order runs, Sqitch's engines, Supabase and Neon branching pricing, and whether D1 runs each migration file atomically. The npm download figures look high next to 2025 levels; they are reported as the API returned them and not cross-checked.

## Claude

Case C showed other problems but not the one I was testing, a rewritten hash. Something is off, so I'll call the function directly on a temporary branch to see what it reads:

## Claude

Now the combined research report, which the ADR already points to:

## Claude

A separate project is worth building, but not with "any database" as the opening promise. Two independent studies found that the corner we're in is unserved: Cloudflare D1 and serverless SQLite, with migrations written in code as Payload's are. The full report is in `docs/research/2026-11-19-migration-guard-research.md` (`c2a27da`).

**What exists today:**
- **No open tool protects production migrations across migrators.** Each migrator guards only its own history, and most guard little:
  - 14 of the 21 migrators studied keep no content hash at all.
  - Drizzle stores a hash but never compares it, and silently skips an older migration forever.
  - Prisma only warns when a deployed migration was edited.
  - Payload and Wrangler run anything not yet run, in any order.
- **For D1 nothing free works.** Flyway and Liquibase need a direct database connection that D1 doesn't offer, Atlas doesn't support D1, and the D1 attempts on GitHub are a few near-empty projects from this year.
- **The safety checks are being put behind paywalls.**
  - Atlas's linter is paid since late 2025.
  - Liquibase moved to a licence that isn't open source.
  - Flyway's policy checks are Enterprise only.
- **Demand for free checks is large:** Squawk, a free Postgres-only linter, gets about 7.4 million npm downloads a month.

**How the tool would work, in three layers:**
1. **Record and freeze:** what production ran, kept as content hashes in the repository. This needs no database access and works with any migrator.
2. **Compare with production:** one query per migrator against its history table, run at release time with read-only credentials.
3. **Breaking changes:** compare the schema of a throwaway database before and after the new migrations, rather than reading the SQL text. Because it runs the user's own migrate command, it works for migrations written in code, and it sees the triggers, constraints and views that text matching misses.

Parts of Atlas can be reused: its schema readers and change analyzers are open source, and only the command that runs them is paid (I confirmed the analyzer packages in its repository).

**My recommendation for the pitch:** "the release guard for serverless SQLite and code-written migrations, with Postgres next". Start with SQLite/D1 and the Payload, Wrangler, Drizzle and Prisma migrators. The biggest risk is running another tool's migrations against a throwaway database, which needs the app's own setup to run. The space is also full of new, unknown projects, so getting noticed is its own hurdle.

**Fixed in this repo along the way** (`cfd5508`). The technical study found that my migration freeze could be bypassed, and I reproduced it:
- **The bypass:** one pull request could edit a deployed migration and rewrite its hash in the record, and every check agreed. The record is now append-only against `origin/main`, and CI fetches full history and fails if it can't read it. I tested this against a real git base: rewritten and removed entries are caught, and a growing record passes.
- **The missed case:** a new `NOT NULL` column without a default is now refused after launch.
- **One claim was wrong:** its reported false positive on a safe migration did not reproduce.
- **Still out of reach** for this text-based check: triggers, constraints and views, which is exactly what the new project's schema comparison would fix.

Still waiting on you: your OK to re-download Playwright's Chromium (about 100–150 MB), so the two website fixes (the year filter and the WhatsApp button) can be tested and committed. Want me to set up the new repository from this report? I can also publish the report as a shareable page.

## User

Well, my my initial idea is not that we target D1 only, but target any database for this matter. But yeah, we can start with D1.

## Claude

Understood: the target is any database, and D1 is just the first one. I'll fix the report, since its verdict implied a narrower aim. The design already supports that goal:
- The first two layers (freezing deployed migrations, and comparing with production's history) don't depend on the engine. They can cover every database from day one, given one history-table query per migrator.
- Only the breaking-change layer needs a plug-in per engine: a way to start a throwaway database and a schema reader.

So "any database" becomes the architecture's promise from the first release, and D1 is simply the first engine plug-in. Postgres should be the second, early, to prove the interface really generalises.

## Claude

I've updated the report to match (`baa9e19`).

To start the new project I need three decisions from you:
1. **Name and location.** For example `~/Desktop/<name>` as its own git repository. Do you have a name in mind?
2. **Language.** I recommend one Go binary, with npm and pip wrappers so any project can install it. It can reuse Atlas's open schema readers for Postgres, MySQL and SQLite. The alternative is TypeScript, which is closer to this repo and can run D1's local emulator and PGlite in the same process, but would need its own schema readers for each engine.
3. **Licence.** I'd suggest Apache-2.0: it's compatible with the Atlas code we'd reuse, and it promises users the checks won't move behind a paywall.

Separately, still pending: your OK to re-download Playwright's Chromium (about 100–150 MB), so the two website fixes can be tested and committed.

## User

make sure to create a Claude transcript starting from my question about generalizing this tool to work on any database stack, and open a new window, terminal window, and yeah, launch that session.

## Claude

The transcript is missing the research agents' reports, which are the most valuable part. They must be stored in a different record shape. Checking:

## Claude

The two reports are stored differently: one as a hidden user message, the other as a queued attachment. Updating the extractor to pick up both:

## Claude

The two agent reports are in now. Their own headings collide with the transcript's, so I'll nest them one level down. Then I'll create the project:

## Claude

The project exists at `~/Desktop/migration-guard`, with its first commit. Before opening the new window, checking the command that starts your Claude sessions:

## Claude

The launch command goes through an MCP selector (`mcp`). Checking whether it passes an opening prompt through to Claude:

## Claude

A new Terminal window is open in `~/Desktop/migration-guard`, with your Claude session launching there. Pick your servers in the selector as usual, then paste the first message I put on your clipboard. It asks the new session to read the research and propose a name, language, licence and first-version plan, without writing code until you've decided.

**The project** (working name `migration-guard`, one commit, `6296f8f`):
- **`README.md`**: the goal (any database, any migrator, any host), your direction (D1 first, Postgres second), and the open decisions.
- **`CLAUDE.md`**: tells the new session to read the research first and to leave name, language and licence to you.
- **`docs/research.md`**: the combined research report.
- **`docs/transcript.md`**: this conversation from your question "can a tool be developed that is plug and play for any db, any target infra?" onward. It has your messages, my replies and both research agents' full reports; tool calls are left out.

**Your agentic-browser suggestion worked.** I used the scraping pipeline's Lightpanda browser through `agent-browser` on the built site with test data:
- **"2024 and older"** now lists 2 cars instead of an error.
- **The empty search's "Ask us to find one"** and every other WhatsApp link on the page now use the published number.
- **The old code fails both checks**, so the probe really tests the fixes.

Committed in `3f3f833`. The Playwright tests I added for them will run in CI, since Chromium is still missing on this Mac. One slip on the way: my first probe pointed the API at an empty database and returned errors; I caught it and reran.

## User

And does the new Claude session that is part of the migration guard repo contain like the questions, the prompts, the answers that we asked in this session?

## Claude

Only through a file, not as memory. A new Claude session starts empty and doesn't inherit this conversation. What it has is `docs/transcript.md` in the repo, and its `CLAUDE.md` tells it to read that first, as does the message I put on your clipboard. Checking what the transcript actually contains:
