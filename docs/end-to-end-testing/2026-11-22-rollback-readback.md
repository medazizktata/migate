# Rollback readback on the origin project: 2026-11-22

Run 12 of migate's end-to-end runs; migate `800d159`. How runs are judged, and the others: `README.md`.

MG812 needs a project `readback` command that reads every row with the old version's code. The origin project (Payload
on D1) has none, so the dogfood harness supplied one that any Payload app could use: through Payload's Local API, load
every document of every collection, in every locale, and every global. migate copies it into the old version's
temporary checkout; the project itself is not changed. Mock data only, with the project's seed for rows.

| Case | The new version | Expected | migate |
|---|---|---|---|
| bad | rewrites a JSON field as plain text (`leather; heated`) | the old version cannot load it | blocked, MG812: the old code's `SyntaxError` while loading the rows |
| good | rewrites the same field as a JSON list | the old version loads it | passed |
| no change | nothing | pass | passed |

The bad case failing is also the proof that the readback really loads the rows (a stub would pass it). Each run took
about 4.5 minutes, some 40 seconds more than without the readback.

Not covered: what a Local API read does not exercise (the project's own API mappers and pages); a readback for
non-Payload projects is theirs to write.
