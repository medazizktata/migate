# Is there a gap, and what would a better wheel be?

Date: 2026-11-19. Scope: **any database, any host** (the owner's direction). D1 may be one of the first plug-ins; it is
not the product. This builds on `first-pass.md` and the landscape report in `../transcript.md` (lines 245-360) and does
not repeat them.

**How this was made.** A research agent read primary sources (docs, source files, the GitHub, npm, PyPI and Docker Hub
APIs). Pages were fetched with curl; no browser. The main session then re-checked the claims the verdict rests on (see
"Checked by the main session" at the end).

**Labels.** **V** read the primary source today. **R** relayed from a secondary source. **U** unknown. **P** from the
first-pass research, not re-checked today.

**Dates for counts.** GitHub stars: 2026-11-19 UTC. npm downloads: 2026-10-19 to 2026-11-17, compared with the same
window in 2025. PyPI: pypistats "last month" on 2026-11-19. Docker Hub: lifetime pulls on 2026-11-19.

**npm warning (V, unexplained).** 2026 counts are 2-20x the same 2025 window, and the growth is uneven:

| Package | 2026 window | 2025 window |
|---|---|---|
| wrangler | 169.7M | 8.6M |
| pg | 382M | 49M |
| drizzle-orm | 187M | 10.5M |
| knex | 28.1M | 11.8M |

Use these only as ratios inside one window.

## The verdict in short

- **No open, free tool gives P1-P5 for any migrator, engine and host (V).**
- **Linting is saturated, and a frozen ledger is becoming a commodity.** 821 repositories from 15 GitHub searches, most
  created in 2026, most with 0-5 stars. New entrants get 18 to 3,693 npm downloads a month, against squawk-cli's 7.4M.
  Adoption and trust are the scarce resource, not capability.
- **What stays open and durable is the release decision:** does the pull request's migration set match what production
  actually ran (P1-P3), and will its schema change break the app version still serving traffic (P4), unless a contract
  step with a stated reason says so? No open tool answers that for any migrator, engine or host.
- **Positioning:** "a free, offline release gate that stops a migration from breaking the app version still running or
  drifting from what production ran, for the migrator and database you already use."
- **Main threat:** DbProof (same method, PostgreSQL and GitHub only, hosted, a dropped column is only a warning).

## 1. Closest competitors in depth

### 1.1 Near-direct entrants (both October 2026)

**DbProof** (dbproof.dev; GitHub org created 2026-10-01, 4 repos, 0 stars each; V)

- **Method:** a GitHub Action restores production's schema and planner statistics on a throwaway Postgres, then applies
  the pull request's migrations one at a time with **your own migrate command** (`npx prisma migrate deploy`,
  `npx drizzle-kit migrate`, Flyway in Docker). A capture step before and after each deploy reads the catalog, the
  planner estimates and your migration history table.
- **What it catches:** NOT NULL on a column that has nulls; a migration that fails to apply; an index built without
  CONCURRENTLY; a rewrite of a table over 1M rows; a foreign key without an index; a dropped column (**a warning only**);
  "Version used by another open pull request"; "An applied migration file changed"; drift.
- **Scope:** PostgreSQL 13+ on RDS, Cloud SQL, Azure, Neon or self-hosted; GitHub Actions only. FAQ: "MySQL, GitLab,
  other CI? Not yet."
- **Migrators:** Flyway, Atlas, Prisma, Drizzle.
- **Licence and price:** agent and Actions Apache-2.0. The service needs a GitHub App with write access and an account.
  "Nothing during the beta"; price after beta U.
- **Short of P1-P5:** P2 covers only version collisions; P4 has no notion of the app version still running and no
  contract marker; no P5; Postgres and GitHub only; your schema and statistics go to a third party.

**migrations-safe** (github.com/NahuelMosse/migrations-safe; MIT, 1 star, created 2026-10-30; npm 812 in the window; V)

- **Lint:** regex rules per dialect for PG, MySQL/MariaDB, SQL Server, Oracle, Db2 and SQLite. README: "The linter is
  regex-based; for PostgreSQL also run squawk."
- **History check:** `lint --history` diffs against the git base and reports `applied-migration-changed`, `out-of-order`
  and `duplicate-sequence`. Files on the base branch are "assumed applied everywhere" (`history.mjs`).
- **Rehearsal:** `rehearse` runs the migration SQL on a copied SQLite file or a Docker database and reports lost data,
  orphans and lock time.
- **Migrators:** 16 auto-detected. Alembic, Django, Knex, Rails, EF Core and SqlPackage must have their SQL generated and
  piped in.
