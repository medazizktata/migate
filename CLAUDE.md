# migate: project notes for Claude

Read `README.md`, then `docs/research/README.md` (and the reports it links) and `docs/transcript.md` before proposing
anything: they hold the research, the design and the owner's direction (any database and any host; D1 is one host of
the SQLite engine and may be an early plug-in, never the product's identity; PostgreSQL early).

- Decided (2026-11-19, `docs/research/README.md` §6): name `migate` (pip package `migate-cli`), Rust, Apache-2.0, no
  revenue, SQLite then PostgreSQL in the first release, independent of any vendor. The primary check is a mixed-version
  test (the live version's tests or queries against the migrated database); the schema diff is the fast first pass and
  fallback. Anything not listed there: ask the owner, give a recommendation, do not assume.
- migate stands alone; it has branched off from the project it started in. Files and commit messages never name or
  describe the owner's other projects: no names, paths, schemas, test names or internal documents. Evidence gathered on
  one is written generically ("a Payload app on D1"). Known limits of the approach it started from: a text-based
  breaking-change check misses triggers, constraints, views and runtime SQL, and a schema diff alone misses breaking
  additions (`docs/research/blind-review.md`).
- Work directly on `main`; no feature branches while the project is early.
- Code structure follows `docs/architecture-decisions/0001-hexagonal-core.md`: no I/O in `migate-core`, ports only when a
  task needs them, migration tools as TOML data. Run `just check` (or `bash scripts/check.sh`) before reporting work done.
- Every rule the tool enforces gets a known-bad fixture it fails on and a known-good one it passes (calibrate both ways).
- Do not push or publish anything without the owner's say-so.

## Commit messages: short

Follows the widely used seven rules (cbea.ms/git-commit) and Git's own SubmittingPatches. `.githooks/commit-msg`
enforces what a script can check.
- **Subject:** what the commit does, in the imperative ("Add", "Fix", "Move", not "Added"), capitalised, no full stop,
  50 characters or fewer (72 at most).
- **Body, only if the subject is not enough:** a blank line, then at most about 6 lines wrapped at 72. Say **why**, and
  what a reviewer cannot see in the diff: a trade-off, a risk, a deliberate limit.
- **Verification, one line at most:** `Tested: ...` and, when something is not, `Not tested: ...`. Never claim a check
  that did not pass in this commit's state.
- **Leave out:** file lists, test counts, sabotage runs, a recap of the diff, the session's story. Those go in the task
  or the docs.
- One logical change per commit. A message stands alone: a tracker ID is never the only reference.

```
Clean up cancelled and killed sandbox runs

Drop never runs on a signal, so a cancelled CI job left worktrees
behind. A signal now stops the running command and removes the run,
and each run removes what dead runs left.

Not tested: that another user's folders are left alone.
```
