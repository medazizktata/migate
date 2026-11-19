# Spike TASK-31: what the mixed-version test costs and catches

Date: 2026-11-19. Question: the owner made the mixed-version test the primary check (`blind-review.md` §2): migrate a
throwaway database to the pull request's head, then run the still-live (base) version's tests against it. What does it
cost, how flaky is it, and what does it catch that a schema diff does not?

**How this was made.** A research agent built fixtures and a harness on three projects: a Django sample, a Rails 8.1
sample (SQLite and Postgres each) and a clone of the Payload-on-D1 app (Payload on local D1). It ran every fixture 5-7 times. The main
session re-ran the decisive cases (section 5). Harness and per-run data: `bench/spike-31/` (`results.jsonl` has one line
per run; `harness/summarise.py` rebuilds the tables with `SPIKE_DIR` set to a work folder).

## 1. The verdict

- **The mixed-version test catches what the schema diff cannot:** the GitLab #5171 class (a column added, and the old
  version's own SQL breaks), in both Django and Rails. On Postgres the failure is GitLab's exact message, "each UNION
  query must have the same number of columns".
- **For Rails, a fresh run of the old tests is not enough.** The old version must be booted *before* the migration, then
  exercised: a fresh process reloads the schema and misses both headline incidents (GitHub August 2025 and GitLab
  #5171). Booting first is what makes the catch; migrating before booting makes it disappear (calibrated both ways).
- **Cost is about one integration-suite run per live version:** about 113 s for the Payload-on-D1 app, 1-4 s for the small samples.
- **No flakiness:** about 84 Django, 78 Rails and 15 payload-d1 runs, identical outcomes on every repeat.
- **The schema diff stays useful as the sub-second first pass,** but it raises false alarms on a safe contract step (F6)
  unless the per-ORM reference rules are applied.

## 2. Results

Fixtures: **F1** drop a column the old code uses; **F2** NOT NULL without default; **F3** the #5171 class (a nullable
column added; the old version's raw `SELECT * … UNION ALL`, or Rails' cached `column_names`); **F4** a type change;
**F5** a safe nullable addition; **F6** a safe drop of a column the old code no longer declares (`ignored_columns` in
Rails).

