# Test drive 1: a blind agent uses migate on real projects

Date: 2026-11-19. Commit tested: `15d8e3a`. A context-free agent was given the repository (read-only), the build
command and the documentation, and asked to establish how migate behaves on realistic projects and whether the
documentation's claims hold. It was not told what the authors believed worked. The brief is in the session scratchpad;
its six areas were getting started, the ledger checks, real projects through `[sandbox]`, output formats, messages, and
side effects.

**How to read this.** Everything below is the agent's report, condensed. Lines marked *(verified)* were re-checked by
the main session against the code or the disk; the rest is relayed. Its projects, scripts and raw outputs lived in the
session scratchpad and are not kept.

Toolchains: Python 3.14.8 (SQLite 3.53.4) with Django 6.1.3; Ruby 4.0.10 with Rails 8.1.4.1; Node 26.10 with
drizzle-kit 0.31.11, prisma 6.19.3, wrangler 4.165.0. Binary md5 `2c586eb14d6742cb368fe36414ae1cb0`, re-checked before
each block.

## Verdict

What held:
- The migration record and order rules (MG001-MG007) on all six bundled migrators. The Drizzle silent skip (MG004) is
  real: drizzle-kit printed "migrations applied successfully" while skipping the migration.
- Running the base version against the head-migrated database caught every added-column break the schema diff only
  noted: Django d03, Rails r03, Drizzle z02 and z03, Prisma p03.
- Normal runs leave nothing behind. SARIF validates against the OASIS 2.1.0 schema (validator calibrated).

What did not (each now a task):

| # | Finding | Task |
|---|---|---|
| 1 | A test command that never touches the database passes: `test = "true"`, or stock Django `manage.py test` (its own `test_` database), gave exit 0. A command that cannot start (exit 127) was reported as MG810, exit 2. *(verified: no such guard in `sandbox.rs`)* | TASK-45 |
| 2 | Data loss passes: drizzle-orm#4938 reproduced (a parent rebuild cascade-deleted `order_items` 2 to 0) on Drizzle z05 and D1 w02; a `DELETE` migration (d09, 3 rows to 1); all exit 0. *(verified: `docs/product.md` promised otherwise)* | TASK-33, TASK-47 |
| 3 | The migration list comes from walking the disk: an untracked virtualenv `env/` added 23 Django contrib migrations, `ledger record --all` recorded them, a clean CI clone then failed with 23 x MG002. *(verified: `project.rs` skips folders by name only)* | TASK-46 |
| 4 | No way to pass a safe contract step (Rails `ignored_columns`, r02); MG205 errors on SQLite declared lengths SQLite does not enforce (d10); MG006 errors for Wrangler, which applies both duplicates | TASK-19, TASK-48 |
| 5 | A run killed with SIGTERM leaves two worktrees and about 455 MB in the temp folder *(verified on disk: 494 MB, two worktrees)* | TASK-49 |
| 6 | Locations and messages: schema findings point at the ledger file; exit 3 prints no JSON; MG810 shows a 15-line tail; MG803 says "listed here" with none listed; renames read as MG201 + MG203 | TASK-50 |
| 7 | `ledger record --all \| head` panics; unknown ids are not listed; `--help` lists only MG001-MG007 | TASK-51 |
| 8 | D1 setup: the hint leads to an MG804 that blames a variable Wrangler does not read, `*.sqlite` also matches `metadata.sqlite`; `npx` installed `prisma@8.0.0-rc.34` from npm; `BUNDLE_`/`GEM_`/`CARGO_` variables are passed wholesale | TASK-52 |
| 9 | `docs/product.md`'s status line is stale and several claims describe unbuilt parts *(verified)* | TASK-47 |

## What migate said against what happened

Ground truth from the agent's own harness: seed the base-migrated database, migrate to head, run the base app's code
(Rails booted before the migration).

| Change | migate (exit) | Reality | Match |
|---|---|---|---|
| Django d01 drop column | MG201, MG810 (2) | old inserts fail | yes |
| d02 add NOT NULL, Python-side default | MG203, MG810 (2) | NOT NULL failure | yes |
| d03 add nullable column | MG810 (2) | `SELECT *` report breaks | yes |
| d04 rename column | MG201, MG203, MG810 (2) | breaks | yes, as drop + add |
| d05 add index | 0 | fine | yes |
| d06 add UNIQUE | MG302 (2) | fails on duplicates | yes |
| d08 add table | 0 | fine | yes |
| d09 DELETE data migration | 0 | 3 rows to 1 | **no** |
| d10 varchar 20 to 5 | MG205 (2) | fine on SQLite | **no**, false alarm |
| d11 ADD NOT NULL, no default | MG203, MG810 (2) | fails on non-empty tables | yes |
| d12 set NOT NULL | MG204, MG810 (2) | fails | yes |
| Rails r01 drop column | MG201, MG810 (2) | fails | yes |
| r02 drop an `ignored_columns` column | MG201 (2) | fine | **no**, false alarm |
| r03 add nullable column | MG810 (2) | report breaks | yes |
| r05 rename column | MG201, MG810 (2) | fails | yes |
| r06 string to integer | MG205 (2) | fine | in between |
| r07 add index | 0 | fine | yes |
| r08 drop unused, not-ignored column | MG201 (2) | pre-booted insert fails | yes, by the diff only (no pre-boot) |
| Drizzle z01 drop column | MG201, MG810 (2) | fails | yes |
| z02 add NOT NULL with default | MG810 (2) | breaks | yes |
| z03 add nullable column | MG810 (2) | breaks | yes |
| z04 change default (rebuild) | MG209 (2) | `order_items` 2 to 0 | right verdict, wrong reason |
| z05 relax NOT NULL (rebuild) | 0 | `order_items` 2 to 0 | **no** |
| z06 add index | 0 | fine | yes |
| z07 late migration, ledger present | MG004 (2) | silently skipped | yes |
| z08 late migration, no ledger yet | 0 | silently skipped | **no** |
| Prisma p01 drop column | MG201, MG810 (2) | fails | yes |
| p02 add required column | MG203, MG810 (2) | fails (P3018) | yes |
| p03 add optional column | MG810 (2) | breaks | yes |
| p04 add unique | MG302, MG810 (2) | fails | yes |
| p05 add index | 0 | fine | yes |
| D1 w01 drop column | MG201, MG810 (2) | fails | yes |
| w02 rebuild | 0 | `order_items` 2 to 0 | **no** |
| w03 add nullable, explicit columns | MG210 (0) | fine | yes |

27 of 34 match. Every miss is data loss or a skipped migration; every false alarm is a safe change with no override.

## Not reached

Payload through `[sandbox]`, the Rails pre-boot mode (not built), RSpec, the release build, Postgres (not built), the
enum and sequence rules, several database files, a hanging test command, SIGINT from a terminal.

## What the agent would do instead

Keep the core (the base version against the head-migrated database, plus the ledger and order rules), and: prove the
tests touched the sandbox; detect data effects with seeded row counts and the cascade-rebuild rule; make rules
engine-aware; ship the contract marker with a required reason; list migrations from git; never let a migrate command
download packages; handle SIGTERM; point findings at the migration that introduced them.
