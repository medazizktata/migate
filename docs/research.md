# A migration guard for any migrator, database and host: research

Date: 2026-11-19. Question from the owner: could the migration protection built in the project it came from become a separate,
plug-and-play project for any database and any infrastructure?

**How this was made.** Two independent research agents (a landscape survey and a technical feasibility study), each told
to cite sources and label claims V (read the primary source), R (relayed) or U (unknown). Their labels are kept below. I
re-checked the claims a decision rests on: the local check's weaknesses (reproduced, and fixed in `cfd5508`), Atlas's
open-source analyzer packages (listed through the GitHub API), and Payload's history table (read in this repo). Prices,
licences and star counts are as the agents found them on 2026-11-19.

## 1. The protections in question

- **P1 Frozen:** a migration production has run can never be edited or removed.
- **P2 In order:** a migration dated before the last one production ran is refused.
- **P3 Matching production:** before a release, production's own migration history is compared with the repository.
- **P4 Not breaking:** a migration that breaks the app version still running during a deploy (dropped or renamed column,
  narrowed type, NOT NULL without default, table rebuild, and also new triggers or constraints that reject the old app's
  writes) is refused unless marked as the later "contract" step of an expand/contract change.
- **P5 One ordered release:** restore point, migrate, deploy, smoke test.

## 2. What exists today

| Tool | P1 | P2 | P3 | P4 | Engines | Licence and price |
|---|---|---|---|---|---|---|
| Atlas | atlas.sum (own files) | reports | Pro | analyzers parse SQL with a dev DB; **`migrate lint` paid since v0.38** | PG, MySQL, SQLite; no D1 | Apache core; Pro $9/dev/mo + $59/CI project/mo (V) |
| Flyway | CRC32 checksum | off by default | `validate` | Enterprise policy checks | JDBC (no D1) | Apache community; Enterprise by quote |
| Liquibase | MD5 per changeset | U | paid drift | paid policy checks | JDBC | **FSL (not OSI) since 5.0, 2025-09-30** (V) |
| Sqitch | SHA-1, `sqitch check` | plan order | `check` | no | several | MIT |
| Supabase CLI | **no** (versions only) | refuses older local files | `migration list`, refuses unknown remote versions | no | Supabase PG | MIT |
| Prisma Migrate | checksum, but `deploy` only **warns** | U | no | no | PG, MySQL, SQLite, D1 | Apache |
| Drizzle (stable) | hash stored, **never compared** | **skips older migrations silently, forever** | no | no | PG, MySQL, SQLite, D1 | Apache |
| Payload | no (name, batch) | runs anything not yet run | no | no | PG, SQLite, D1, Mongo | MIT |
| Wrangler D1 | no (name) | runs anything not yet run | no | no | D1 | Apache |
| strong_migrations | no | no | no | yes, Rails only | PG, MySQL | MIT, 88.6M downloads |
| django-migration-linter | no | no | no | yes, Django only | Django back ends | Apache, 4.5M/month |
| Squawk | no | no | no | partial, parses SQL | PG only | Apache, 7.4M npm/month |
| MigrationPilot | no | sequence checks | no | 112 rules, `--from-command` | PG only | MIT; Org $499/yr |
| pgroll / Reshape | n/a | n/a | n/a | runs expand/contract itself | PG | Apache / MIT |
| PlanetScale | n/a | queue | diff vs prod | warns, 30-min revert | Vitess only | hosting plan |

Smaller or context only: Bytebase (statement rules for MySQL/PG, MIT core, Pro $20/user/mo), Skeema (MySQL,
$1,238-4,988/yr), Neon schema diff (PR comment, no gate), Eugene (PG, runs SQL in a transaction to see locks), migra
(deprecated), gh-ost and pt-osc (MySQL online DDL executors).

