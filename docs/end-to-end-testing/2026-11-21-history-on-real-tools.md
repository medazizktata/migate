# migate history on real migrators' history tables: 2026-11-21

Run 10 of migate's end-to-end runs; migate `7e58cf6`, then `ca4eed2` after the fix this run led to. How runs are
judged, and the others: `README.md`.

`migate history` compares what production's history table records with the repository and the ledger (TASK-21). Here
the "production" databases were written by the real tools, read back by real clients, never a real production.

**Four sample apps on PostgreSQL 18, read with `psql` and each migrator's own history query.** Each state was set up
with the real tool: the ledger recorded at base, the database migrated to head (or left at base).

| Case | Expected | Django | Rails | Prisma | Drizzle |
|---|---|---|---|---|---|
| base checkout, production at head | MG010 | ok | ok | ok | ok |
| head checkout, head not recorded | MG011 | ok | ok | ok | ok |
| head recorded, production at head | clean | ok | ok | ok | ok |
| head recorded, production still at base | MG012 | ok | ok | ok | ok |

On `7e58cf6` Django was wrong in three of the four: its history table also records Django's own apps (`admin`,
`auth`, `contenttypes`, `sessions`), whose files ship with Django, not with the project, so every real Django project
got 18 false MG010s. Fixed in `ca4eed2` (only the groups the repository ships are compared); 16 of 16 after.

**The Payload-on-D1 app**, a local D1 database its own migrate wrote, read through Wrangler (`d1 execute --local
--json`, reduced with `jq -r '.[0].results[].name'`): 29 applied migrations; against the ledger recorded at adoption,
clean; with that ledger's last entry removed, MG011 for exactly that migration.

Not run: a real production database (`--remote`); the README's D1 example assumes `--remote` prints the same JSON shape
as `--local`.