- **Output:** SARIF and GitHub annotations, a GitHub Action, a Claude Code skill.
- **Short of P1-P5:** no P3 (never reads production's history table); P4 covers data loss and locks only, its docs list
  the N-1 app under "Not covered"; the rehearsal imitates each migrator rather than running it; `--allow-loss` needs no
  reason; no P5.

### 1.2 Atlas across engines (V)

- **Editions:** Community (Apache-2.0) supports MySQL, PostgreSQL, SQLite and MariaDB. The standard build is under the
  Atlas MSA and needs `atlas login` (30-day trial, then a licence). It adds SQL Server, ClickHouse, Redshift, Aurora DSQL,
  CockroachDB, YugabyteDB, Oracle, Spanner, Snowflake, Databricks and Azure, plus "views, triggers, functions,
  sequences, partitions, RLS".
- **Lint (correction to the first pass, which called it paid):** the v0.38 post (2025-10-28) says lint left the free
  Starter plan but "remains open-source ... available in Atlas Community Edition". The source agrees:
  `cmd/atlas/internal/cmdapi/cmdapi.go` registers `migrateLintCmd()`. Today's Community page marks lint as unavailable.
  **The docs and the source disagree.**
- **Releases:** v1.0 (2025-12-24) through v1.4 (2026-11-17). Drift detection is Pro.
- **Price:** Starter free (MySQL, MariaDB, PG, SQLite); Pro $9/developer/month + $59/CI project/month (2 targets) + $39
  per extra target.
- **Open inspectors:** `sql/sqlite/inspect.go` (673 lines) has 0 mentions of "trigger" and 0 of "view";
  `sql/postgres/inspect.go` (1,656 lines) has 0 mentions of "trigger". **P4 built on Atlas's open packages cannot see
  triggers or views.**
- **D1:** 0 of 678 sitemap URLs mention D1 or Cloudflare. There is a Turso guide.
- **Atlas replaces your migrator rather than guarding it:** its Drizzle guide says "replace `drizzle-kit migrate` with
  Atlas". Import covers Flyway, Liquibase, goose, golang-migrate and dbmate.
- **Issues:** ariga/atlas#3467 (Community BC102 reports a rename as "destructive" since v0.30, open since 2025-04);
  ariga/atlas#3730 (the PR comment is never updated when the PR passes).

### 1.3 Platform owners and hosts

| Who | Today (V) | Gap |
|---|---|---|
| **Prisma** (dist-tag latest 8.0.0-rc.34) | `migrate deploy` warns "WARNING The following migrations have been modified since they were applied" on every run and continues. `migrate diff`: "If two databases differ only in unsupported features, such as views or triggers, then migrate diff will not show any difference." | P1 only warns; P4 blind to triggers and views |
| **Drizzle** v1 still RC (rc.5, 2026-10-21); stable 0.45.4 / kit 0.31.11 | v1 tracks by folder name and intentionally allows out-of-order. `drizzle-kit check` finds non-commutative branch migrations, "PostgreSQL and MySQL today". The hash is used for upgrade matching, not edit detection. drizzle#5316: "production broke as it hadn't run the previous migration"; #5777: check adds about 115 s per migrate; #5960: exponential with branches. | No P1, P3 or P4; P2 for PG/MySQL in v1 only |
| **Payload** | #16644 (open, 2026-05): the SQLite migrate runner executes table rebuilds "inside FK-ON transaction". #15106: the rename prompt makes it easy to get a down migration that "recreates an empty table". #19062 (open, 2026-10): `payload migrate` hangs in CI after a dev push. No public roadmap item found (U). | No P1-P4 |
| **Wrangler / D1** (4.165.0) | The D1 changelog RSS has 14 items; the only migration change is `migrations_pattern` nested layouts (2026-06-04). workers-sdk#13568 (filesystem order) fixed in 4.80. | No P1-P4 |
| **Supabase branching** | "A migration file with a timestamp older than the latest applied migration still runs and the step finishes with a warning"; a missing file fails the step. | P2 only warns; no P4 |
| **Neon** | Schema Diff with a PR-comment Action (8 stars). | Shows, never gates |
| **PlanetScale Postgres** | "Unlike PlanetScale Vitess deploy requests, schema changes must be manually applied to each branch." | Gating is Vitess-only |
| **Turso** | Blog: "Turso is joining Supabase" (2026-10-02). No migration safety features found (U). | n/a |
| **stripe/pg-schema-diff** (909 stars, MIT, pushed 2026-11-09) | Declarative PG diff producing online-safe plans, validated on a temporary database; hazards approved with `--allow-hazards`; PG 14-17. | A migrator replacement; no ledger; no N-1 check |

### 1.4 Everything else, counted

15 GitHub searches (top 100 by stars each), 821 unique repos (V). Selected queries:

| Query | Total | Created in 2026 / returned | 50+ stars |
|---|---|---|---|
| migration guard | 320 | 83/100 | 5 |
| migration safety | 298 | 71/100 | 5 |
| migration linter | 239 | 58/100 | 9 |
| migration checksum | 75 | 56/75 | 0 |
| schema diff breaking | 92 | 84/92 | 1 |
| d1 migrations | 71 | 61/71 | 1 |
| expand contract migration | 27 | 26/27 | 0 |
| shadow database migration | 17 | 13/17 | 0 |

The 50+ star hits are mostly off-topic. 25 READMEs were read (V for the text; no claim was tested).

| Tool | Stars, created | Does | Engines | Short |
|---|---|---|---|---|
| sqlite-schema-gate (curlewlabs-com) | 0, 2026-11-18 | Pragma layout lint; text migration lint; seed fence; **allowlist with a required reason that reports unused entries** | SQLite, D1 | No P1-P3; P4 by text |
| d1-migration-guard | 0; npm 18/month | Order, config, regex destructive checks | D1 | Regex only |
| d1-preflight | 4, 2026-11-01 | Disposable SQLite snapshot diff, plan regressions | D1/SQLite | SQL files only; no P1/P3 |
| cloudflare-deploy-toolkit (curlewlabs-com) | 1, Apache-2.0 | Lock, drain, D1 Time Travel bookmark, recovery | Workers+D1 | P5 only |
| knex-drift | 0 | Checksums; blocks edited and out-of-order | Knex | One migrator |
| MigrationHashGuard | 0, 2026-10-28 | "Immutable hashes" for migration files | any | 30-line README |
| migraguard | 0 | CI tamper detection, drift, LLM expand/contract | PG first | Its own workflow |
| dbtools | 1; npm 716/month | SHA-256 ledger, scratch replay and diff | MSSQL/PG/MySQL/SQLite | It is a migrator |
| mgcheck, safe-migrate, dryrun, pgfence | 5 / 29 / 36 / 11 | AST or parse rules, shadow PG, snapshot | PG | PG only |
| drizzle-migration-guard, drizzle-graph, prisma-flow, schema-pilot | 0-3 | Single-migrator checks | Drizzle/Prisma | One migrator |
| laravel-schema-sentinel 64, orm-lens 112, diesel-guard 122, excellent_migrations 303, pg_ha_migrations 241 | — | Ecosystem lint and drift | mostly PG | One framework |
| rls-guard | 0 | Replays migrations on an ephemeral PG and checks RLS invariants | PG | A useful precedent |

### 1.5 Where established tools stop, by engine

Usage figures: Stack Overflow developer survey 2025 (V).

- **PostgreSQL (55.6%).** P1/P2: Flyway, Liquibase (FSL), Sqitch. P3: Flyway `validate`; DbProof (SaaS). P4: Squawk,
  Eugene, MigrationPilot, pgfence, strong_migrations, the Django linter, Atlas Community lint (no triggers or views),
  DbProof. Lint is crowded. **An N-1 schema-diff gate is missing.**
- **MySQL (40.5%) and MariaDB (21.7%).** P1-P3: Flyway, Liquibase, Sqitch. P4: Atlas Community, Bytebase (P), Skeema
  (paid). Open P4 is thin.
- **SQLite (37.5%), including D1 and libSQL.** Wrangler, Payload and stable Drizzle check nothing (P). P3: none for D1.
  P4: Atlas Community (no triggers); tiny new repos. The thinnest-served engine.
- **SQL Server (30.1%).** Flyway and Liquibase cover P1-P3. P4: Atlas only in the paid build; migrations-safe has regex
  rules.
- **Oracle (10.6%), CockroachDB (0.6%).** Flyway covers P1-P3; P4 in Atlas's paid build only.
- **MongoDB (24.3%).** No DDL, so P4 does not apply as defined (breakage lives in code, validators and indexes). P1-P3
  still apply: offer the ledger and history layers only.

## 2. Practitioner pain (V, linked issues)

- **Edited migrations.** Prisma "will always warn ... each time you run the command" (docs); prisma/orm#7101 (CRLF
  checksum mismatches); prisma/orm#11376 ("conflicting info about editing migrations").