Of 21 migrators studied in source, only 7 keep a hash or the statements (Flyway, Liquibase, Sqitch, Prisma, Drizzle,
Atlas, Supabase), and "pending" means different things: lexical order, version, a dependency graph (Django, Alembic,
Sqitch), or a single current version (golang-migrate, Alembic). TypeORM's out-of-order check is commented out in its source
(V).

## 3. The gaps

1. **D1 and serverless SQLite.** No free tool gives P1-P4 for Cloudflare D1. Wrangler, Payload and Drizzle were checked in
   source: none compares hashes or refuses out-of-order runs (V). Atlas has no D1 support; Flyway and Liquibase need JDBC,
   which D1 (HTTP API only) lacks. The only D1-specific attempts found were near-zero-star repositories from 2026.
2. **Breaking-change checks on SQLite** are paid (Atlas Pro) or absent.
3. **Migrations written in code** (Payload, Knex, TypeORM, EF, Rails, Django): SQL-parsing linters need the SQL first.
4. **P1-P3 across migrators:** each tool guards only its own table; Atlas imports five formats.
5. **No open tool provides P1-P5 for any migrator.** The contract escape hatch exists only inside single ecosystems
   (`safety_assured`, linter ignore lists, `squawk-ignore`).

The incident pattern P4 prevents is documented: Deno's 2021 outage came from dropping a column while old instances still ran
(V, deno.com post-mortem).

## 4. How it would work

Three layers; only the third depends on the database engine.

1. **The ledger (P1, P2, P5): no database access.** Hash every migration file production ran; the record lives in the repo
   and is **append-only against the base branch** (the bypass found in review: one pull request could edit a migration and
   rewrite its hash). Order rules must be per migrator, since "pending" differs: lexical for Payload, Wrangler, Rails;
   a graph check for Django, Alembic, Sqitch; a warning for golang-migrate and Drizzle, which skip silently.
2. **Production history (P3): one query per migrator.** Every migrator's table is one `SELECT`; reach it with the host's own
   client (`wrangler d1 execute --json`, `psql`, a URL). Read-only credentials, used only at release, never in pull-request
   CI. The authority should be a small table the tool writes in production at each release (name, hash, commit, time),
   with the committed ledger as the CI-side copy. For the 7 migrators that keep hashes, call their own validation instead.
3. **Breaking changes (P4): compare schemas, don't parse text.** Migrate a throwaway database to the base branch, read its
   schema; apply the pull request's migrations; read it again; classify the difference. This runs the user's own migrate
   command, so it works for code migrations and dynamic SQL, and it sees triggers, views and constraints, which the text
   check here cannot. Telling a rename from a drop plus an add is unnecessary for the gate: both break the old app. SQL
   parsing (sqlglot, pg_query) stays a secondary signal for data migrations (UPDATE, backfills), which no schema diff sees,
   and for lock warnings (Postgres ACCESS EXCLUSIVE, MySQL COPY algorithm).

**Reusable parts.** Atlas's schema inspectors and its `sqlcheck` analyzers (destructive, incompatible, datadepend, condrop)
are in its Apache-2.0 repository; only the `lint` command is paid (V, re-checked). Throwaway engines: SQLite in memory,
PGlite (Apache/PostgreSQL licence, under 3 MB), embedded Postgres, Testcontainers; startup times were not measured (U).
Atlas's open repo has no SQL Server inspector.

**Packaging.** A single Go binary built on Atlas's open packages, with a GitHub Action and npm/pip wrappers, is the
technical agent's recommendation; a TypeScript build would run PGlite and Miniflare in-process but lose breadth. A config
file names the engine, the migrator, the migrate command for a throwaway database (`DATABASE_URL` injected), the history
query, the ledger path and the contract marker.

## 5. A first version worth shipping

- **Engines:** a plug-in interface from the start; D1 and SQLite first (the verified gap), PostgreSQL second (to prove the
  interface generalises). The ledger and history layers work for any engine from the first release.
