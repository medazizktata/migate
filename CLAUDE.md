# migate: project notes for Claude

Read `README.md`, then `docs/research/README.md` (and the reports it links) and `docs/transcript.md` before proposing
anything: they hold the research, the design and the owner's direction (any database and any host; D1 is one host of
the SQLite engine and may be an early plug-in, never the product's identity; PostgreSQL early).

- Decided (2026-11-19, `docs/research/README.md` §6): name `migate` (pip package `migate-cli`), Rust, Apache-2.0, no
  revenue, SQLite then PostgreSQL in the first release, independent of any vendor. The primary check is a mixed-version
  test (the live version's tests or queries against the migrated database); the schema diff is the fast first pass and
  fallback. Anything not listed there: ask the owner, give a recommendation, do not assume.
- The prior art in `docs/prior-art/the Payload-on-D1 app/` is a starting point with known limits (see its ADR's review section): a
  text-based breaking-change check misses triggers, constraints, views and runtime SQL, and a schema diff alone misses
  breaking additions (`docs/research/blind-review.md`).
- Work directly on `main`; no feature branches while the project is early.
- Code structure follows `docs/architecture-decisions/0001-hexagonal-core.md`: no I/O in `migate-core`, ports only when a
  task needs them, migration tools as TOML data. Run `just check` (or `bash scripts/check.sh`) before reporting work done.
- Every rule the tool enforces gets a known-bad fixture it fails on and a known-good one it passes (calibrate both ways).
- Do not push or publish anything without the owner's say-so.
