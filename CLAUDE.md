# migration-guard: project notes for Claude

Read `README.md`, then `docs/research.md` and `docs/transcript.md` before proposing anything: they hold the research,
the design and the owner's direction (any database; D1 first; PostgreSQL second).

- Nothing is decided about name, language or licence: ask the owner, give a recommendation, do not assume.
- The prior art in `docs/prior-art/the Payload-on-D1 app/` is a starting point with known limits (see its ADR's review section): a
  text-based breaking-change check misses triggers, constraints, views and runtime SQL; the research favours comparing
  schemas of a throwaway database before and after.
- Every rule the tool enforces gets a known-bad fixture it fails on and a known-good one it passes (calibrate both ways).
- Do not push or publish anything without the owner's say-so.
