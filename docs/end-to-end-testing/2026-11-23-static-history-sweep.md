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
| kannon | dbmate | 29 |
| pages-editor | Payload | 40 |
| cv-manager | Payload | 23 |

## Results

261 commits. Documenso, netbox, maybe, kannon and pages-editor: nothing in all 189. The rest, and what each led to:

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
- **Payload migrations created without a name** (`payload migrate:create` writes `20241008_105308.ts`): migate's
  Payload layout required a `_name` after the timestamp, so these were never read. On pages-editor 9 of its 70
  migrations were silently left out (61 read before, 70 after); on cv-manager, whose early migrations are all unnamed,
  17 of 23 commits gave "nothing was checked". The layout now takes both.
- **One migration set per database adapter** (cv-manager keeps `src/migrations/postgres`, `.../mongodb` and
  `.../sqlite`): `migrations_dir` could not select one, since the layout still looked for a folder named `migrations`.
  It now reads the chosen folder as the tool's migrations folder, and when nothing is found migate names the folders
  that would work. With `migrations_dir` set to the PostgreSQL set, the one such commit tried reads its 10 migrations and
  checks clean; without it, 7 commits remain "nothing was checked", now with that hint.

After the fixes, the sweep was run again: the same exit codes everywhere, and on openstatus the new migration is the one
named. Basejump ran on `2040b46`; the other four on the build before `a9cc54a`, whose change concerns only a commit
removing every migration, which none of them has.

## Not covered

- Only static checks: no sandbox, so nothing about the schemas or the old versions' tests.
- A migration whose version moves later than the last one applied runs again under a timestamp-ordered tool; that is
  now MG001 (`6a4dafa`), tested on built input only: no real history here has one.
- A Wrangler D1 history was not swept (no suitable open-source project found).
