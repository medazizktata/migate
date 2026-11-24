# documenso with its own test suite: 2026-11-24

Run 15 of migate's end-to-end runs. How runs are judged, and the others: `README.md`.

## What was run

migate's primary check on outside code for the first time: the base version's own tests against a database migrated
to the head. The project is documenso (Prisma on PostgreSQL), at four commits of its main line that add migrations,
each against its parent as the base. Two were labelled unsafe and two safe before migate ran, from the migration and
the code alone.

On a self-hosted Linux box, as an unprivileged user, without Docker. The `[sandbox]` drives documenso's own commands:

```toml
migrator = "prisma"
[sandbox]
engine = "postgres"
target_version = "15"
env = { NEXT_PRIVATE_DATABASE_URL = "{url}", NEXT_PRIVATE_DIRECT_DATABASE_URL = "{url}" }
setup = "bash <harness>/setup.sh"            # npm ci --prefer-offline
migrate = "npx --no prisma migrate deploy --schema packages/prisma/schema.prisma"
test = "bash <harness>/test.sh"
keep_output = "<harness>/kept/<case>"
```

`test.sh` writes documenso's `.env` from its `.env.example` with the sandbox's database URL, builds the Remix app once
per checkout, starts it on the sandbox database with `NODE_ENV=test`, and runs documenso's Playwright `api` project:
the spec files of an allowlist (39 files) that pass at documenso's HEAD on that box. Left out: the browser specs (no
browser installed) and specs needing outside services. documenso's own `DANGEROUS_BYPASS_RATE_LIMITS=true` stops its
rate limits from failing a parallel run, and a small SMTP sink on port 2500 accepts the mail older versions send
during tests. Node 24, PostgreSQL 15, 22 cores. The harness scripts are kept outside this repository.

## Results

| Case | Base, head | Migration | Label | Control (base tests, base database) | Treat (base tests, head database) | migate | Time |
|---|---|---|---|---|---|---|---|
| 1 | `9b59f1a2`, `d5ce2224` | `Envelope.signatureLevel` added NOT NULL, default dropped right after | unsafe | 165 passed | 144 failed, 23 passed, 2 flaky | MG810, MG203; MG004 warning | 1,313 s |
| 2 | `da6670eb`, `fdd0337b` | four NOT NULL counters on the claim tables, backfilled, no default | unsafe | 283 passed, 3 flaky | 279 failed, 7 passed | MG810, 4 MG203; MG004 warning | 1,795 s |
| 3 | `ebf5b75a`, `4ee789ea` | a new table, an enum, two nullable foreign keys | safe | 167 passed | 167 passed | clean | 1,115 s |
| 4 | `581ffb97`, `5a383144` | emails lowercased in place | safe | 336 passed, 1 flaky | 337 passed | clean | 1,531 s |

Every label held: two caught, two correct passes, no false alarm, no missed break.

- **Case 2, why the old tests fail.** All 279 failures are the same error, read from the kept output: the old Prisma
  client creates an organisation claim without the new column, and PostgreSQL answers `Null constraint violation on
  the fields: (envelopeContentCount)`, then a `findFirstOrThrow` finds no organisation (558 of each for 279 failed
  tests; no other Prisma error appears). This is the break a rolling deploy would see: the old version cannot sign up a
  user while the new one deploys. documenso shipped it; no later migration adds a default (checked at its HEAD,
  `e745c9ec`).
- **Case 1.** The same shape: the old client creates an envelope without `signatureLevel`, and every failure is
  `Null constraint violation on the fields: (signatureLevel)` from `prisma.envelope.create()` (no other Prisma error in
  the output). The migration drops the column's default on purpose, so that new code must choose a level; the old
  code cannot. Still so at documenso's HEAD. The first pass of this case, without kept output, gave the same verdict
  (1,240 s).
- **MG203 next to MG810.** The schema rule names the same columns the tests broke on, so the report says both what
  broke and why. MG203 alone would need a `migate: contract` marker; MG810 accepts none.
- **MG004, a warning in both unsafe cases.** Each head's migration is dated before migrations already in the base
  (they were written on a branch months earlier). `prisma migrate deploy` runs it anyway, so a warning, not an error,
  is right.
- **Case 4's flaky test** passed on its retry in the control and needed none in the treat. Playwright's retry decides;
  migate reads the exit code.

## What it changed

- No migate fix came out of the run. The first pass of case 1 ran without `keep_output`, and its MG810 showed only the
  tail of the output, too little to tell a real break from a harness fault; the harness now sets it, and case 1 was
  rerun. Each command's output is kept per step (`01-base-setup.log` to `06-head-test.log`; `head-test` is the base
  version's tests on the head database).
- Harness faults met and fixed along the way, none of them migate's: a rate limit
  (bypassed with documenso's own switch), older versions sending mail in tests (the sink), the app's port still held
  by the previous run (a wait), and the build rewriting tracked translation files (restored before each case).

## Limits

- Four cases, one project, one migrator. The allowlist is HEAD's passing specs, so a spec that an older version
  passes but HEAD fails is not run.
- Each case takes 20 to 30 minutes, nearly all of it two `npm ci`, two app builds and two test runs.