- **Out-of-order merges across branches.** drizzle#5316 ("production broke"; "Now there's no merge conflicts ... silent
  bugged behavior"); drizzle#2488 "Merge conflicting migrations" (52 reactions); workers-sdk#13568; GitLab's
  `db:check-migrations` checks migration age ("a migration that passes in one pipeline can fail in a later one").
- **Renames become drop plus add.** prisma/orm#7790 (82 reactions), prisma/orm#12387, payload#15106, atlas#1535.
- **False positives.** Squawk #673, #385, #693, #1189; django-migration-linter #326, #294, #310; Atlas #3467.
- **Escape hatches without reasons.** strong_migrations#257 "safety_assured should require justification", declined by
  the maintainer ("I'm not sure it's something I'd like to include"; see #165). strong_migrations#295: "I reject the PR
  unless they document proper reasoning." `squawk-ignore` and `atlas:nolint` take no reason.
- **Speed and CI friction.** drizzle#5777 (about 115 s per migrate), payload#19062 (hangs in CI), atlas#3730 (stale PR
  comments).
- **Login and paywall.** Atlas lint, CI and drift need `atlas login`; DbProof's GitHub App needs write access to
  contents, PRs and workflows.
- **Not searched:** Reddit, HN, Discord; incident write-ups beyond Deno and Buildkite (P).

