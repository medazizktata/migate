# End-to-end testing

Every run of migate on real projects so far: what was run, how its ground truth was set, what it found, and what it
changed. The test suite and CI check migate's code; these runs check whether it gives the right verdict on a real
project's migrations and tests.

The main subject is the project migate started from, a Payload app on Cloudflare D1 (local mode), described here only
generically. Its harness drives that private project and is kept outside this repository.

## How a run is judged

Each case is a base (the version still serving traffic) and a head (the change), and its truth is set without migate:

- **The deploy:** does head's migration fail on the database base's tests left behind? Then migate must block.
- **The old version:** base's own tests on that database, before (control) and after head's migration (treat). A
  break is a test that passed in the control and not in the treat. Tests already failing in the control are ignored.
- **Data loss:** rows and non-empty values counted directly just before and just after head's migration.
- **By construction:** rules whose consequence cannot be seen locally (an edited migration production already ran, an
  engine older than the local one). The case is built to break the rule, and must be checked to fail for that reason.
- **Probes and controls:** a bad setup must give "could not check" (exit 3), and a case with no change must pass.

Outcomes: caught, missed, correct pass, false alarm, and "needs a trigger marker" (a block resting only on trigger
rules, MG503/MG504, where the tests saw nothing: by the owner's decision these stay errors until accepted with a
`migate: contract` marker).

## The runs, in order

One document per run, named by its date.

| # | Date | Run | Document |
|---|---|---|---|
| 1 | 2026-11-19 | Ledger and order checks on the origin project | `2026-11-19-ledger-and-order.md` |
| 2 | 2026-11-19 | Test drive 1: a blind agent on five stacks | `2026-11-19-test-drive-1.md` |
| 3 | 2026-11-19 to 20 | Dogfood 2 and the PostgreSQL drive | `2026-11-20-dogfood-2-and-postgres-drive.md` |
| 4 | 2026-11-20 | Fix-verification drive | `2026-11-20-fix-verification.md` |
| 5 | 2026-11-20 | Trigger-probe blind test | `2026-11-20-trigger-probe.md` |
| 6 | 2026-11-20 | Dogfood 3 | `2026-11-20-dogfood-3.md` |
| 7 | 2026-11-21 | Dogfood 4 | `2026-11-21-dogfood-4.md` |
| 8 | 2026-11-21 | Sample apps, every bundled migrator on both engines | `2026-11-21-sample-apps-all-migrators.md` |
| 9 | 2026-11-21 | Table rebuilds and cascading children, on real tools | `2026-11-21-rebuild-cascades.md` |
| 10 | 2026-11-21 | migate history on real migrators' history tables | `2026-11-21-history-on-real-tools.md` |
| 11 | 2026-11-21 | Dogfood 5 | `2026-11-21-dogfood-5.md` |
| 12 | 2026-11-22 | Rollback readback on the origin project | `2026-11-22-rollback-readback.md` |
| 13 | 2026-11-22 to 23 | Dogfood 6: twelve profiles, real PostgreSQL projects, two open-source projects, blind labels | `2026-11-23-dogfood-6.md` |
| 14 | 2026-11-23 | Static checks over eight open-source projects' migration history | `2026-11-23-static-history-sweep.md` |
| 15 | 2026-11-24 | documenso with its own test suite: the primary check on outside code | `2026-11-24-documenso-own-tests.md` |
| 16 | 2026-11-24 | Payload on PostgreSQL, the last bundled migrator not yet run on it | `2026-11-24-payload-on-postgres.md` |

## Not covered yet

- A PostgreSQL project's own test suite beyond one project and one migrator (run 15 is documenso, Prisma).
- MG002, MG003, MG007 end to end, MG105 by a table rebuild, MG801 and MG805, and a project's own contract-marker
  convention.
- One-off statements (`ALTER TABLE ... DROP COLUMN` and the like) inside TypeScript, Python or Ruby migrations, for the
  syntax-version check (MG701): reported as not checked (MG801). What such migrations leave in the schema is checked.
