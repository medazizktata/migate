# Spike TASK-7: real migrators in the two-checkout sandbox

Date: 2026-11-19. Question: does "two checkouts, one sandbox" work with real migrators, and what does each need? Before
this, only Postgres with a toy migrator had been shown (`architecture.md` §13, risk 1).

**How this was made.** A research agent built the flow and ran it; the main session re-ran part of it (section 4).
Scripts are in `bench/spike-7/`. Everything ran on one heavily loaded Mac (Apple M3 Pro), most flows also inside a
`sandbox-exec` jail that denies non-loopback network and writes outside the work folder (calibrated: `curl` fails inside,
works outside; a write to `~/Desktop` is refused inside).

## 1. The flow

Reset the sandbox; check out `base`; run the project's own migrate command; snapshot the schema (A); check out a head
branch; migrate again on the same database; snapshot (B); diff. Four head branches per project: `head-good` (one nullable
column), `head-bad` (adds a column, then alters a table that does not exist), `head-noopmig` (a migration that does
nothing), `head-noopcommit` (no migration change). Schema readers: `snap_sqlite.sh` (reads a copy of the file, `-wal`
included) and `snap_pg.sh` (catalog queries).

## 2. Results

| Migrator / engine | Verdict | Good head | Bad head | No-op heads |
|---|---|---|---|---|
| Payload, the Payload-on-D1 app runner (`pnpm db:migrate`), local D1 | works with conditions | only the new column | exit 1, rolled back | exit 0, empty diff |
| Payload, stock `payload migrate`, local D1 | works with conditions | only the new column | exit 1, **first statement left applied** | exit 0, empty diff |
| Prisma 7.10.1 `migrate deploy`, SQLite | works | only the new column | exit 1 (P3018), **first statement left applied** | exit 0, empty diff |
| Prisma, Postgres | works (creates the database) | only the new column | exit 1, **first statement left applied** | exit 0, empty diff |
| Django 5.2.19 `manage.py migrate`, SQLite | works with conditions | only the new column | exit 1, rolled back | exit 0, empty diff |
| Django, Postgres | works with conditions (database must exist) | only the new column | exit 1, rolled back | exit 0, empty diff |
| Rails 8.1.4.1 `bin/rails db:migrate`, SQLite | works with conditions | only the new column | exit 1, rolled back | exit 0, empty diff |
| Rails, Postgres | works with conditions (database must exist) | only the new column | exit 1, rolled back | exit 0, empty diff |

No migrator needed network at migrate time once dependencies were installed.

**Timings** (mean ± sd, s; loaded machine, so an upper bound; dependency installs excluded):

| | n | base (full chain) | head (one migration) | nothing pending |
|---|---|---|---|---|
| Payload, the Payload-on-D1 app runner (26 migrations) | 5 | 6.43 ± 3.08 | 4.65 ± 2.31 | 4.65 ± 1.96 |
| Payload, stock | 5 | 9.48 ± 2.26 | 4.46 ± 0.79 | 5.06 ± 2.45 |
| Prisma, SQLite | 8 | 1.99 ± 0.45 | 1.74 ± 0.57 | 1.71 ± 0.60 |
| Prisma, Postgres | 8 | 1.16 ± 0.20 | 1.13 ± 0.26 | 1.19 ± 0.35 |
| Django, SQLite | 8 | 0.55 ± 0.14 | 0.40 ± 0.18 | 0.41 ± 0.16 |
| Django, Postgres | 8 | 1.00 ± 0.35 | 0.83 ± 0.28 | 0.63 ± 0.22 |
| Rails, SQLite (production env) | 8 | 1.20 ± 0.06 | 1.10 ± 0.07 | 1.05 ± 0.02 |
| Rails, Postgres (production env) | 8 | 1.23 ± 0.13 | 1.14 ± 0.05 | 1.14 ± 0.07 |

Most of each figure is the migrator starting up.

## 3. What "inject only a connection setting" really needs

It holds for none of them as a blanket rule:

