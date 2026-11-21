# Fix-verification drive: 2026-11-20

Run 4 of migate's end-to-end runs. How runs are judged, and the others: `README.md`.

A test agent drove a temporary build of the fixes from run 3. It found three regressions in those fixes, all fixed
before they shipped: a PostgreSQL query over 100 columns hit `json_build_object`'s argument limit; a backfill finding
had been softened to a note; and letting passing tests soften schema rules (TASK-60) let a real break through, so that
change was reverted.
