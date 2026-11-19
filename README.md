# migate

A free, offline release gate for database migrations that works with **any migrator, any database and any host**. Its
primary check runs the version of your app that is still live against the database after your new migrations, so a
migration that would break the running app is stopped before it ships. It also stops a migration from changing after
production ran it, from running out of order or being silently skipped, and from disagreeing with production's own
history.

Status: research and decisions done, nothing built yet. The repository folder is still named `migration-guard`.
Licence: Apache-2.0 (`LICENSE`). Start here:

- `docs/research/README.md`: the summary: the gap, the validation, the architecture, the stack, what is not verified,
  and the decisions taken.
- `docs/research/validation.md`: is the problem real and weighty (41 classified incidents, kill criteria, the case
  against).
- `docs/research/blind-review.md`: a context-free review of the decisions, and the decisions taken after it.
- `docs/research/gap.md`, `architecture.md`, `stack.md`: the full reports; `docs/research/bench/`: the benchmark sources
  and validation data.
- `docs/research/first-pass.md`: the first round (landscape, 21 migrators' history tables), with corrections at the top.
- `docs/transcript.md`: the conversation this project came from, with the first round's research reports in full.
- `docs/prior-art/the Payload-on-D1 app/`: the single-repo version it grew out of (the Payload-on-D1 app, Payload on Cloudflare D1), with its ADR.

## Direction (the owner's, 2026-11-19)
- The goal is any database and any host, reached through the architecture: the ledger and production-history layers
  work for every engine from the first release; breaking-change detection is a plug-in per engine (a throwaway database
  plus a schema reader), and reaching production is a plug-in per host.
- D1 is one host of the SQLite engine, the way RDS is one host of PostgreSQL. SQLite is the first engine to build and
  dogfood; PostgreSQL ships in the same first release and leads the public launch.
- Independent and pluggable into any system; no vendor is a dependency of the core. The general solution comes first;
  a GFS-dedicated feature only once it works for most databases.
- Rust; Apache-2.0; pure open source, no revenue.

## Open
- Renaming the repository folder to `migate`.
- `docs/research/architecture.md` still describes the design before the validation and the blind review; updating it is
  the next task.
