# Sample apps, every bundled migrator on both engines: 2026-11-21

Run 8 of migate's end-to-end runs; migate `aef7d63`, a release build with its checksum recorded. How runs are judged,
and the others: `README.md`.

Minimal sample apps (one model, one test file each), one change per case. They test migate's wiring for each migrator
and engine, not real-world history. Ground truth without migate, on migate's own path on one database: base migrate,
base tests (control), head migrate, base tests (treat), on a throwaway PostgreSQL 18 server of its own or one SQLite
file. Prisma and Drizzle needed an online `pnpm install` first (their offline install had lost its package metadata);
Rails on SQLite needed the `sqlite3` gem.

| Stack | Case | Truth | migate | Outcome |
|---|---|---|---|---|
| Django, PostgreSQL | drop a column the code reads | break | block | caught |
| | add a nullable column | safe | pass | correct pass |
| | add a column with a Python-only default | break | block (MG203) | caught |
| Rails, PostgreSQL | drop a column no test reads | safe | block (MG201) | blocked, tests saw nothing |
| | add a nullable column | safe | pass | correct pass |
| | rename a column | break | block | caught |
| Prisma, PostgreSQL | drop a column | break | block | caught |
| | add a nullable column | safe | pass | correct pass |
| | add a required column | deploy fails | block (MG803) | caught |
| | add a unique constraint the rows break | deploy fails | block (MG803) | caught |
| Drizzle, PostgreSQL | rename a column | break | block | caught |
| | add an index and a column | safe | pass | correct pass |
| | add a CHECK constraint | safe | block (MG301) | blocked, tests saw nothing |
| Django, SQLite | drop a column the code reads | break | block | caught |
| | add a nullable column | safe | pass | correct pass |
| | a trigger refusing a write the code makes | break | block | caught |
| | rename a column | break | block | caught |
| Rails, SQLite | drop a column no test reads | safe | block (MG201) | blocked, tests saw nothing |
| | add a nullable column | safe | pass | correct pass |
| | rename a column | break | block | caught |

**20 cases: 11 caught, 6 correct passes, 3 blocked by a schema rule where the tests saw nothing, 0 missed.** The three
blocks are schema rules deciding on their own (a dropped column, a new CHECK), by the owner's decision that passing
tests do not soften them (TASK-60). 2 to 23 s per case.

All four renames now say "column dropped; or renamed to <new>" (TASK-79, fixed today); before, they read as an
unrelated drop and add.

Also checked on real tools, for TASK-34:
- **A failed migration that is not rolled back:** a Prisma migration whose first statement adds a column and whose
  second fails gives MG803 and names the column it left behind; the same failure in Django, which rolls back, gives
  MG803 alone.
- **A database the project does not write:** Django pointed at another file gives MG804 ("could not check"); a
  misnamed variable on an app with no fallback crashes Django, and migate shows the traceback and says it could not
  check.
