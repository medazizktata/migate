# Trigger-probe blind test: 2026-11-20

Run 5 of migate's end-to-end runs; TASK-70. How runs are judged, and the others: `README.md`.

A probe that re-wrote a table's rows to judge a new trigger was meant to turn trigger errors into warnings. A blind
agent built 8 changes it would have passed wrongly (a new column filled then required, a row-count limit, `RAISE
(IGNORE)` on update, and others). The downgrade was removed; trigger rules stay errors, each printing the marker that
accepts it.