| Project / engine | Fixture | Mixed-version test | Schema diff |
|---|---|---|---|
| Django, SQLite + PG | F1 | FAIL (4/4 tests) | flags DROP_COLUMN |
| Django | F2 | FAIL (NOT NULL violation) | flags |
| Django | F3 | **FAIL**; PG: "each UNION query must have the same number of columns" | **misses** |
| Django | F4 | FAIL; rollback readback FAIL | flags |
| Django | F5 | PASS | PASS |
| Django | F6 | PASS | **false alarm** |
| Rails, SQLite + PG | F1 (GitHub Aug 2025) | fresh PASS (**miss**); pre-booted **FAIL** ("table orders has no column named legacy_code" / `PG::UndefinedColumn`) | flags |
| Rails | F2 | fresh FAIL; pre-booted FAIL | flags |
| Rails | F3 (#5171) | fresh PASS (**miss**); pre-booted **FAIL** (PG: the #5171 message) | **misses** |
| Rails, SQLite | F4 | fresh FAIL; pre-booted tests PASS; pre-booted readback FAIL: "stored 'shipped', read as 0" (silent corruption) | flags |
| Rails, PG | F4 | fresh FAIL; pre-booted FAIL; pre-booted readback FAIL | flags |
| Rails | F5 | PASS | PASS |
| Rails | F6 | PASS | **false alarm** |
| Payload on D1, local D1 | control (head = base) | PASS (SQL suite 122, admin integration 203, API integration 76) | PASS |
| Payload on D1 | F1 dropping a column the old app reads | **FAIL** (integration tests: "D1_ERROR: table … has no column named …"); the SQL-level suite passed, as none of its 122 tests touches that column | flags |
| Payload on D1 | F5 a new nullable column | PASS | PASS |

**Rollback compatibility** (rows written by the new version, read by the old one): the plain test run never reads such
rows, so it does not cover the CircleCI pattern alone. An explicit readback did: Django's `full_clean()` on every row
caught F4; in Rails only a readback *inside the pre-booted process* caught F4.

## 3. Cost (one loaded Mac, not a CI runner)

| Project | Setup | Migrate | Tests | One mixed-version run |
|---|---|---|---|---|
| Django, SQLite | 0.16-0.21 s | 0.24-0.47 s | 0.26-0.72 s | 0.66-1.38 s |
| Django, PG | 0.24-0.35 s | 0.38-0.52 s | 0.37-0.52 s | 1.06-1.29 s |
| Rails, SQLite + PG | 0.15-0.24 s | base 1.1 s + head 1.1 s | 0.9 s (mostly boot) | 3.2-3.6 s; pre-booted the same |
| Payload on D1, control | 18.4 s (install 8.5 + base migrate 8.3) | 2.6 s | 92.4 s (SQL 6.3, admin int 70.9, API int 15.3) | **113.4 s [111.3-119.1]**, n=5 |
| Payload on D1, F1 | 18.0 s | 2.7 s | 70.2 s | 90.9 s (fails early) |
| Payload on D1, F5 | 18.0 s | 2.6 s | 91.3 s | 111.9 s |

The schema diff costs 0.04 s on the Payload-on-D1 app. The project's own full `pnpm test:int` takes 148 s.

## 4. What a runner must do, and what a team must provide

**Per framework (calibrated):**
- **Django:** `manage.py test --keepdb` with `TEST.NAME` set to the head-migrated database. Without `--keepdb` Django
  rebuilds the database from the base migrations and F1 becomes a false PASS (shown); a guard now marks such a run
  INVALID because the head migration record is gone.
- **Rails:** `DATABASE_URL` at the migrated database, `PARALLEL_WORKERS=1` (workers rebuild their databases from
  `schema.rb`), `maintain_test_schema=false` (when the database is built straight at head with head's `schema.rb`, the
  default silently purged it and loaded base's schema: "4 runs, 0 failures"), and a pre-boot harness (about 30 lines):
  boot the base app, warm its schema cache, wait while the tool migrates to head, then run the base suite and the
  readback in that same process.
- **the Payload-on-D1 app:** a fresh worktree of base, its local D1 migrated with the base chain, then head's migrations folder over
  it and `pnpm db:migrate` again; two tests that check the migration chain itself (`migrations.test.ts`,
  `schema.test.ts`) fail on every head, safe ones included, and must be excluded.

**Guards that proved necessary:** a stub test command (`/bin/echo`) is reported INVALID, not PASS; a run whose database
no longer holds the head migration is INVALID; a SQLite snapshot must use the backup API (D1 keeps data in the WAL, and
a plain file copy gave a silently empty schema, now caught by a guard that exits 2 on an empty schema).

**A team must provide:** tests that write and read the affected tables (payload-d1's SQL suite alone missed F1); a way to point
the old suite at an externally migrated database; the list of migration-chain tests to exclude; test secrets (payload-d1 needs
`PAYLOAD_SECRET`, as its CI writes); and a seed command in the new version for the rollback readback.

**The "load old models, run their queries" fallback** is a fast first layer, not a replacement: on the Payload-on-D1 app it loads
base's 36 Drizzle tables and selects every declared column in 0.57 s (F1 fails with "no such column"; control
and F5 pass), against about 91 s for the tests. It catches drops, renames and type changes on ORMs that declare their
columns; it misses write-path breakages (F2), the team's own raw SQL (F3) and the whole schema-cache class (Rails).

## 5. Re-checked by the main session

- **Django, SQLite:** F3 → mixed-version FAIL (`test_all_orders_raw`) with an empty schema diff; F5 → PASS.
- **Rails, SQLite, F1:** fresh PASS, pre-booted FAIL ("table orders has no column named legacy_code") with the diff
  flagging `DROP_COLUMN orders.legacy_code`; the same fixture with `--migrate-before-boot` → pre-booted PASS (so boot
  order is what makes the catch); control (`base`) → all PASS.
- **the Payload-on-D1 app probe:** F1 fails in 0.56 s with `no such column`; F5 passes 5/5 in 0.57 s.

Not re-checked: the Postgres runs, the full the Payload-on-D1 app test runs and their timings, the flakiness counts.

## 6. Not tested

MySQL; remote D1; a CI runner; more than one live version at once; old and new versions writing concurrently; data
volume, locks and timeouts; Django in a pre-booted process (assumed to have no schema cache, unverified); Rails Postgres
"cached plan must not change result type"; the Payload-on-D1 app NOT NULL, type-change, #5171 and rollback fixtures; the Payload-on-D1 app head
commits with code changes; e2e suites; Django renames.

**Side finding in the Payload-on-D1 app:** its D1 runner applies every `.ts` file in the
migrations folder, while the SQL test helper follows `index.ts`; a stray file in the folder is applied by one and not
the other.
