# migration-guard (working name)

A free, offline release gate for database migrations that works with **any migrator, any database and any host**. It
stops a migration from changing after production ran it, from running out of order, from disagreeing with production's
own history, or from breaking the app version still running during a deploy, and it ships everything in one ordered
release.

Status: research done (two rounds), nothing built yet. Start here:

- `docs/research/README.md`: the summary: the gap, what makes this a better wheel, the architecture, the stack, what is
  not verified, and the decisions open.
- `docs/research/validation.md`: is the problem real and weighty (41 classified incidents, kill criteria, the case
  against).
- `docs/research/gap.md`, `architecture.md`, `stack.md`: the full reports; `docs/research/bench/`: the benchmark sources
  and validation data.
- `docs/research/first-pass.md`: the first round (landscape, 21 migrators' history tables), with corrections at the top.
- `docs/transcript.md`: the conversation this project came from, with the first round's research reports in full.
- `docs/prior-art/the Payload-on-D1 app/`: the single-repo version it grew out of (the Payload-on-D1 app, Payload on Cloudflare D1), with its ADR.

## Direction (the owner's, 2026-11-19)
- The goal is any database and any host, reached through the architecture: the ledger and production-history layers
  work for every engine from the first release; breaking-change detection is a plug-in per engine (a throwaway database
  plus a schema reader), and reaching production is a plug-in per host.
- D1 is one host of the SQLite engine, the way RDS is one host of PostgreSQL. It may be among the first plug-ins; it is
  not the product's identity. PostgreSQL comes early, to prove the plug-in interface generalises.
- Independent and pluggable into any system; no vendor is a dependency of the core. The general solution comes first;
  a GFS-dedicated feature only once it works for most databases.

## Open decisions
- Language: Rust recommended and the owner leans to it; not yet formally confirmed (`docs/research/stack.md`).
- The design changes proposed by the validation (`docs/research/validation.md` §3).
- Name (`migration-guard` is a placeholder).
- Licence: Apache-2.0 suggested.
- Engine plug-in order (a suggestion is in `docs/research/README.md` §6).
