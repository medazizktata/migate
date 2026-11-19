# migration-guard (working name)

A release guard for database migrations that works with **any migrator, any database and any host**: it stops a
migration from changing after production ran it, from running out of order, from disagreeing with production's own
history, or from breaking the app version still running during a deploy, and it ships everything in one ordered release.

Status: research done, nothing built yet. Start here:

- `docs/research.md`: what exists, the gaps, the three-layer design, a first version, the risks.
- `docs/transcript.md`: the conversation this project came from, with both research reports in full.
- `docs/prior-art/the Payload-on-D1 app/`: the single-repo version it grew out of (the Payload-on-D1 app, Payload on Cloudflare D1), with its ADR.

## Direction (the owner's, 2026-11-19)
- The goal is any database. It is reached through the architecture: the ledger and production-history layers work for every
  engine from the first release; breaking-change detection is a plug-in per engine (a throwaway database plus a schema
  reader).
- Start with Cloudflare D1 (and SQLite); PostgreSQL second, early, to prove the plug-in interface generalises.

## Open decisions
- Name (`migration-guard` is a placeholder).
- Language: one Go binary over Atlas's open-source inspectors and analyzers, with npm and pip wrappers (recommended in the
  research), or TypeScript (Miniflare and PGlite in-process, but schema readers to write).
- Licence: Apache-2.0 suggested (compatible with the Atlas code it would reuse).
