# Static checks over open-source history: 2026-11-23

Run 14 of migate's end-to-end runs. How runs are judged, and the others: `README.md`.

## What was run

migate's checks without a sandbox (ledger, order, dependency graphs) over the last 40 commits that touched migrations on
the main line of five open-source projects, each commit against its parent as the base. Merged history deployed, so an
error there is a candidate false alarm, and a commit migate could not check is a candidate bug.

| Project | Migrator | Commits |
|---|---|---|
| documenso | Prisma | 40 |
| netbox | Django | 40 |
| maybe | Rails | 40 |
| openstatus | Drizzle | 40 |
| basejump | Supabase | 9 (all it has) |

## Results

169 commits. Documenso, netbox and maybe: nothing in all 120. The other four results, and what each led to:

- **basejump, a migration added before the ones already shipped** (`00000000000000_...` after `...0001` to `...0011`):
  MG004. Correct: the Supabase CLI refuses an out-of-order migration unless forced. The same commit also edited a
  shipped migration, which only a ledger would show.
- **openstatus, a new migration given an applied one's journal time**: the change moved an applied migration's time
  earlier and gave the new one its old time. Drizzle runs only entries newer than the last time it recorded, so the
  new one is skipped forever. migate blocked it, but named the applied migration and advised renumbering that one,
  which would run it twice. Fixed (`3b77a11`): a migration is pending by its name, a version stays taken only while no
  remaining migration answers for it, and an equal version counts as skipped where the tool skips silently. Rerun: the
  new migration is named.
- **basejump, a commit removing every migration** (the project moved to another layout): "no migrations found, nothing
  was checked". Right without a ledger, but it showed that with one, the ledger's MG002 for each removed migration was
  never reached. Fixed (`46484ae`, then `a9cc54a` after the first fix let the no-ledger case pass): with a ledger
  recording migrations each removal is MG002; without one it stays "nothing was checked".
- **basejump's first commit**: no parent to compare with; a limit of the sweep, not of migate.

After the fixes, the sweep was run again on `2040b46`: the same exit codes everywhere, and on openstatus the new
migration is the one named.

## Not covered

- Only static checks: no sandbox, so nothing about the schemas or the old versions' tests.
- A migration whose version moves later than the last one applied would run again under a timestamp-ordered tool;
  migate does not flag it yet.
- Payload, Wrangler D1 and dbmate histories were not swept.
