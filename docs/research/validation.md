# Validation: is the problem real, and does it have weight?

Date: 2026-11-19. The owner asked for proof that this tool answers a real, costly problem, not just one more repository.

**How this was made.** Kill criteria were written down *before* any search (below). Two research agents then ran in
parallel with opposite jobs: an **incident census** (find and classify every public incident caused by a schema
migration) and a **devil's advocate** (build the strongest case against building the tool). The main session re-checked
the evidence the verdict rests on; see "Checked by the main session". Data and scripts are in `bench/validation/`.

**Labels.** **V** primary source read by the agent or the main session. **R** relayed. **U** unknown.

## Verdict

**The problem is real and has weight, but the weight sits in a narrower place than the five protections assumed.**

1. **Breaking the app version still running (P4) is the class with repeated, costly evidence at large companies:**
   Snowflake (13 h, 10 of 23 regions), GitHub twice in August 2025, Deno, Buildkite, Val Town, GitLab.
2. **In those incidents the engineers usually *meant* to drop the column.** What broke was that the running code still
   referenced it. A schema diff plus a "contract, reason: no longer used" marker would have let GitHub's and
   Buildkite's migrations through. **The check that matters is "is the old app still using this?", not "is this a
   drop?"**
3. **Most migration-caused incidents are locks, timeouts, load and infrastructure moves**, which an empty sandbox and a
   schema diff cannot see (7 of 10 at GitHub; at least 37 of 101 at GitLab by title).
4. **Edited, removed and misordered migrations (P1, P2) cause frequent friction and rare outages.** One company
   postmortem (Tuist 2026). A checksum check can also fail the other way: blocking every deploy for days after a
   comment-only edit.
5. **Production drift (P3) has one weak public incident**; restore points (P5) shorten recovery rather than prevent.
6. **Demand is not falling, supply is exploding.** Free linters hold or grow their share; 126 new "migration linter"
   repositories were created in 2026. Distribution, not capability, is the bottleneck.
7. **A new signal (small sample):** AI agents writing migrations. Buildkite's override was added by an LLM, and most
   2026 edited-migration incidents are in apparently agent-driven projects.

## Kill criteria (fixed before searching)

