# Table rebuilds and cascading children, on real tools: 2026-11-21

Run 9 of migate's end-to-end runs; migate built from the working tree before `MG104` was committed. How runs are
judged, and the others: `README.md`.

The question (TASK-33): when a migration rebuilds a parent table, do its `ON DELETE CASCADE` children survive, and
can migate tell without rows to count? Measured on SQLite with real tools, each parent holding one row and two
children.

| Tool | How it rebuilds the parent | Children after | Table's storage page |
|---|---|---|---|
| Plain SQL, foreign keys on (as D1 always is) | copy, drop the original, rename the copy | 0 of 2 | new |
| Plain SQL, foreign keys off | the same | 2 of 2 | new |
| Rails 8.1 (`change_column_null`) | copy aside, drop, create again under its own name, copy back | **0 of 2** | unchanged |
| Django 6.1 (`AlterField`, a `DB_CASCADE` child) | copy, drop the original, rename the copy | 2 of 2 | new |

- **Rails deletes the children.** It sets `PRAGMA foreign_keys = OFF` around the rebuild, but SQLite ignores that
  inside the migration's transaction (its log shows the pragma, then `DROP TABLE "owners"`).
- **Django keeps them.** Its schema editor refuses to run unless foreign keys are really off.

What migate does with this (MG104):
- A parent whose storage page changed (it was dropped and a copy renamed into place) and whose children cascade or
  set null on delete is an error, rows or not. Caught: a Drizzle-style rebuild (drizzle-orm#4938) with no child rows,
  and Django's real rebuild once its exemption is switched off.
- Django is exempt by data (`foreign_keys_off_for_rebuilds` in its migrator file): its real rebuild passes, as the
  measurement says it should.
- A rebuild that recreates the table under its own name, as Rails does, keeps the page and leaves no trace without
  rows. With a seed, MG105 counts the lost children. Not covered without one; the README says so.
- Every new test was seen failing first: the MG104 test on the old code, the no-false-alarm test with the rule firing
  for every delete action, and the exemption by switching it off on real Django.