- **the Payload-on-D1 app' Payload runner accepts no connection setting at all.** The database path is fixed by
  a script of the app's (`<checkout>/<app>/.wrangler/state/v3/d1/miniflare-D1DatabaseObject/<hash>.sqlite`);
  the only knob is `APP_DATA=mock|real`. The sandbox is the checkout itself. It needs Node 26, pnpm 12 and a full
  workspace install (10 s from a warm store, about 2.3 GB of `node_modules`).
- **Stock `payload migrate`** also needs `PAYLOAD_SECRET` ("missing secret key", exit 1) and the whole
  `payload.config.ts`.
- **Prisma:** `DATABASE_URL` only; it creates a missing Postgres database itself (needs CREATEDB).
- **Django:** the stock `startproject` settings hard-code the database; the common `dj_database_url` pattern works; a
  dummy `DJANGO_SECRET_KEY` with the common `os.environ[...]` pattern; the database must exist.
- **Rails:** `DATABASE_URL` plus `RAILS_ENV=production` and `SECRET_KEY_BASE_DUMMY=1` (a CI checkout has no
  `master.key`); the database must exist. In development it needs no secret but rewrites the tracked `db/schema.rb`,
  leaving the tree dirty across checkouts.

## 4. Surprises, and what the main session re-checked

1. **Failed migrations can stay half-applied.** Prisma (both engines) and stock Payload on local D1 left the bad
   migration's first statement in place; the the Payload-on-D1 app runner, Django and Rails rolled back. **Re-checked:** Prisma on
   SQLite, `head-bad`: exit 1 (`Error: P3018`), and snapshot B shows the column `ok` added; Django on SQLite, `head-bad`:
   exit 1 and an empty diff. Both `head-good` runs showed only the new `note` column.
2. **A misnamed connection variable can pass silently.** **Re-checked:** with `DB_URL` instead of `DATABASE_URL`,
   Django exited 0, created and migrated the project's own `db.sqlite3` (19 rows in `django_migrations`), and never
   created the intended database; the control run with `DATABASE_URL` created it. The agent saw the same with Rails in
   development (relayed).
3. **SQLite's WAL matters.** A copy of Payload's D1 file without its `-wal` showed a stale migration table (stopping at
   `20261118_232700`); with the WAL it had all 27 rows (relayed).
4. **Writes outside the project** (relayed): wrangler logs to `~/Library/Preferences/.wrangler/logs/` and carries on when
   denied; Prisma writes `~/Library/Preferences/prisma-nodejs/commands.json` and a telemetry checkpoint under
   `~/Library/Caches/checkpoint-nodejs/`, and tolerates being offline. Django and Rails wrote nothing to HOME.

Not re-checked by the main session: the Payload and Rails runs, the Postgres runs, the timings, the jail runs.

## 5. What this changes in the design

- **The migrator adapter needs more than a URL:** an env map with per-migrator extras (`PAYLOAD_SECRET`,
  `DJANGO_SECRET_KEY`, `RAILS_ENV` + `SECRET_KEY_BASE_DUMMY`, project knobs like `APP_DATA`), a declared database
  **location** for project-defined layouts (a path template inside the worktree), and `sandbox.create()` always
  creating the database (works for all four).
- **Guards in the core (new rules):**
  - **MG804 the sandbox was not the database migrated:** after the base run, assert the sandbox holds the migrator's
    history table with the expected rows; catches the silent wrong-variable case. Not yet built or calibrated.
  - **MG803 a failed migration left changes behind:** any non-zero exit is a hard failure, and the diff after it is
    reported as "this migration is not atomic".
  - A clean-tree check (`git status`) before each checkout, or a forced checkout.
  - Readers go through a transport or copy the WAL with the file.
- **Risk 1 in `architecture.md`** is partly retired for Payload, Prisma, Django and Rails on SQLite and Postgres, on one
  developer machine.

## 6. Not tested

A `git worktree` per sandbox (a clone was used); the stock Payload template's default `getPlatformProxy` path; reading
D1 through `wrangler d1 execute`; remote D1; a cold machine (Prisma's engine was already cached); Rails multi-database
setups and a real master key; Django with several databases; out-of-order, skipped and data migrations; MySQL, PGlite
and container providers; parallel sandboxes; reader completeness beyond columns; idle-machine timings.
