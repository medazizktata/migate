# Blind review of the M0 decisions

Date: 2026-11-19. The owner asked for the M0 design decisions to be checked by an agent with no context: no access to
this repository's research, transcript, README, CLAUDE.md or backlog, only the decisions themselves, the predecessor
code (`docs/prior-art/the Payload-on-D1 app/*.mjs`) and the open web. It was told to ask and research its own questions first, to
assume every decision wrong until checked, and to cite everything. The brief is in the appendix, verbatim.

**Labels.** **V** checked by the main session after the review (see section 3). **R** relayed from the blind agent,
which cites its sources.

## 1. The verdict

The core idea holds: a free, offline release gate that runs the team's own migrations on a throwaway database and checks
what the running versions still use fills a real gap, and GitHub's two August 2025 incidents are exactly its case (R).
But several decisions were wrong or narrower than stated:

| Decision | Verdict | Why |
|---|---|---|
| D1 Product shape | holds | No free tool works across migrators and runs code-written migrations; Atlas lint needs a Pro login (R) |
| D2 Primary check | imprecise, load-bearing | Method holds; the rule list misses breaking *additions* (V: GitLab #5171, severity 1), default and type changes, index drops and data effects; the Rails reference check is wrong (V); "previous version" should be "all live versions" (R: Vercel keeps old deployments 12-24 h) |
| D3 Overrides | holds in design | The guard (no override for a still-referenced column) is what would have stopped GitHub; that a written reason reduces incidents is unsupported (R) |
| D4 Locks | partly wrong | Squawk cannot emit SARIF (V); the timeout rule does not apply to SQLite/D1, must accept role-level settings, and needs retries (R); production counts predict failures but miss later bad writes (R); rehearsal holds and should rank higher (R) |
| D5 Ordering | imprecise | Per migrator is right; silent-skip migrators (Drizzle, golang-migrate) must always block (R, with source lines) |
| D6 Stored record | partly wrong | Append-only holds; "comment-only edits don't block" is wrong for Flyway, which checksums every line (V) |
| D7 History comparison | imprecise | Impossible as a full history for Alembic and golang-migrate, which store only the current version (R; also in `first-pass.md`) |
| D8 Architecture | unsupported | No evidence either way; its objection that ordering is code is already met (ordering is core code chosen by name, a detail the brief left out) |
| D9 Rust and distribution | holds for feasibility | Squawk ships one Rust binary through npm and PyPI exactly this way (R) |
| D10 Licence and money | licence holds; "no revenue" unsupported for sustainability | Atlas moving lint behind Pro suggests multi-engine maintenance is costly (R) |
| D11 Engine order | unsupported for frequency | Public incident evidence is Postgres and MySQL; SQLite first is justified by dogfooding, not by incidents (R) |
| D12 Independence | holds | No counter-evidence (R) |
| D13 Name `migate` | wrong for PyPI | Taken there by an active package (V) |

### How D1-D7 fare against real incidents (R)

| Kind of incident | Outcome |
|---|---|
| Drop or rename still used (GitHub Aug 2025 ×2), new NOT NULL (GitLab #17655) | Stopped, if Rails references are handled correctly |
| Breaking addition, default change, schema cache (GitLab #5171 sev1, #20561, #14469) | **Missed** by a schema diff |
| Type change (CircleCI 2021); its rollback half | Stopped only if every type change is flagged; rollback missed |
| Lock queue (GoCardless; ~33 GitLab incidents) | Postgres: an outage becomes a failed deploy via timeouts; SQLite/D1: not applicable |
| Load from online schema change at scale (GitHub ×5) | Missed; mostly irrelevant to small teams |
| Index or plan regression (GitHub 2021-03) | Missed |
| Data loss (GitLab #6072, GitHub 2023-09, Drizzle #4938) | Missed by the diff; caught only by a rehearsal with data |
| Reverted, skipped or drifted migrations (GitLab #4569, Drizzle, golang-migrate, GitHub 2025-12) | Stopped by D5-D7, with the fixes above |
| Routine NOT NULL after a backfill | False alarm; override each time (rate not measured) |

### Missing (R)

1. **A mixed-version test:** migrate a throwaway database to the new head, then run the live version's tests (or at
   least load its models and run its queries) against it. GitLab's corrective action after #5171 and #14470 was a
   "mixed deployment test environment". It catches breaking removals and breaking additions.
2. All live versions, not one. 3. Rollback compatibility (can the old code read what the new code wrote?). 4. Row counts
   before and after on a seeded database, and a D1/SQLite rule against table rebuilds under `ON DELETE CASCADE`.
5. Index usage at release before dropping an index. 6. A check after any rehearsal that every expected migration ran.
7. Lock retries alongside timeouts. 8. A drift check after migrating. 9. Running across many per-tenant databases.

The agent would make item 1 the primary check, keep the schema diff as a fast first pass and for the PR comment, and make
a rehearsal with data the main defence against data loss.

## 2. Owner's decisions after the review (2026-11-19)

- **The mixed-version test is the primary check.** The schema diff is the fast first pass, the fallback where a team has
  no tests to run, and the source of the PR comment.
- The other corrections are adopted: per-ORM reference rules (Rails: a column is in use unless in `ignored_columns` in
  every live version), all live versions, rollback compatibility, comment-only edits per migrator (block where the
  migrator checksums the whole file), Squawk imported from its JSON or GitLab output, timeouts for Postgres and MySQL
  only with role-level settings accepted and retries, head-only history comparison for Alembic and golang-migrate, and
  the additions in section 1.
- Unchanged: Rust, Apache-2.0, no revenue (sustainability noted as a risk), SQLite then Postgres (both in the first
  release), independence, the name `migate` (pip package `migate-cli`).

## 3. Checked by the main session

- **Squawk has no SARIF output:** `docs/docs/cli.md` lists `--reporter` values `tty, gcc, json, gitlab`; the only file in
  the repository mentioning "sarif" is `squawk-vscode/pnpm-lock.yaml` (GitHub code search).
- **Flyway checksums comments:** `flyway-core/.../ChecksumCalculator.java` reads every line, strips only the BOM from the
  first, and feeds each line to the CRC32, with no comment filter.
- **GitLab #5171:** "increase in 500 errors - each UNION query must have the same number of columns", labels
  `RootCause::Release-Compatibility` and `severity::1`; "A database migration caused a inconsistent version of the
  database schema to be cached in Rails processes. This resulted in certain processes generating SQL queries invalid with
  the new schema." (GitLab API). That the migration *added* a column is relayed.
- **Rails:** strong_migrations' README uses `self.ignored_columns +=` as the documented step before removing a column
  (lines 41 and 118). The GitLab documentation quote about Rails caching the schema at boot is relayed.
- **Drizzle #4938:** open since 2025-09-25, 20 comments, "Migration generator silently causes cascade data loss during
  SQLite table recreation without warning".
- **`migate` on PyPI:** taken by an active Xiaomi authentication gateway (v1.1.11, 2026-09-10); `migate-cli` is free
  (checked before the review).

Not re-checked: the GitHub and GitLab incident counts, the Vercel and Cloudflare deployment details, the Alembic and
golang-migrate source lines, the Atlas "restrictive" analyzer being Pro-only (it conflicts with `gap.md`'s finding that
`migrate lint` is still registered in Atlas's open-source code; both can be true for the official binary versus a source
build).

## 4. What the brief pushed

The agent's own line: the brief "pushed me to hunt for faults decision by decision and to quantify incidents. It
discouraged reading the owner's own research, so I may have re-derived things it already covers. It also nudged me
toward a D1-first frame, which public incident data does not support." The brief's one-line summaries also dropped
details (ordering as built-in code), which produced one finding the architecture already answers.

## Appendix: the brief, verbatim

```
## Ground rules (read first)

- **Assume every decision below is wrong until you have checked it against evidence you found yourself.**
- Read-only everywhere. Do not post, comment, star, open issues or create accounts anywhere.
- Do not read these paths; they would bias you: docs/research/, docs/transcript.md, README.md, CLAUDE.md, backlog/,
  ~/.claude*/. You may read the predecessor code in docs/prior-art/the Payload-on-D1 app/*.mjs (not the ADR).
- Web: WebSearch, WebFetch, curl, gh api (authenticated, read-only). Never use Chrome, Claude in Chrome, Playwright
  or chrome-devtools; lightpanda fetch --dump markdown <url> is allowed for JavaScript-built pages. Stop at login or
  bot walls; never spoof a user agent.
- Do not start subagents. Write files only under the session scratchpad. Return your full report as your final message.

## The goal

The owner wants to build an open-source tool that prevents production incidents caused by database schema migrations,
usable by teams on any database, any migration tool and any hosting. Nothing is built yet.

## The decisions under test

- D1 Product shape. A release gate run in CI and at release time. It works alongside the team's existing migration
  tool (Rails, Django, Prisma, Drizzle, Flyway, Alembic, Payload, Knex, golang-migrate and others) rather than replacing
  it. Works offline, with no account.
- D2 Primary check. Refuse a schema change that breaks the application version still running during a deploy (for
  example dropping or renaming a column, adding NOT NULL without a default, narrowing a type, adding a constraint or
  trigger that rejects the old version's writes). Method: run the team's own migrate command against a throwaway
  database, read the schema before and after, classify the difference. In addition, for a dropped or renamed column,
  check whether the previous version's code (its ORM model definitions) still references it.
- D3 Overrides. A team can mark a breaking change as a deliberate "contract" step only by naming the object and
  giving a written reason; every override is listed in the pull-request comment; an override cannot cover a column the
  previous version's code still references.
- D4 Locks and long-running changes. No new lock linter. Instead: a rule that migrations set a lock or statement
  timeout; read-only count queries against production at release time (for example rows that would violate a new NOT
  NULL); an optional rehearsal on a copy of production; and importing findings from existing linters (for example
  Squawk) in SARIF form.
- D5 Ordering. Whether an out-of-order migration is blocked, warned about or allowed is decided per migration tool;
  two migrations with the same version are flagged.
- D6 Applied-migration record. A hash of each applied migration file (line endings normalised), kept in the repo
  and append-only against the base branch; a comment-only edit is reported separately rather than blocking.
- D7 History comparison. At release time, compare production's own migration-history table with the repository,
  using read-only credentials; production credentials never reach pull-request CI.
- D8 Architecture. A small core; database engines, hosts and migration tools as separate plug-in kinds; migration
  tools described by declarative data files; third-party plug-ins as subprocesses speaking JSON.
- D9 Language and distribution. Rust; one binary shipped through npm per-platform packages, pip wheels, Homebrew and
  a GitHub Action.
- D10 Licence and money. Apache-2.0; no revenue intended.
- D11 Build order of database support. SQLite (including Cloudflare D1) first, then PostgreSQL, then MySQL/MariaDB,
  then SQL Server; MongoDB gets only D6 and D7.
- D12 Independence. No vendor is a dependency; vendor-specific features only as optional adapters.
- D13 Name. migate.

## What to do

0. Ask your own questions first. Before judging any decision, write down the questions you would want answered to
   judge this goal and these decisions well: about the problem, the people who would use the tool, existing
   alternatives, technical feasibility, anything. Do not limit yourself to D1-D13. Answer each one through research
   and reasoning, and keep asking follow-up questions where an answer opens one. Questions you cannot answer stay in
   the report as open, with what you tried.
1. For each decision D1-D13, determine whether it holds against evidence you find: public incident reports and
   postmortems, existing tools and their documentation and source code, migration-tool documentation and issue
   trackers, practitioner discussions, package registries.
2. Evaluate the method as a whole: would this set of decisions actually prevent the incidents that happen in practice?
   Establish what kinds of migration-caused incidents occur, and how often each kind would be stopped, missed or
   falsely flagged by D1-D7.
3. Establish what is missing that a tool with this goal should have, and what in D1-D13 is unnecessary.
4. If you think the approach itself is wrong, say what you would do instead.

## The report

- First, your own questions: each question, its answer with citations (or "open" and what you tried), in the order
  you asked them.
- Then one section per decision D1-D13, then the method section, then missing and unnecessary items.
- For each decision: holds, wrong (contradicted by a source), unsupported (no source found either way) or
  imprecise (a source exists but says something weaker), with citations (URL, file and line, command output).
- Say clearly which decisions you verified as correct. I need to know which parts are load-bearing and which are
  shaky, not just a list of faults.
- State what you checked exhaustively versus sampled, and treat anything unstated as unchecked.
- For each judgement, say what evidence would change your mind.
- End with one line: what this brief pushed you to look at, and what it discouraged.

## Ground rules again

Assume each decision is wrong until checked. Read-only. Do not read the paths listed at the top. No subagents. Every
judgement needs a citation.
```
