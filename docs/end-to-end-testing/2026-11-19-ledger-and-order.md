# Ledger and order checks on the origin project: 2026-11-19

Run 1 of migate's end-to-end runs. How runs are judged, and the others: `README.md`.

The project's 26 migrations through `migate check` with no sandbox. MG001, MG007 and MG004 fired where expected. Found:
adopting migate needs every migration production ran recorded at once (`migate ledger record --all` was added).