## 3. A better wheel: differentiators

| Differentiator | Who lacks it | Evidence of demand | Verdict |
|---|---|---|---|
| **N-1 compatibility gate from a schema diff, triggers, views and constraints included** | Atlas's open inspectors; Prisma diff; migrations-safe ("N-1 not covered"); DbProof (warning only, PG only) | Deno outage (P) | **The core differentiator** |
| **Contract marker that requires a reason, plus a stale-exemption report** | strong_migrations (declined), Squawk, Atlas, migrations-safe | strong_migrations #257, #295, #165; sqlite-schema-gate proves the pattern | **Cheap and distinctive** |
| **Read-only P3 comparison with production's history table, any migrator** | Only DbProof (PG, SaaS), Atlas Pro (own format), Flyway/Liquibase (own tables) have it | DbProof is built around it | **Strong, engine-free** |
| **Runs the user's own migrate command** | Squawk and migrations-safe (SQL piped in); d1-preflight; Atlas (replaces the migrator) | DbProof proves the method on PG | **Strong** |
| **Guards the migrator rather than replacing it** | Atlas, dbtools, pg-schema-diff, migraguard replace it | drizzle-kit 159M, prisma 104M, alembic 233M a month | **The positioning anchor** |
| **No account, works offline** | Atlas standard build, DbProof | Squawk's free usage | Table stakes against free tools |
| **Explains why and suggests the expand/contract fix** | Most regex linters print only an id | strong_migrations does it (88.6M downloads); strong_migrations#119 | Do it |
| **Low false positives** | See the issues above | — | Claim only what fixtures prove |
| **Per-migrator order model** | migrations-safe is closest; Drizzle's check is PG/MySQL only | drizzle#5316, #2488; GitLab age check | Strong; the most error-prone part to build |
| **SARIF, annotations, one updated PR comment** | Atlas leaves stale comments (#3730) | — | Table stakes |
| **Speed** | Drizzle's check takes about 115 s | — | Measured in `stack.md` |
| **P5 release order** | Only cloudflare-deploy-toolkit and PlanetScale (Vitess) | — | A documented recipe per host, not core code |

## 4. Is the gap real and durable?

### 4.1 Market size (V)

- **Databases (Stack Overflow 2025):** PG 55.6%, MySQL 40.5%, SQLite 37.5%, SQL Server 30.1%, MongoDB 24.3%, MariaDB
  21.7%, Oracle 10.6%, Supabase 5.7%, CockroachDB 0.6%.
- **Node drivers (npm, one window):** pg 382.4M, mysql2 95.0M, better-sqlite3 64.9M, mongoose 36.3M,
  @neondatabase/serverless 29.6M, @libsql/client 22.8M.
- **Migrators:** npm (one window): drizzle-kit 159.3M, prisma 103.6M, knex 28.1M, typeorm 27.8M, sequelize 16.3M, umzug
  12.3M, payload 6.1M, node-pg-migrate 5.3M, dbmate 2.1M. PyPI (last month): alembic 233.5M, django 51.9M,
  yoyo-migrations 1.0M. RubyGems (lifetime): activerecord 870M, strong_migrations 88.6M. Docker Hub (lifetime pulls):
  flyway 96.3M, liquibase 29.4M, migrate/migrate 24.5M, arigaio/atlas 6.3M, dbmate 3.0M. GitHub stars: golang-migrate
  19,034, goose 11,712, flyway 10,210, dbmate 7,555, liquibase 5,644.
- **The D1 slice:** @payloadcms/db-d1-sqlite 194k against 4.23M for @payloadcms/db-postgres (about 1/22);
  @prisma/adapter-d1 295k against 103.6M for prisma. **A D1 identity would cap the audience at a small fraction.**
- **Free-check demand:** squawk-cli 7.4M on npm plus 2.7M on PyPI; django-migration-linter 4.5M.

### 4.2 Overtake risk

- **Platform owners fix only their own migrator.** Drizzle v1 adds a P2 check for PG/MySQL; Prisma could turn its
  warning into an error; Supabase already warns. None will guard competitors' migrators, so **P4 (N-1) and
  cross-migrator P3 are the durable part.**
- **Atlas is moving these pieces behind a login** (lint in the standard build, drift, triggers, views, extra drivers),
  which widens the open gap. Licence drift risk remains (P).
- **DbProof is the main threat.** Defend with: offline and no account; all engines through plug-ins; an N-1 gate, not a
  warning; reasoned contract markers; CI systems other than GitHub. Watch whether it adds MySQL or an offline mode.
- **Do not build another regex linter.** Ingest SARIF from Squawk or migrations-safe for lock rules instead.

### 4.3 Build order (engine-neutral)

1. P1+P2+P3 as one engine-free "production matches the repo" gate. It works for every engine and host on day one.
2. P4 as an N-1 compatibility gate: our own trigger, view and constraint readers, plus the user's migrate command on a
   throwaway database; a contract marker that must give a reason; stale markers reported.
3. Leave lock rules to Squawk and similar tools, and ingest their SARIF.

### 4.4 Opinion (the agent's, not evidence): engine plug-in order

1. **SQLite family first (SQLite, D1, libSQL) as the build target, not the launch identity:** the cheapest throwaway
   database, the owner's own production, the thinnest-served engine, and it forces the plug-in interface to handle
   HTTP-only hosts early.
2. **PostgreSQL second, early, and lead the public launch with it:** the largest share; DbProof shows people adopt this
   kind of check.
3. **MySQL/MariaDB third. SQL Server fourth** (Atlas Community has no driver, so the gap is real but costly).
4. **MongoDB: ledger and history layers only.**

## 5. Bias and limits

- **Nothing was installed or run.** Competitor capabilities are their own README claims (U for behaviour), except the
  Atlas and migrations-safe source files read.
- **Search limits:** at most 100 results per query, sorted by stars; classification was a regex over descriptions.
  Closed-source SaaS is likely missed (DbProof was found only through its Action repo).
- **Not checked:** Reddit, HN, Discord; Bytebase, Flyway and Liquibase pages (P); Maven and Go download counts;
  DbProof's price after beta; whether remote D1 accepts the pragma table functions; npm inflation's cause.
- **How the brief pushed the agent:** "a better wheel" pushed toward finding a gap; DbProof and migrations-safe are the
  main evidence against an easy verdict. The engine-neutral correction moved weight away from D1, so SQLite/D1 being the
  least-served corner may now be under-weighted. Some users may want P4 alone, which is DbProof's bet.

## Checked by the main session (2026-11-19)

Re-run through the GitHub API and curl, not relayed:

- **DbProof:** org created 2026-10-01, 4 repos (agent, check-action, capture-action: Apache-2.0; prisma-example), 0
  stars each. Its site lists "Dropped column — Code still reading it breaks the moment the migration runs." with
  severity `warning`, the FAQ answer "Not yet: PostgreSQL and GitHub Actions today", and "Nothing during the beta".
- **migrations-safe:** MIT, 1 star, created 2026-10-30; README line 87 "The linter is regex-based";
  `skills/migrations-safe/references/rehearsal.md:121` lists "the N-1" under "Not covered".
- **Atlas lint is open source:** `migrateLintCmd()` is defined in `cmd/atlas/internal/cmdapi/migrate.go:781` and
  registered in `cmdapi.go:644`, with no build tag on either file.
- **strong_migrations#257** is closed; the maintainer's reply is quoted above.
- **curlewlabs-com** (sqlite-schema-gate, cloudflare-deploy-toolkit) is not an organisation the owner's GitHub account
  publicly belongs to (it belongs to one unrelated organisation). The agent flagged their writing style as close to this
  repo's; that remains for the owner to confirm.