| | Criterion | Result |
|---|---|---|
| K1 | Fewer than ~10 credible public incidents in 10 years caused by schema migrations | **Not met.** 41 curated incidents 2015-2026; GitLab's public tracker labels 101 `RootCause::DB-Migration` incidents (2021-02 to 2026-09). |
| K2 | Most incidents are a class none of P1-P5 would prevent | **Met in samples the agent did not pick** (GitHub reports 7 of 10; GitLab ≥37 of 101 by title; Dan Luu's list 4 of 5). Not met in the curated list (20 of 41), which was searched with P4 terms and is biased. |
| K3 | Protections treated as solved by process AND free-tool adoption flat or falling | **Not met.** Process is the main defence, but teams still want checks (below), and adoption grows. |
| K4 | Most teams don't run old and new app versions against one database during deploys | **Not met** on platform defaults; the share of stop-the-world teams and the size of the harm are U. |
| K5 | The pain sits in one ecosystem already well served | **Not met, but it narrows the target.** Rails and Django are served; TypeScript/edge migrators, SQLite, MySQL, SQL Server and P3 for most migrators are not. |

## 1. Incident census

### 1.1 Curated incidents (n = 41)

Classes: **NONE** none of P1-P5 would have prevented it. **P4\*** P4 as specified would only have asked for an
acknowledgement that was (or would have been) given, or an equivalent guard existed and was bypassed. **P4?** P4 class,
but the schema change is not public. n/s: not stated.

| # | Date | Who | Engine / migrator | What happened | Impact | Class | Label | Source |
|---|---|---|---|---|---|---|---|---|
| 1 | 2015-10-08 | Stripe | n/s | Index change filed as "add new" and "remove old"; the removal was run first, by hand | ~90 min; ~2/3 of API calls failed | P5-partial | V | [archive](https://web.archive.org/web/2016/https://support.stripe.com/questions/outage-postmortem-2015-10-08-utc) |
| 2 | ≤2016 | GoCardless | Postgres | Fast migration queued behind a long read and blocked the table | 15 s | NONE (lock) | R | [blog](https://gocardless.com/blog/zero-downtime-postgres-migrations-the-hard-parts/) |
| 3 | 2017-12-06 | Microsoft VSTS | Azure SQL | DB upgrade caused high CPU | availability issues | NONE (load) | R | [devblogs](https://devblogs.microsoft.com/devopsservice/?p=15725) |
| 4 | ≤2018 | anonymous (gist) | Postgres / Rails | Migration removed `topic_id` in the same deploy; old code broke; irreversible | 14 min | P4 | V | [gist](https://gist.github.com/dwbutler/2910806f9204b0fffa77fb27356579b8) |
| 5 | 2019 | company n/s | Rails | Dropped a column while old code served | n/s | P4 | R | [blog](https://smsohan.github.io/blog/2019/03/18/ruby-on-rails-database-migration-and-downtime) |
| 6 | 2021-02-09 | OpenAI | n/s | "routine database migration caused an unexpected database lock" | ~10 min API outage | NONE | V | [status](https://status.openai.com/incidents/j6rhhcxtsrf4) |
| 7 | 2021-03-12 | GitHub | MySQL / Rails | Migration flipped an index order | 1 h 10 min degraded | NONE (plan) | V | [report](https://github.blog/news-insights/company-news/github-availability-report-march-2021/) |
| 8 | 2021-04 | GitHub | MySQL | Data move of secret-scanning tables to their own cluster broke writes | hours | NONE (infra) | V | [report](https://github.blog/news-insights/company-news/github-availability-report-april-2021/) |
| 9 | 2021-05-11 | GitLab.com #4569 | Postgres / Rails | Migration applied in production, then reverted in master; table stayed | none stated | P1 | V | [issue](https://gitlab.com/gitlab-com/gl-infra/production/-/issues/4569) |
| 10 | 2021-07-27 | Deno | n/s | Dropped a column and added its replacement during the rollout; old instances crashed | 6 min | P4 | V | [post](https://deno.com/blog/2021-07-27-outage-post-mortem) |
| 11 | 2021-07-28 | GitLab.com #5241 | Postgres / Rails | Schema mismatch after canary migrations; 500s until restart | 5 h 30 min | P4? | V | [issue](https://gitlab.com/gitlab-com/gl-infra/production/-/issues/5241) |
| 12 | 2021-11-08 | CircleCI | Postgres | Field type changed; old rows broke the distributor; rollback made new rows unreadable | ~53 min blocked | P4\* | V | [report](https://discuss.circleci.com/t/incident-report-november-8-2021-jobs-stuck-in-a-not-running-state/41890) |
| 13 | 2021-11-27 | GitHub | MySQL | Final rename step deadlocked read replicas | 2 h 50 min | NONE | V | [report](https://github.blog/news-insights/company-news/github-availability-report-november-2021/) |
| 14 | 2021-12-15 | GitLab.com #6072 | Postgres | Buggy data migration deleted epic-issue links | data loss, mostly restored | NONE (data) | V | [issue](https://gitlab.com/gitlab-com/gl-infra/production/-/issues/6072) |
| 15 | 2023-05-19 | GitLab.com #14469 | Postgres / Rails | Inconsistent schema cached in Rails after a migration | 42 min; ~12,698 customers | P4? | V | [issue](https://gitlab.com/gitlab-com/gl-infra/production/-/issues/14469) |
| 16 | 2023-09-19 | GitHub | n/s | Projects datastore migration; misconfigured index constraint | 95% of Projects down 3.5 h | NONE | V | [report](https://github.blog/news-insights/company-news/github-availability-report-september-2023/) |
| 17 | 2024-01-24 | Linear | Postgres | Migration ran `TRUNCATE … CASCADE` | 1 h down; 12% of workspaces lost data until restore | NONE (P5 helps recovery) | V | [post](https://linear.app/now/linear-incident-on-jan-24th-2024) |
| 18 | 2024-02-21 | Resend | n/s | Local migration pointed at production dropped all tables | 12 h | NONE (bypassed any pipeline) | V | [post](https://resend.com/blog/incident-report-for-february-21-2024) |
| 19 | 2024-02-26 | GitLab.com #17655 | Postgres / Rails | Post-deploy migration added NOT NULL that broke writes | 500s for Agent users | P4 | V | [issue](https://gitlab.com/gitlab-com/gl-infra/production/-/issues/17655) |
| 20 | 2024-10-11 | GitHub | n/s | "database migration" broke DNS at one site | 19 h; code search down ~4 h | NONE (infra) | V | [report](https://github.blog/news-insights/company-news/github-availability-report-october-2024/) |
| 21 | 2024-11-25 | Fly.io | SQLite (cr-sqlite) | Column added to a large CRDT table; backfill flood | fleet-wide | NONE (volume) | R | [infra-log](https://fly.io/infra-log/2024-11-30/) |
| 22 | 2025-01-06 | GitLab.com #19079 | Postgres | "database issue post-migration" | 16 min of 500s | undetermined | V | [issue](https://gitlab.com/gitlab-com/gl-infra/production/-/issues/19079) |
| 23 | 2025-04 | incident.io | Postgres 17 | Index migration plus PGAudit hung holding locks | ~2 min | NONE | R | [write-up](https://status.incident.io/incidents/01JRDFKAGE07YYDY0KZR137BX3/write-up) |
| 24 | 2025-04-02 | Val Town | n/s / Render | Migrations before code; old code crashed; rollback refused | ~12 min | P4 (+P5) | V | [post](https://blog.val.town/blog/post-mortem-db-migration/) |
| 25 | 2025-04-23 | GitHub | MySQL | Schema change plus load saturated connections | 20 min, 2-5% errors | NONE | V | [report](https://github.blog/news-insights/company-news/github-availability-report-april-2025/) |
| 26 | 2025-08-05 | GitHub | MySQL / Rails | Dropped a column "no longer in direct use"; the ORM still referenced it | 32 min; ~4% of web/API | P4\* | V | [report](https://github.blog/news-insights/company-news/github-availability-report-august-2025/) |
| 27 | 2025-08-27 | GitHub | MySQL / Rails | Same pattern on a Copilot table; GitHub then blocked all column drops | ~42 min; up to 77% Copilot failures | P4\* | V | same report |
| 28 | 2025-11-18 | Cloudflare | ClickHouse | Permissions change made a metadata query return duplicates | global outage | NONE | R | [blog](https://blog.cloudflare.com/18-november-2025-outage/) |
| 29 | 2025-12-08 | Railway | Postgres | Schema change with a data migration held locks | platform outage | NONE | R | [report](https://blog.railway.com/p/incident-report-december-8-2025) |
| 30 | 2025-12-16 | Snowflake | metadata store | "backwards-incompatible database schema update"; old packages "errantly referenced the updated fields" | 13 h; 10 of 23 regions | P4 | V | [status](https://status.snowflake.com/incidents/xdxj4klf7sc5) |
| 31 | 2025-12-18 | GitHub | n/s | "database migration causing a schema drift" | ~1.5 h | P3 (uncertain) | V | [report](https://github.blog/news-insights/company-news/github-availability-report-december-2025/) |
| 32 | n/s | Sentry | Postgres / Django | `RunSQL` migrations had no lock or statement timeouts | 22 min US outage | NONE | R | [status](https://status.sentry.io/incidents/1b4n0vq8gl7q) |
| 33 | n/s | Autumn | MySQL / Vitess | Collation migration caused a locking conflict | ~10% of API failed | NONE | R | [post](https://useautumn.com/blog/post-mortem-database-outage-caused-by-collation-migration-locking-conflict.md) |
| 34 | 2026-02-23 | Infisical | n/s | "recent DB migration … caused lock contention" | ~20 min EU partial | NONE | V | [status](https://status.infisical.com/incidents/01KQZD715M59TN8WV3W9VBJXDR) |
| 35 | 2026-05 | GitHub | MySQL | Online schema migration on a large table saturated connections | multi-service | NONE | V | [report](https://github.blog/news-insights/company-news/github-availability-report-may-2026/) |
| 36 | 2026-05-01 | Buildkite | Rails + strong_migrations | `rename_column`; an LLM-assisted workflow added `safety_assured`; migration ran before code | 19 min; most customers | P4\* (bypassed guard) | V | [status](https://buildkitestatus.com/incidents/8nd3cm8kqlnn) |
| 37 | 2026-08-19 | Tuist | ClickHouse / Ecto | Two PRs used the same migration version; the second was treated as applied | 21 min | P1/P2 | V | [postmortem](https://hive.tuist.dev/postmortems/2) |
| 38 | 2026-08/09 | runtara | sqlx | Comment edit to an applied migration; checksum check blocked all later migrations | ~8 days with no migrations | P1 | V | [PR #220](https://github.com/runtarahq/runtara/pull/220) |
| 39 | 2026-09-03 | margince | n/s | Applied migration edited; deployed databases diverged from fresh ones | unclear | P1 | V | [issue #3875](https://github.com/margince/margince/issues/3875) |
| 40 | 2026-09-30 | IH35-TMS | db-migrate / Render | Comments added to applied migrations; checksum refusal blocked deploys | deploys blocked | P1 | V | [PR #23447](https://github.com/tioperfumes07/IH35-TMS/pull/23447) |
| 41 | 2026-10-11 | pragmatic-papers | Payload | `payload migrate` dropped columns, then the build failed, leaving old code on the new schema | 55 min of 500s | P4 + P5 | V | [issue #1288](https://github.com/digitalgroundgame/pragmatic-papers/issues/1288) |

Excluded: incidents not caused by running a migration (integer overflows at Heroku 2023, GitHub 2021, Basecamp 2018;
Clerk 2025 hand-run update; Axiom, LootLocker, Reddit infrastructure moves), before 2014 (GitHub 2012), too little detail
(PostHog EU 2026, Render 2026), and unverifiable first-person blog stories (U).

| Class | Count |
|---|---|
| P4, prevented as specified | 7 (Deno, Val Town, Snowflake, GitLab #17655, gist, Sohan, pragmatic-papers) |
| P4\*, acknowledgement only or bypassed | 4 (GitHub ×2, Buildkite, CircleCI) |
| P4?, change not public | 2 |
| P1 / P2 | 5 (Tuist; three small projects; GitLab #4569) |
| P3 | 1 (uncertain) |
| P5 partial | 1 |
| NONE | 20: locks, load and volume 12; infrastructure moves 4; destructive data operations or ad-hoc runs 3; permissions 1 |
| Undetermined | 1 |

### 1.2 Samples the agent did not pick (the fairer denominator)

- **GitHub availability reports** (78 reports, 2020-07 to 2026-10, via RSS): 10 migration-caused incidents; 7 NONE, 2
  P4\* (August 2025), 1 P3 (uncertain).
- **GitLab `RootCause::DB-Migration`** (n = 101): 37 lock, timeout, deadlock, vacuum or replication by title (NONE); 45
  "post-deploy migration failed" (mostly blocked deploys, no user impact; descriptions mention "lock" in 20 and a
  constraint violation by existing data in 10); of 19 others, 13 read: 1 P4, 2 P4?, 1 P1, 6 NONE, 3 undetermined.
- **Dan Luu's post-mortems list:** 5 migration-caused incidents in the window; 4 NONE, 1 tool class.
- **snakescott/awesome-tech-postmortems:** 0 hits.

### 1.3 Issue trackers and Stack Overflow (low precision)

GitHub issue search, open + closed, `repo:<r> is:issue migration (<phrases>)` (exact queries in
`bench/validation/data/github_issue_counts.tsv`):

| Repo | (a) edited applied | (b) out of order | (c) drift | (d) broke production |
|---|---|---|---|---|
| prisma/orm | 40 | 5 | 295 | 320 |
| drizzle-orm | 3 | 17 | 41 | 56 |
| rails | 6 | 4 | 13 | 22 |
| alembic | 0 | 14 | 18 | 60 |
| flyway | 30 | 101 | 19 | 18 |
| liquibase | 5 | 0 | 4 | 9 |
| payload | 1 | 0 | 5 | 19 |
| typeorm | 6 | 3 | 15 | 83 |
| knex | 0 | 8 | 5 | 30 |
| golang-migrate | 2 | 6 | 8 | 4 |
| **Total** | **93** | **158** | **423** | **621** |

A spot-check of the top issues by reactions judged only 35% (a), 36% (b), 15% (c) and 23% (d) relevant, so these totals
overstate the real numbers roughly 3-7x. Notable: typeorm#3357 (data loss from generated drops, 121 reactions),
golang-migrate#179 (91), drizzle#2488 (52), drizzle#5316 and #5769 (drizzle-kit silently skips migrations older than the
newest applied one and exits 0).

Stack Overflow counts are small (top-voted: Flyway "Validate failed. Migration Checksum mismatch" 121 votes; Liquibase
checksum error 78; alembic "multiple heads" 77; EF "update a production database with EF migrations?" 78). The signal
is friction at modest volume, not outages.

### 1.4 Surveys

All are vendor-run with self-selected samples, and **none gives a measured rate of migration-caused incidents** (U):
Redgate State of Database DevOps 2021 (>3,200), State of the Database Landscape 2024 (3,849: "minimize application
downtime (34%)" as a reason to automate); Liquibase State of Database Change Governance 2026 (426: "Over a third report
unplanned downtime", not tied to database changes); DORA 2021 via vendors (R); an Atoms/CloudKitchens review claiming
"roughly 80%" of database outages came from schema management (no count, promotes its own tool).

## 2. The devil's advocate

### 2.1 Is the N-1 premise real? Yes, by platform defaults

Kubernetes: "`RollingUpdate` is the default value" (V). Fly.io: `rolling` is the default and `release_command` runs
before Machines update (V; old Machines serving during the migration is an inference). Heroku release phase: the old
release keeps serving until new dynos boot (V; inference). Heroku preboot overlaps versions up to 3 minutes, off by
default (V). Rails 8's Dockerfile entrypoint runs `db:prepare` at server start, and Kamal deploys with zero downtime
(V). Cloudflare Workers gradual deployments: "related requests can use incompatible versions of your Worker" (V). Vercel
Skew Protection keeps old clients on old deployments for 12-24 h by default on supported plans (V). No survey measures
how many teams deploy with downtime (U).

### 2.2 Is it solved by process? Process is the main defence, not a sufficient one

The 2019 Ask HN on migrations (269 points) answers N-1 with process ("Migrations have to be written so that currently
deployed code will work after the migration"). The 2026 HN thread "Is your Postgres migration safe or not safe?" (138
points) shows teams still want checks: the author, who led Cloudflare's Postgres platform for 170+ product teams, wrote
"One of the constant asks is schema migration review. We published a lot of best practices, added CI checks however, it
was still hard to catch." Practitioners there distrust rule-only checks: "some migration safety depends on the state of
the database, which isn't represented in the DDL statement alone" (orf); "I only feel safe when I can run them against a
replica first" (hakanil); "migration linters like this one don't give enough peace of mind" (fabianlindfors).

### 2.3 Demand: flat-to-growing, supply exploding

| Signal | Numbers |
|---|---|
| strong_migrations (RubyGems, Jan 1-Nov 18) | 10.7M (2023), 14.0M (2024), 14.4M (2025), 20.5M (2026): 12-17% of activerecord |
| django-migration-linter | steady 16-25 per 1,000 Django downloads (2025-01 to 2026-07) |
| squawk-cli (PyPI) | 9.3k (2025-01) → 1.95M (2026-11), +37% per million Django downloads after inflation |
| New GitHub repos "migration linter" per year | 3, 8, 5, 22, **126** (2022-2026) |
| New GitHub repos "schema drift" per year | 4, 4, 2, 96, **1,111** |

2026 download counts are inflated across registries for unknown reasons; use ratios. GitHub star history could not be
measured: the stargazers API no longer returns data, and GH Archive star events collapsed from mid-2025.

### 2.4 Do people pay? Yes, for platforms, not CLIs

Ariga (Atlas) ~$18M raised (R); Atlas Pro from $9/developer/month (V). Bytebase $3M seed (R), Pro $20/user/month (V).
Redgate ~$100M ARR company-wide, bought Flyway ~$10M (R); drift and policy checks are Enterprise (V). Liquibase ~$27M
raised (R). The money buys approval workflows, dashboards and policy, not a free offline CLI. The owner's revenue goal
is not stated.

### 2.5 Why free tools stay small or die

migra was deprecated in 2022 and its downloads kept rising (242k in 2022 → 918k in 2025; a community fork appeared in
2026). LendingHome/zero_downtime_migrations is archived. Reshape's author now monetises through a platform. Abandonment
tracks solo-maintainer economics, not falling demand: a direct risk for a wide adapter surface.

### 2.6 Cross-migrator, or each framework's own tool?

Framework-native linters dominate their ecosystems (strong_migrations, django-migration-linter); migrator-neutral but
engine-specific tools are adopted (Squawk); on HN, tools that *perform* safe migrations draw far more attention than
linters (pgroll 328 points, Reshape 177, against single digits for linters). Drizzle users' top requests are triggers
(568 reactions), MSSQL (357) and migration rollback (201), not lint.

### 2.7 Platform risk

Drizzle v1 (beta.16, 2026-03-05): "The new structure also intentionally allows out-of-order migrations (as it should for
team workflows)" (V). Prisma warns on edited migrations but applies pending ones (R). No announced feature makes the
whole proposal redundant; the real risk is incremental, as each migrator adds its own checksums and history checks.

## 3. What this changes in the design (proposed, not yet applied)

1. **Lead with "stops the migration that breaks the running app".** The stored record and order checks are table
   stakes, not the headline.
2. **Make the breaking-change check code-aware.** Before allowing a drop or rename, check whether the base branch's ORM
   model still declares the column (Prisma schema, Drizzle schema, Rails and Django models, ignored-column lists), and
   optionally whether production queried it recently where the engine exposes that. This catches the GitHub and
   Buildkite pattern that a contract marker lets through.
3. **Contract markers that resist agents:** a reason is required; every marker is listed in the PR comment for a human;
   a marker on a column the old code still references stays an error.
4. **Locks without becoming another linter:** a rule that a migration sets a lock or statement timeout (Postgres
   `lock_timeout`/`statement_timeout`, MySQL `lock_wait_timeout`); read-only production size checks; the optional
   rehearsal on a production copy; run Squawk and similar tools and import their SARIF.
5. **P2 per migrator, not a hard rule:** a block for migrators that silently skip older migrations, a warning or nothing
   where out-of-order is the migrator's design (Drizzle v1). Add a duplicate-version check (the Tuist case).
6. **P1 that does not block deploys for nothing:** report a comment-only or whitespace-only edit separately from a real
   edit (runtara, IH35-TMS).
7. **Distribution and maintenance are the real risks:** a small core, migrators as data files, a narrow first release,
   and integrating existing linters rather than competing with them.

## 4. Checked by the main session (2026-11-19)

- **GitLab count:** the GitLab API returns `x-total: 101` for `labels=RootCause::DB-Migration` on
  `gitlab-com/gl-infra/production`.
- **Snowflake 2025-12-16** status page: "our most recent release introduced a backwards-incompatible database schema
  update. As a result, previous release packages errantly referenced the updated fields".
- **GitHub August 2025 report:** "we initiated a production database migration to drop a column from a table backing
  pull request functionality. While the column was no longer in direct use, our ORM continued to reference the dropped
  column" (and the same for Copilot).
- **Buildkite:** "The migration was generated by an LLM-assisted workflow, which added the safety_assured annotation as
  part of the generated change."
- **Tuist postmortem 2:** pull request 12403 added a migration "with the same version as the migration from 18 August".
- **Drizzle v1.0.0-beta.16** release notes (2026-03-05): the out-of-order quote above.
- **HN item 49854161** (138 points, 2026-09-26): the Cloudflare quote and the orf, hakanil and fabianlindfors comments.
- **Corrected:** the devil's advocate said Atlas's backward-compatibility analyzers (BC101-104) need `atlas login`.
  Atlas's analyzers page lists them among checks "built into Atlas"; the login unlocks "more than a hundred additional
  checks". So Atlas does not clearly charge for the core N-1 check.

Everything else (the rest of the 41-row table, the issue and download counts, funding figures) is relayed from the
agents, with their V/R labels kept.

## 5. Bias and limits

- **Survivorship:** few companies publish migration postmortems; those that do (GitHub, GitLab) already have N-1 guards,
  so what remains for them skews toward NONE. Small teams, who would gain most from P1-P3, rarely publish.
- **The curated list was searched with P4 terms;** the unpicked samples are the fairer denominator.
- **Judgment calls:** CircleCI (DDL or serialized data?), the GitLab schema-cache cases, GitHub's December 2025 "drift",
  Stripe's ordering.
- **The devil's advocate was told to kill the idea,** and its searches leaned negative.
- **Issue counts are 15-36% precise;** Stack Overflow phrase search undercounts; 2026 download counts are inflated.
- **Not done:** Django's Trac tracker, user interviews, a survey of how many teams deploy with downtime.
- **The AI-agent signal is a small sample** (Buildkite plus several 2026 small projects): a hypothesis, not a finding.
