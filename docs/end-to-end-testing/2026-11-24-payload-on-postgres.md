# Payload on PostgreSQL: 2026-11-24

Run 16 of migate's end-to-end runs. How runs are judged, and the others: `README.md`.

## What was run

The one bundled migrator not yet run on PostgreSQL. A minimal Payload app (Payload 3.94.1 with
`@payloadcms/db-postgres`, one `posts` collection with a required `title` and an optional `subtitle`), its
migrations written by Payload's own `migrate:create`. The old version's test is a script run with `payload run`: it
creates a post through Payload's local API and finds it by `subtitle`, as the app would. Three changes, each a branch
off `main`, labelled before migate ran. migate `b0ebe0c` (release build, md5 `2e37179f0c77`), on the self-hosted Linux
box, PostgreSQL 15, Node 24.

```toml
migrator = "payload"
base = "main"
[sandbox]
engine = "postgres"
target_version = "15"
env = { DATABASE_URI = "{url}" }
setup = "npm ci --prefer-offline --no-audit --no-fund"
test = "npx --no payload run test.ts"
```

The migrate command is the migrator's default, `npx --no payload migrate`. Before the cases, the test was run by hand
on a database migrated to `main`, and passed: a failure in a case is the change's, not the harness's.

## Results

| Case | Payload's SQL | Label | migate | Why, from migate's output | Time |
|---|---|---|---|---|---|
| drop | `DROP COLUMN "subtitle"` | break | MG201, MG212 (`collections/posts.ts:7`), MG810 | the old insert names `subtitle`: `column "subtitle" of relation "posts" does not exist` | 81 s |
| add | `ADD COLUMN "summary" varchar` | safe | pass (MG210 note) | | 83 s |
| required | `ADD COLUMN "author" varchar NOT NULL` | deploy fails | MG803 | on the rows the old tests left: `column "author" of relation "posts" contains null values` | 65 s |

Every label held: two caught, one correct pass. MG212 read the old version's Payload field from its collection file,
so the drop is named as still used even before the tests run.

## Limits

- A sample app, one collection, three changes: it shows the wiring for Payload on PostgreSQL, not real-world history.
- Making the run go took a `tsconfig.json` (Payload's CLI reads it) and a free port for the hand-run check (another
  project's server held the first one tried). Neither is migate's.