- **Migrators:** Payload, Wrangler D1, Drizzle, Prisma, then Rails and Django.
- **Protections:** the append-only ledger and per-migrator order (P1, P2), the history comparison (P3), the schema-diff gate
  (P4), with a `CONTRACT:` marker that must name its reason.
- **Tests:** one small real repository per migrator, with a known-bad case per rule, each rule shown failing on it and passing
  once fixed; an engine matrix in CI.

## 6. Risks, ranked

1. **Running another tool's migrations on a throwaway database** is the whole generality claim and the most work: code
   migrations need the app's runtime and config; Prisma needs CREATEDB; D1 needs Miniflare (U how stable its file layout is).
2. **Trusting the ledger:** 14 of 21 migrators store no hash, so the tool's own record carries the weight; it needs the
   append-only rule and a production-side copy.
3. **Migrators that skip silently** (stable Drizzle, golang-migrate): a generic order rule would give false confidence.
4. **D1 introspection over HTTP:** whether D1 accepts the table-valued pragma functions Atlas's SQLite inspector uses is U.
5. **Atlas licence drift:** its lint command is already paid; the open core could shrink.
6. **Attention:** of the first 100 GitHub results for "migration linter", 61 were created in 2026 and one has over 50 stars.

## 7. Verdict

**Worth building, if scoped.** For: the D1 / serverless-SQLite / code-migration corner is verifiably unserved (Payload's D1
adapter alone has about 194k npm downloads a month, R); incumbents are moving these checks behind paywalls (Atlas lint,
Liquibase's FSL, Flyway policies) while demand for free linting is large (Squawk 7.4M/month); and P1-P3 are cheap, proven
designs (Flyway, Sqitch) that need only one read of the history table. Against: the space is crowded with new projects;
"any engine" P4 is the large part, and every successful P4 tool picked one engine or one framework; platform owners may close
the gap (Drizzle v1 is redesigning its tracking; Supabase already refuses out-of-order runs).

**The goal is any database** (the owner's direction, 2026-11-19), reached through the architecture rather than promised
for every engine on day one. Layers 1 and 2 (the ledger, the production-history comparison) do not depend on the engine: they
cover every database from the first release, given one history query per migrator. Layer 3 is a plug-in per engine: start a
throwaway database, read its schema. **D1 is the first engine plug-in** (the verified gap), **PostgreSQL the second, early**,
to prove the plug-in interface generalises before more engines (MySQL, SQL Server, libSQL) are added, by the project or by
contributors.

## Open questions before starting

- Measure throwaway-database startup on a CI runner (SQLite, PGlite, a container).
- Confirm D1's support for `pragma_table_list()` and friends, and whether D1 runs each migration file atomically.
- Decide the licence (Apache-2.0 matches the reusable Atlas code) and whether a paid tier is ever intended, given that
  paywalls are exactly what users of this space resent.

## Main sources

Atlas lint and pricing: atlasgo.io/versioned/lint, atlasgo.io/pricing; Atlas analyzers: github.com/ariga/atlas
(sql/sqlcheck). Flyway: documentation.red-gate.com/fd/validate-277578898.html. Liquibase FSL:
liquibase.com/blog/liquibase-community-for-the-future-fsl. Supabase: supabase.com/docs/guides/deployment/database-migrations,
pkg.go.dev/github.com/supabase/cli/pkg/migration. Prisma: prisma.io/docs/orm/prisma-migrate/understanding-prisma-migrate/
migration-histories. Drizzle: drizzle-orm source (migrator.ts, dialect.ts), v1.0.0-beta.16 release notes. Wrangler:
workers-sdk src/d1/migrations/helpers.ts. Squawk: squawkhq.com/docs/rules. strong_migrations: github.com/ankane/
strong_migrations. pgroll: xata.io/blog/pgroll-expand-contract. Parallel change: martinfowler.com/bliki/ParallelChange.html.
Deno outage: deno.com/blog/2021-07-27-outage-post-mortem. PGlite: pglite.dev. SQLite ALTER TABLE: sqlite.org/
lang_altertable.html.
