# migate architecture

First written 2026-11-19; revised the same day after `validation.md` and `blind-review.md` (section 0 lists what
changed). Scope: any migrator, any database engine, any host. D1 is one host of the SQLite engine, the way RDS is one
host of PostgreSQL. Implementation language: Rust (`stack.md`, decided in `README.md` section 6); the type sketches below
use TypeScript notation only for readability.

**How this was made.** A research agent read primary sources (Atlas, wrangler, Miniflare, Payload, drizzle-kit, Flyway,
Prisma source; Cloudflare docs) and ran local probes, now in `bench/arch/`. The main session re-ran the probes (see the
end).

**Labels.** **V** read the primary source or ran it. **V(sub)** a research sub-agent read the primary source and its
file:line is relayed; the ones re-checked are marked "re-checked". **R** relayed. **U** unknown. Line numbers are for
the commit named, fetched 2026-11-19.

## 0. Revisions (2026-11-19, after the validation and the blind review)

| Change | Why | Where |
|---|---|---|
| **The primary check is a mixed-version test:** every live version's tests (or its models and queries) run against the database migrated to the PR head. The schema diff becomes the fast first pass, the fallback where a team has no tests, and the source of the PR comment. | A schema diff misses breaking *additions* (GitLab #5171, severity 1: a schema change made queries from running Rails processes invalid); GitHub's two August 2025 outages were intended drops of a column the running ORM still used. | §1, §2, §3, §6 |
| **Reference rules per ORM**, against **all live versions**, not "the previous version". Rails: a column is in use unless it is in `ignored_columns`; Prisma: the model definition is the reference. | Rails caches every column at boot; Vercel keeps old deployments serving 12-24 h. | §3, §6 (MG212) |
| **An override cannot cover a column a live version still references.** | Buildkite 2026: an LLM added the override. | §6.3 |
| **Rollback compatibility:** the old version must read rows the new version wrote. | CircleCI 2021. | §2, §6 (MG812) |
| **Locks without a new linter:** a timeout rule for Postgres and MySQL only (role-level settings accepted) with retries; Squawk imported from its JSON or GitLab output (it has no SARIF reporter). | Most migration incidents are locks and timeouts; SQLite/D1 have no lock timeout. | §6, §10 |
| **Read-only production checks at release** (counts that predict failure, index usage) and **three sandbox levels** (empty + checks by default; opt-in rehearsal on a production copy). | Safety depends on the data, which an empty sandbox cannot see; the rehearsal is the main defence against data loss. | §2, §7 |
| **Per-migrator ledger and order details:** comment-only edits block where the migrator checksums the whole file (Flyway); head-only history comparison for Alembic and golang-migrate; silent-skip migrators always block; duplicate versions flagged; a check after any rehearsal that every expected migration ran. | Flyway, Alembic, golang-migrate and Drizzle behaviour; Tuist 2026. | §5, §6 |
| **SQLite/D1 cascade-rebuild rule and seeded row counts.** | drizzle-orm#4938 (open): rebuilds silently cascade-delete child rows; D1 cannot turn foreign keys off. | §6 |
| **Hexagonal edges around a functional core:** `diff` and `classify` are pure functions; ports only where two real implementations or a test double exist; a CI check keeps drivers out of the core crate. | Testability of every rule both ways; keeps "any database" structural. | §3, §10 |

Everything else below is the first version's analysis, unchanged unless marked "revised".

## 1. The answer in one page

- **Three plug-in axes and a small core.**
  - **Engine** reads a schema and starts an empty copy.
  - **Host** reaches production and takes a restore point.
  - **Migrator** knows the history table, the order model and the migrate command.
  - **Core** holds the ledger, the canonical schema model, the diff, the classifier, the reporters and the release
    runner.
- **Engines and hosts hand SQL to a transport** and never talk to a driver themselves. A transport is a driver URL
  (`postgres://`, `mysql://`, `sqlserver://`, `file:`), a command (`wrangler d1 execute --json --command {sql}`) or HTTP
  (D1 `/query`, Neon or Supabase SQL APIs). D1 = SQLite engine + wrangler or REST transport + Time Travel restore point.
  Nothing in the core knows Cloudflare.
- **Migrators are declarative files** (history query, order model chosen from a closed set in the core, migrate command,
  detection): about 10-20 lines plus two fixture repos.
- **Engine readers are mostly declarative:** one catalog query per object kind returning canonical rows, plus an
  optional post-processor for what the catalog hides (SQLite CHECK constraints exist only in the `CREATE TABLE` text).
- **Out-of-tree engines, sandboxes and hosts run as a subprocess:** one JSON request on stdin, one JSON response on
  stdout, as protoc and sqlc plug-ins do. Built-in plug-ins use the same contract. Version 1 uses no gRPC and no WASM.
- **Our own canonical model, published as JSON Schema, not Atlas's.** Atlas's open `sql/schema/schema.go` has a View
  type at :53 but no Trigger type; its open SQLite inspector reads only `sqlite_master.type='table'` via
  `JOIN pragma_table_list` (`sql/sqlite/inspect.go:656-661`); its open Postgres inspector has no view or trigger code (V,
  re-checked at commit e933d0b). The Atlas docs say views and triggers are Pro (V(sub)).
- **The gate runs the user's own migrate command twice against one throwaway database** (merge base, then PR head),
  reads the schema after each run, diffs, and classifies each change on four axes: **B** breaks the old app still
  running, **L** locks or is slow, **D** loses data, **F** may fail on existing rows.
- **(revised) The primary check is the mixed-version test:** with the sandbox at the PR head, run every live version's
  tests, or load its models and run its queries, against it; then check that the old version reads rows the new one
  wrote. The schema diff is the fast first pass and the fallback where a team has no tests. For a dropped or renamed
  column, per-ORM reference rules ask whether any live version still uses it.
- **(revised) Release adds read-only production checks** (counts that predict a failure, index usage) through the same
  read-only connection as the history comparison; an opt-in rehearsal on a production copy catches data effects.
- **(revised) A functional core:** `diff(Snapshot, Snapshot)` and `classify(changes, rules, contracts)` are pure
  functions with no I/O; ports exist only at the edges.
- **Rule IDs are permanent and keyed by change kind** (`MG201 drop-column`); axes and severity are data on the rule.
- **Every suppression and contract marker must name the object and give a reason.** None of the six linters studied
  requires one (V(sub)).
- **The ledger hashes line-ending-normalised bytes**, is append-only against the base branch, and a tool-owned
  production table is the authority.
- **Outputs:** exit codes, JSON, SARIF 2.1.0, PR comment, annotations, GitHub Action.

## 2. Components and data flow

**Revised flow:**

```
pull request (no production credentials):
  1  ledger and order checks (P1, P2): files, base ledger, per-migrator order model, duplicate versions
  2  sandbox.create()  -> empty DB + transport + env
  3  checkout base; migrator.migrate(env); reader.snapshot() -> A
  4  checkout head; migrator.migrate(env); reader.snapshot() -> B; applied == expected? (silent skips)
  5  diff(A, B) -> changes -> classify(rules, contracts)            fast first pass, PR comment
  6  for each live version V (base, plus any declared still-serving versions):
       run V's tests (or load V's models and queries) against the head database      PRIMARY CHECK
       reference rules: does V still use any dropped or renamed column?
  7  rollback compatibility: write rows with head's tests or seed, read them with V's
  8  (optional) imported linter findings (Squawk JSON or GitLab), seeded row counts
  9  sandbox.destroy(); report; exit code
release (separate job, read-only role + separate migrate role):
  history comparison (P3, full or head-only per migrator) -> stop on any difference
  read-only production checks for risky findings (counts, index usage), time-limited
  (optional) rehearsal on a production copy: migrate, time it, check rows and applied set
  restore point -> migrate -> history again -> write ledger rows -> deploy, smoke tests
```

The first version's diagram follows; steps 6-8 and the release checks are what the revision adds.

```
                 +------------------ core (engine-, host-, migrator-neutral) ------------------+
repo @ base ---> | detect migrator -> ledger check (P1,P2): files, git base ledger, order model   |
repo @ head ---> |                                                             |                 |
config/auto ---> | sandbox run (P4):                                           v                 |
                 |  1 sandbox.create() -> empty DB + transport + env      findings[]           |
                 |  2 checkout base; migrator.migrate(env); reader.snapshot() -> A                |
                 |  3 checkout head; migrator.migrate(env); reader.snapshot() -> B                |
                 |     (+ statement capture where the engine allows it -> DML list)               |
                 |  4 diff(A,B) -> changes[] -> classifier(rules, contracts) -> findings          |
                 |  5 sandbox.destroy()                                                           |
                 | report: text | JSON | SARIF | PR comment | annotations ; exit code              |
                 +--------------------------------------------------------------------------------+
release (separate job, production credentials, never on PR CI):
  host.history() -> compare with ledger + files (P3) -> stop on any difference
  host.restorePoint() -> print id + exact restore command
  migrator.migrate(prod) -> host.history() again -> write ledger rows (name, sha256, commit, time)
  deploy steps, smoke tests (P5); on failure print the restore command
```

- **One sandbox, two checkouts, in one `git worktree`.** This mirrors production: the PR's migrations run on top of what
  the base left behind, and git-ignored local state (`.wrangler/state`) survives between the two runs.
- **A fresh worktree means fresh state.** `getPlatformProxy` defaults to `.wrangler/state/v3`, relative to the project
  (V, workers-sdk `packages/wrangler/src/api/integrations/platform/index.ts:393-395`, commit bf7568a).
- **Readers always query through a transport,** never through a migrator's or host's private file layout.
- **P1-P3 need no sandbox,** so they work for every engine on day one.

## 3. Plug-in interfaces (type sketches)

```ts
interface Transport { query(sql: string, params?: unknown[]): Promise<Row[]>; exec(sql: string): Promise<void>; close(): Promise<void> }
// kinds: url (postgres|mysql|sqlserver|sqlite file) | command (template with {sql}, JSON out) | http (D1 /query, Neon/Supabase SQL APIs)
interface Engine {                     // sqlite | postgres | mysql | sqlserver | ...
  id: string
  reader: { capabilities: Capability[]; snapshot(t: Transport): Promise<Snapshot> }
  sandboxes: SandboxProvider[]
  lockModel?: (c: Change, v: EngineVersion) => LockHint[]
  statementCapture?: { enable(t: Transport): Promise<void>; collect(t: Transport): Promise<Statement[]> }
}
interface SandboxProvider {            // pglite | embedded | container | url | branch-api | sqlite-file | miniflare-local
  create(opts): Promise<{ transport: Transport; env: Record<string,string>; destroy(): Promise<void> }>
}
interface Host {                       // d1 | rds | neon | supabase | cloud-sql | planetscale | url | ...
  engine: string
  transport(cfg, secrets): Transport   // read-only for history, release-only
  restorePoint?(cfg): Promise<{ id: string; restoreCommand: string; retention?: string }>
}
interface Migrator {                   // a declarative file, interpreted by the core
  id: string; detect: DetectRule[]; files: Glob; idFromFile: Pattern
  history: { query: string; checksumColumn?: string }
  order: 'lexical'|'numeric'|'timestamp-prefix'|'graph:django'|'graph:alembic'|'graph:sqitch'|'single-version'
  silentSkip?: boolean; migrate: { command: string; env: Record<string,string> }; nativeValidate?: string
}
```

**Revised: ports for the mixed-version test.**

```ts
interface LiveVersion {                // base commit, plus declared still-serving versions (tags or SHAs)
  ref: string
  worktree(): Promise<string>          // a git worktree of that version
}
interface VersionRunner {              // how a version's own checks run against a given database
  tests?: { command: string; env: Record<string,string> }   // e.g. "pytest", "pnpm test:int", "bin/rails test"
  queries?: { command: string }        // fallback: load models and run representative queries
  run(v: LiveVersion, db: { transport: Transport; env: Record<string,string> }): Promise<{ ok: boolean; failures: string[] }>
}
interface ReferenceRules {             // per ORM, data-driven where possible
  orm: 'rails' | 'django' | 'prisma' | 'drizzle' | 'payload' | 'none'
  usesColumn(v: LiveVersion, table: string, column: string): Promise<'used' | 'unused' | 'unknown'>
  // rails: used unless listed in ignored_columns; prisma: used if the model declares it; none: 'unknown'
}
```

The core stays pure: `diff` and `classify` take snapshots, the results of the version runs and the reference answers as
data, and return findings. A port exists only where there are two real implementations or a test double.

The sketches use TypeScript notation for readability only; they map one-to-one onto Rust traits or Go interfaces.

**The subprocess protocol.** An executable `migration-guard-<kind>-<name>` on PATH. `--protocol` prints `1`, as in Buf
PluginRPC (V(sub), `pluginrpc/README.md:95-97`). A call is JSON on stdin and JSON on stdout, with an optional loop of
`query` requests answered by the host's transport, as in LSP. The environment is cleared except for allow-listed keys,
as in sqlc (V(sub), `internal/ext/process/gen.go:64-71`).

## 4. The canonical schema model

**Shape.** A flat set of objects keyed by `(kind, namespace, name, parent?)` with a payload, plus dependency edges. This
is pg-delta's "fact base" (V(sub), `packages/pg-delta/src/core/fact.ts:19-34`). The diff is a set difference on (id,
payload hash), then a field comparison per kind.

| Kind | Payload | Notes |
|---|---|---|
| namespace | name | PG and SQL Server schemas; MySQL database; SQLite main and attached |
| table | flags (strict, without rowid), `extra{}` | |
| column | ordinal, type `{raw, family, params}`, nullable, default (raw), generated `{expr, stored\|virtual}`, identity or autoincrement, collation | Prisma's raw-plus-family pattern (V(sub), `sql-schema-describer/src/lib.rs:648,680-708`) |
| primary_key, unique | columns | |
| check | expression, validated? | PG `NOT VALID` matters for the L axis |
| foreign_key | columns, referenced table and columns, on_delete, on_update, deferrable, validated? | |
| index | unique, parts `[{column\|expr, desc}]`, predicate, method | |
| view | definition (engine-rendered), materialized, columns | |
| trigger | table, timing, events, level, definition | |
| routine | kind, identity args, definition hash | |
| enum | values, scope `namespace\|column` | Prisma turns a MySQL ENUM into `{table}_{column}` (V(sub), `mysql.rs:616`) |
| sequence | type, owned-by | |

**Capabilities.** Each reader declares the kinds it sees. A blind kind is reported as "not checked", never as "none"
(rule MG801).

**Types.** Keep the engine's own render (`format_type`, `pg_get_viewdef`, `pg_get_indexdef`, `sqlite_schema.sql`):
equality within one engine version is all a before/after diff on one sandbox needs (pg-schema-diff keeps definitions as
text the same way; V(sub), `internal/schema/schema.go:505,525`). Add `family` and `params` only for rules that compare
across a change, such as narrowing. SQLite affinity folds `INT` and `INTEGER` into one family (R).

**What each catalog exposes.**
- **SQLite (V, probe):** `pragma_table_list`; `pragma_table_xinfo` (covers views too; `hidden=2` marks a virtual
  generated column); `pragma_index_list` (has a `partial` flag); `pragma_index_xinfo` (`cid=-2` marks an expression);
  `pragma_foreign_key_list` (includes actions). Only in the `sql` text: CHECK, partial-index WHERE, index expressions,
  generation expressions, trigger and view bodies. drizzle-kit parses them with regexes (V,
  `drizzle-kit/src/serializer/sqliteSerializer.ts:880-905`, commit 3bcb6a2); Prisma reads partial WHERE the same way
  (V(sub), `sqlite.rs:494-497`).
- **PostgreSQL (V, probe):** `pg_attribute` with `pg_attrdef` (`attgenerated`), `pg_constraint` with
  `pg_get_constraintdef` (shows `NOT VALID`), `pg_indexes`, `pg_views`, `pg_trigger` (`NOT tgisinternal`), `pg_enum`,
  `pg_sequences`, `pg_proc`. One query per kind.
- **MySQL/MariaDB (R):** information_schema `COLUMNS` (with `GENERATION_EXPRESSION`, and `COLUMN_TYPE` carrying ENUM
  values), `STATISTICS` (functional key parts from 8.0.13), `CHECK_CONSTRAINTS` (from 8.0.16), `KEY_COLUMN_USAGE` with
  `REFERENTIAL_CONSTRAINTS`, `TRIGGERS`, `VIEWS`, `ROUTINES`. No partial indexes. No sequences in MySQL; MariaDB has
  them from 10.3.
- **SQL Server (R):** `sys.columns`, `sys.computed_columns`, `sys.default_constraints`, `sys.indexes.filter_definition`,
  `sys.check_constraints`, `sys.foreign_keys`, `sys.triggers` with `sys.sql_modules`, `sys.views`, `sys.sequences`. No
  enums, no expression indexes. Atlas's open repo has no SQL Server driver (V(sub)).

## 5. Migrators: declarative files (example)

```toml
id = "payload"
detect = [{ file = "package.json", contains = "@payloadcms/db-" }, { glob = "**/payload.config.ts" }]
files = "{migrations_dir}/*.ts"
id_from_file = "^(?<id>\\d{8}_\\d{6}_.+)\\.ts$"
order = "timestamp-prefix"
history.query = "SELECT name AS id FROM payload_migrations WHERE batch <> -1"
migrate.command = "npx payload migrate"
```

- **Order models are core code, chosen by name.** The first pass found that "pending" means something different in each
  of 21 migrators. `silentSkip` (stable Drizzle, golang-migrate) makes an out-of-order migration an error, because the
  migrator will never run it. The graph models check that the graph stays consistent and has one head.
- **Escape hatch:** `order = "command:<cmd>"` runs a script that prints JSON.
- **(revised) Three more fields per migrator:**
  - `checksum = "whole-file" | "content" | "none"`: what the migrator itself hashes. Flyway hashes every line including
    comments (V, `ChecksumCalculator.java`), so a comment-only edit is an error there (it fails `flyway validate` in
    production); where the migrator hashes nothing, it is a lower-severity note.
  - `history = "full" | "head-only"`: Alembic and golang-migrate store only the current version, so P3 compares that
    version, not a list.
  - `tests` and `queries` (optional): how this project's own tests run against a given database, for the mixed-version
    test; auto-detected where conventional (`pytest`, `manage.py test --keepdb`, the package's test script).
- **Silent-skip migrators always block** on an out-of-order migration (stable Drizzle, golang-migrate: the migration
  would never run); duplicate versions are flagged for every migrator (Tuist 2026).
- **`nativeValidate`** runs `flyway validate` and the like for the migrators that keep their own checksums (Flyway,
  Liquibase, Sqitch, Prisma, Atlas, Supabase).
- **Declarative is enough:** pre-commit (`clientlib.py:240-260`) and Renovate's regex manager show the style scales
  (V(sub)).

## 6. The breaking-change classifier

### 6.1 How others structure rules (V(sub))

| Tool | IDs | Severity | Suppression | Reason required? | SARIF |
|---|---|---|---|---|---|
| Atlas sqlcheck | class + number (DS103, BC102, MF103); `sqlcheck.Code()` panics on a duplicate (`sqlcheck.go:297-304`) | error or warn per analyzer; `force` makes a check unsuppressable | `-- atlas:nolint DS103` | no; extra words are read as rule names (`migratelint/lint.go:1048`) | no |
| Squawk | kebab name only, 77 rules, aliases kept | all warnings | `-- squawk-ignore` | no; a trailing reason is discarded (`ignore.rs:23-35`) | no |
| strong_migrations | message keys | raises | `safety_assured {}` | no | n/a |
| Eugene | E1-E15, W12-W14 (severity in the ID) | in the ID | `-- eugene: ignore E3` | no | no |
| MigrationPilot | MP001-MP112 plus a name; never reused; new rules start as warnings | critical or warning; exit codes 2 and 1 | `-- migrationpilot-disable` | no; reports stale directives | yes, without partialFingerprints |
| django-migration-linter | DROP_COLUMN and similar | error or warning | per migration or per code | no | no |

### 6.2 The scheme recommended here

- **IDs** are `MG###`, grouped by object, never reused, each with a name. Axes and severity are data, so knowledge can
  change without renumbering (Eugene shows the trap of severity inside the ID).
- **One finding per change.** SARIF tags carry the axes; the level comes from the worst axis that applies.
- **Default severity:** B and D are errors. F and L are warnings, because the sandbox is empty and cannot know table
  sizes. A table-size hint can promote L.
- **Stability policy:** copy MigrationPilot's: a new rule ships as a warning and becomes an error at the next major
  release.

| ID | Change | B | L | D | F | Notes |
|---|---|---|---|---|---|---|
| MG001 | applied migration edited | | | | | P1; unsuppressable |
| MG002 | applied migration removed | | | | | P1 |
| MG003 | ledger entry rewritten or removed versus base | | | | | P1; closes the rehash bypass |
| MG004 | new migration orders before production's last | | | | | P2, per order model |
| MG005 | graph inconsistent or several heads | | | | | P2 (Django, Alembic, Sqitch) |
| MG006 | two migrations with the same version | | | | | revised; Tuist 2026 |
| MG007 | comment- or whitespace-only edit of an applied migration | | | | | revised; error where the migrator checksums the whole file (Flyway), note elsewhere |
| MG010 | production ran a migration the repo lacks | | | | | P3 |
| MG011 | production ran one outside a release | | | | | P3 |
| MG012 | ledger says run, production has no record | | | | | P3 |
| MG101 | drop table | x | | x | | |
| MG102 | rename table | x | | | | drop + add unless explicit |
| MG103 | table rebuilt (SQLite copy-swap, MySQL COPY) | | x | ? | | D if a column is lost |
| MG104 | table rebuilt while children reference it with ON DELETE CASCADE | | | x | | revised; SQLite/D1, drizzle-orm#4938 |
| MG105 | row count dropped on the seeded database | | | x | | revised; needs a seed |
| MG201 | drop column | x | x | x | | |
| MG202 | rename column | x | | | | |
| MG203 | add NOT NULL column without default | x | | | x | |
| MG204 | set NOT NULL | x | x | | x | PG scans unless a valid CHECK exists |
| MG205 | narrow type or length | x | x | x | x | via family and params |
| MG206 | other type change | | x | | | |
| MG207 | drop default | x | | | | |
| MG208 | add or change generated or identity column | | x | | | |
| MG209 | change a column default | x | | | | revised; breaks writes from processes with a cached schema |
| MG210 | add a column | ? | | | | revised; note only in the diff (GitLab #5171 class); the mixed-version test decides |
| MG212 | drop or rename a column a live version still uses | x | | x | | revised; per-ORM reference rules; no contract can cover it |
| MG301 | add CHECK | x | x | | x | L unless NOT VALID |
| MG302 | add UNIQUE | x | x | | x | |
| MG303 | add FK | x | x | | x | |
| MG304 | drop constraint | | | | | note only |
| MG305 | add or change primary key | x | x | | x | |
| MG401 | index built without the online option | | x | | | PG CONCURRENTLY, MySQL INPLACE |
| MG402 | drop index | | | | | note only |
| MG403 | drop or alter an index production still scans | | x | | | revised; release-time production check |
| MG411 | migration sets no lock or statement timeout | | x | | | revised; Postgres and MySQL only; role-level settings accepted; pair with retries |
| MG501 | drop or rename view | x | | | | |
| MG502 | view columns changed | x | | | | |
| MG503 | add trigger | x | | | | can reject old writes |
| MG504 | drop or change trigger | x | | | | |
| MG505 | drop or change routine signature | x | | | | |
| MG601 | remove or rename enum value | x | | x | | |
| MG602 | add enum value not at the end (MySQL) | | x | | | |
| MG603 | drop sequence | x | | | | |
| MG701 | DML in a migration | | x | ? | | statement capture; needs review |
| MG702 | UPDATE or DELETE without WHERE | | x | x | | |
| MG801 | reader blind to a kind the migration touched | | | | | "not checked" |
| MG802 | an expected migration did not apply (sandbox or rehearsal) | | | | | revised; silent skips |
| MG803 | a failed migration left changes behind (not atomic) | | | x | | spike-7: Prisma and stock Payload on D1 |
| MG804 | the sandbox was not the database migrated (no history rows after the base run) | | | | | spike-7: a misnamed variable made Django migrate its own default database; unsuppressable |
| MG810 | a live version's tests fail against the migrated database | x | | | | revised; the primary check |
| MG811 | a live version's queries fail against the migrated database | x | | | | revised; fallback when there are no tests |
| MG812 | a live version cannot read rows the new version wrote | x | | | | revised; rollback compatibility |
| MG820 | imported finding (Squawk or another linter) | | x | | | revised; level mapped from the source |
| MG901 | marker without a reason, or matching nothing | | | | | hygiene |

### 6.3 Contract markers and suppressions

Syntax, in any comment style in the migration file, or as `[[contract]]` in config:

`migration-guard: contract drop-column users.legacy_email -- both apps stopped reading it in v42`

- **Scoped to an object and a rule.** No file-wide "allow all" (the prior art's `// CONTRACT:` covered the whole file; V,
  `prior-art/the Payload-on-D1 app/the origin project's migration check:39-40`).
- **A reason is required.** golangci-lint's `require-explanation` and Clippy's `allow_attributes_without_reason` enforce
  the same (V(sub)).
- **Stale markers fail. MG001-MG003 and MG010-MG012 cannot be suppressed. Every accepted contract is listed in the PR
  comment.**
- **(revised) A contract cannot cover MG212 or MG810-MG812:** a column a live version still uses, or a failing
  mixed-version test, stays an error whatever the marker says. The fix is the expand/contract order (stop using it, ship,
  then drop), not a marker. Buildkite 2026 is the case: an LLM-assisted workflow added the override.

### 6.4 Data migrations (secondary capture)

- **PG:** `-c log_statement=all`, or `ALTER DATABASE … SET log_statement`.
- **MySQL:** `SET GLOBAL general_log=ON` with `log_output=TABLE`.
- **SQL Server:** an Extended Events session on `sql_statement_completed` (V(sub)).
- **SQLite:** no out-of-process hook. Fall back to scanning the migration text for DML (a multi-dialect SQL parser helps
  here, see `stack.md`), reported as "review", never as a pass.
- **PGlite capture:** U.
- **Optional `seed.sql`**, run after the base migrations, with per-table row counts and hashes compared. Off by default.
  (revised) It feeds MG105 and the rollback-compatibility check (MG812).
- **(revised) The rehearsal on a production copy is the main defence against data effects** (GitLab #6072, GitHub
  2023-09, drizzle-orm#4938), which no schema diff sees. See §7.

## 7. Throwaway databases, all engines

| Provider | Engines | Needs | Evidence |
|---|---|---|---|
| memory or temp file | SQLite | nothing | V |
| miniflare-local (`wrangler … --local --persist-to <tmp>`) | SQLite as D1 | Node, wrangler | V, `wrangler/src/d1/execute.ts:303-348` |
| PGlite + pglite-socket | PG | Node, or the `pglite-server` CLI | **V, ran it:** a separate process with the `pg` driver migrated through a plain URL. One connection by default; "not all use cases are guaranteed" (README:12,167) |
| native binaries from a cloned template cluster | PG | Postgres server binaries | measured fastest in `stack.md` |
| embedded binaries | PG (zonky, fergusstrange, embedded-postgres npm, theseus-rs); MySQL tarball; MariaDB4j | a download | V(sub) |
| container (Testcontainers or `docker run`) | PG, MySQL, MariaDB, SQL Server | Docker | V(sub). Atlas `docker://`: `docker run --rm`, one-minute readiness wait (`cmd/atlas/internal/docker/docker.go:401,597`). SQL Server is container-only; Azure SQL Edge was retired 2025-09-30 |
| user URL (CI `services:`, shared server) | all | CREATE DATABASE privilege | V(sub). Prisma: `prisma_migrate_shadow_db_<uuid>`, `DROP … WITH (FORCE)` (`shadow_db.rs:56,178`). sqlc: `sqlc_managed_<hash>` (`dbmanager/client.go:50-56`) |
| branch API | Neon, PlanetScale, Supabase | a token, money | V(sub); an option only |

- **Default:** the cheapest provider that matches production's engine version. Report the sandbox version next to
  production's.
- **(revised) Three levels, each finding labelled with the level that produced it:**
  1. **Empty sandbox** (default, pull request): schema diff, mixed-version test, reference rules.
  2. **Read-only production checks** (default, release): one bounded, time-limited query per risky finding through the
     read-only connection: NULLs for a new NOT NULL, duplicates for a new UNIQUE, orphans for a new FK, the longest value
     for a narrowed type, the catalog's row estimate for lock risk, index-scan counts before dropping an index (MG403).
     Counts only; no rows leave production; a timeout reports "not checked", never a pass.
  3. **Rehearsal on a production copy** (opt-in, protected job): `pg_dump`/`pg_restore`, a restored host snapshot, a
     Neon branch, later other adapters (a GFS full clone once the general solution is proven). Measures duration and
     catches failures nobody thought to query. Copying production data needs masking or a deliberate decision.
- **PGlite is one PG version without most extensions (R),** so it is a fidelity risk.
- **Reset with `CREATE DATABASE … TEMPLATE`** (V(sub), testcontainers-go `modules/postgres/postgres.go:269-310`).

### 7.1 D1, as one host example

- **Supported PRAGMAs** (V, Cloudflare `d1/sql-api/sql-statements`): `table_list`, `table_info`, `table_xinfo`,
  `index_list`, `index_info`, `index_xinfo`, `foreign_key_list`, `foreign_key_check`, `quick_check`, and `sqlite_master`
  queries. Skip the internal `_cf_KV` table (V).
- **Table-valued pragma functions on production D1:** the docs do not say (U). Strong indirect evidence: drizzle-kit uses
  `JOIN pragma_table_xinfo(m.name)`, `pragma_foreign_key_list` and `pragma_index_list` (V,
  `sqliteSerializer.ts:543-544,707-708,779-781`), and its `d1-http` driver sends them to `/query` (V,
  `drizzle-kit/src/cli/connections.ts:1011-1074`). Not verified against D1 today. Fallback: the per-table PRAGMA
  statement forms.
- **Atomicity of a wrangler-applied file:** probably all-or-nothing (each link V; the conclusion is an inference):
  wrangler appends `INSERT INTO d1_migrations` to the file and sends one command
  (`wrangler/src/d1/migrations/helpers.ts:334-350`, `apply.ts:164-185`); the REST `/query` endpoint runs multiple
  statements "as a batch"; locally wrangler uses `db.batch()` (`execute.ts:344-348`), and Worker-API batches "are SQL
  transactions … rolls back the entire sequence". **Payload's D1 migrations run through drizzle, not wrangler; their
  atomicity is U.**
- **Access:** REST `POST /accounts/{id}/d1/database/{uuid}/query` with `{sql, params}` or `{batch}`; CLI
  `wrangler d1 execute --json [--local --persist-to]` (V, `execute.ts:88-96`).
- **Restore point:** `wrangler d1 time-travel info` prints the bookmark; `restore --bookmark` or `--timestamp`; any
  minute within 30 days (V).
- **Local file:** `<persist>/v3/d1/miniflare-D1DatabaseObject/…` (V, `miniflare/src/plugins/d1/index.ts:124-134`). Query
  it through wrangler; do not read the file.
- **Payload pointed at a throwaway:** the template calls `getPlatformProxy({ environment: CLOUDFLARE_ENV,
  remoteBindings: isProduction })` with no `persist` option (V, `templates/with-cloudflare-d1/src/payload.config.ts:70-76`,
  commit 7a41086), so outside production it uses `.wrangler/state/v3`, and a fresh worktree gives a throwaway.

## 8. Hosts: history and restore points (V(sub) unless noted)

| Host | History read | Restore point | In place? |
|---|---|---|---|
| D1 | wrangler `--json`, REST | Time Travel, 30 days (V) | yes |
| RDS | URL with a read-only role | `create-db-snapshot`; PITR | new instance |
| Aurora | URL, Data API | cluster snapshot; MySQL Backtrack up to 72 h | Backtrack only |
| Cloud SQL | Auth Proxy | `gcloud sql backups create`; PITR clone | new instance |
| Azure SQL | URL | `CREATE DATABASE x AS COPY OF y` | copy |
| Azure PG/MySQL Flexible | URL | on-demand backup; PITR | new server |
| Neon | URL | `POST …/branches/{id}/snapshot`; restore keeps an `_old` branch | yes |
| Supabase | URL | PITR add-on; no on-demand snapshot API found (U) | in place, downtime |
| PlanetScale (Vitess) | pscale | `pscale backup create`; revert within 30 min | new branch |
| Turso | HTTP | `turso db create --from-db --timestamp` | new database |
| Self-hosted | URL | pg_dump or basebackup + WAL; mysqldump + binlog | manual |

**Consequence:** a restore point is an id plus the exact restore command and its retention, printed before migrating. It
is not a one-click rollback.

**(revised) Many databases per host:** per-tenant setups (common on D1, which caps a database at 10 GB) need the history
comparison, the production checks and the migration itself run across every tenant database, with one report.

## 9. Ledger and trust

- **Hash:** sha256 of the file with the BOM removed and CRLF or CR turned into LF. Prisma PR #2101 (merged 2021-08-03)
  tries both line endings (V, `schema-connector/src/checksum.rs:12-23`); Flyway computes CRC32 line by line with the BOM
  filtered (V, `ChecksumCalculator.java:60-84`, commit 15b6298). Calibrated in `bench/arch/probe/hash-norm.mjs`.
- **Per-file entries, not chained.** atlas.sum chains on purpose (the hasher is never reset, so each entry covers every
  previous file; V, `sql/migrate/dir.go:662-679`, e933d0b), which forces merge conflicts. Our order models give the same
  protection without the friction.
- **Append-only against the base branch**; fail if the base cannot be read in CI (V, prior art
  `the origin project's migration check:69-92`).
- **The authority is a tool-owned production table** `migration_guard_ledger(id, sha256, commit, released_at,
  tool_version)`, written by the release after re-reading the migrator's history (it does not assume everything ran).
  The committed ledger is the CI copy, and both must agree (MG010-MG012). Fallback where the table cannot be created: the
  committed ledger plus the migrator's native checksum.
- **Credentials:** none in PR CI; the release job uses environment-scoped secrets, a read-only role for history and a
  separate migrate role; URLs are redacted in all output.
- **Optionally also hash the SQL captured in the sandbox,** as Supabase stores `statements`: source hashes miss edits to
  imported helpers (R, inference).

## 10. Integration, config and language constraints

**Exit codes:** 0 clean; 1 warnings (only with `fail_on="warning"`); 2 errors; 3 could not check (sandbox failed, base
unreadable, reader blind).

**Outputs:** a JSON envelope `{version, engine, sandbox{provider,version}, findings[], contracts[], blind[]}`; SARIF
2.1.0 (rules need `id`, `shortDescription`, `fullDescription`, `help`; results need `message`, `locations`,
`partialFingerprints` = ruleId + object id; limits 10 MB gzipped and 25,000 results per run; private repos need GitHub
Advanced Security, so also emit annotations and one sticky PR comment).

**Packaging:** a GitHub Action (`fetch-depth: 0`, sandbox setup, `check`, SARIF upload, sticky PR comment); pre-commit
for ledger checks only; CLI verbs `check`, `release`, `snapshot`, `explain`.

**(revised) Imported findings:** Squawk has no SARIF reporter (its `--reporter` values are `tty, gcc, json, gitlab`;
V), so migate reads its JSON or GitLab output and reports each finding as MG820 with the source rule's name. migate's
own output stays SARIF for GitHub code scanning.

**Auto-detection (R):** prisma/migrations with migration_lock.toml; drizzle.config with meta/_journal.json; wrangler
`d1_databases[].migrations_dir`; a `@payloadcms/db-*` dependency; db/migrate with a Gemfile naming rails; manage.py with
`*/migrations/0001_*.py`; alembic.ini; flyway.toml/conf; knexfile; sqitch.plan; atlas.hcl; supabase/config.toml. When
detection is ambiguous, list the candidates and ask; never guess.

```toml
migrator = "payload"
migrations_dir = "db/migrations"
ledger = "db/deployed.json"
base = "origin/main"
fail_on = "error"
[engine]
id = "sqlite"
sandbox = "miniflare-local"        # postgres: "pglite" | "container:postgres:16" | "url:$SHADOW_URL"
[host.production]
kind = "d1"                        # or url, neon, rds, ...
database = "app-db"
restore_point = true
[versions]                         # revised: the mixed-version test
tests = "pnpm --filter @app/api test:int"    # run against the migrated database; auto-detected when omitted
live = ["base"]                    # plus tags or SHAs still serving traffic (e.g. under skew protection)
[rules]
MG401 = "off"
MG203 = { level = "error" }
[[contract]]
migration = "20261201_120000_drop_legacy"
rule = "MG201"
object = "users.legacy_email"
reason = "both Workers stopped reading it in release 42"
```

**Language constraints.**
- **Not constrained:** sandboxes, migrators and hosts are subprocesses or SQL behind a transport. PGlite ships a
  `pglite-server` CLI (V).
- **(revised) Decided:** Rust, one binary (`stack.md`). The mixed-version test runs the team's own test command as a
  subprocess, so it adds no language constraint.
- **Mildly constrained:** a single-binary CI tool favours Go or Rust. Drivers for all four engines exist in Go, Rust and
  Node (`stack.md`).
- **Trap (V, hit while probing):** a JavaScript host running PGlite in-process must start the migrator asynchronously. A
  synchronous start deadlocks, because PGlite serves the socket from the same event loop; the first probe hung for 180 s.
- **Reusing Atlas would force Go** and buys only the MySQL, PG and SQLite inspectors, without views or triggers.

## 11. Verification run, calibrated both ways

| Check | Pass | Sabotage → failure |
|---|---|---|
| SQLite introspection (`bench/arch/probe/sqlite-introspect.mjs`) | 6 objects, 13 columns incl. views, 3 indexes, 1 FK; partial flag; `cid=-2`; `hidden=2` | `pragma_bogus_list()` gave "no such table". An added trigger made the diff non-empty (exit 1). Two fresh runs were byte-identical. |
| SQLite trap | – | `DROP COLUMN deleted` failed: "error in index posts_live after drop column" (SQLite 3.53.4) |
| PGlite + subprocess migrator (`bench/arch/pglite/probe.mjs`) | trigger, enum `{ok,sad}`, partial index `WHERE (NOT deleted)`, stored generated column, 2 sequences, function, view | A `CHECK … NOT VALID` gave exactly one added constraint. A bad migration exited 1 with "relation "nope" does not exist". Two fresh runs gave equal snapshots. |
| Normalised hash (`bench/arch/probe/hash-norm.mjs`) | LF and CRLF+BOM hash equal; a one-character edit still differs | Removing the CRLF→LF step (`hash-norm-sabotaged.mjs`) turned the equality check to FAIL |

**Not run:** MySQL, SQL Server, production D1, wrangler or Miniflare locally (to avoid the workerd download), real
migrators (Payload, Prisma, Rails) in a sandbox, statement capture, CI timings.

## 12. Alternatives rejected

- **Atlas as the engine layer:** no triggers or views in its open model or inspectors (V, re-checked); those are Pro
  (V(sub)); no open SQL Server driver; forces Go; licence-drift risk. Its shapes are worth borrowing:
  `ColumnType{Raw,Type,Null}`, `IndexPart`, `ReferenceOption`.
- **go-plugin / gRPC:** authors need protobuf, a health service and a handshake (V(sub),
  `docs/guide-plugin-write-non-go.md:82-130`); in practice plug-ins end up in Go (R).
- **WASM (Extism, WASI):** a WASM reader cannot open a database socket, so the host must provide `query()` (V(sub),
  Extism README:30); component support in Go and JS hosts is narrower (R/U). A possible later tier.
- **Code plug-ins for migrators:** not needed; a query, a pattern and an enum suffice.
- **Text or SQL-parse linting as the main gate:** misses triggers, views, constraints and runtime SQL (V, first-pass
  probe). Kept as a secondary signal.
- **The atlas.sum chain:** merge friction; the order models cover the same ground.
- **A managed branch as the default sandbox:** needs a token, costs money, slower.
- **Reading Miniflare's file directly:** its layout is internal.

## 13. Open risks, ranked

**(revised) Added by the review:**
- **The mixed-version test's cost and reliability are unmeasured.** Running a live version's test suite against the
  migrated database may be slow or flaky for small teams, and many projects have tests that do not touch the database.
  Spike TASK-31 measures it on the Payload-on-D1 app and a Django sample.
- **Reference rules are per ORM and partial.** Rails and Prisma are understood; Django, Drizzle and Payload are not yet
  verified; raw SQL and query builders give "unknown", never "unused".
- **Declaring every live version** needs the team to say which versions still serve traffic; a wrong list misses an
  old version.

**First version's ranking:**

1. **Running arbitrary migrators in a sandbox.** (Partly retired by `spike-7.md`: Payload, Prisma, Django and Rails ran
   the two-checkout flow on SQLite and Postgres, offline, with per-migrator env extras and a database location for
   project-defined layouts.) It needs the app's runtime, config and secrets; Payload's D1 path needs a
   worktree; Prisma needs CREATEDB. Only PG with a toy migrator is proven.
2. **"Breaks the old app" is approximated from the schema diff.** Contracts handle false positives; false negatives
   remain (partial-reject triggers, defaults the app relied on). Stronger optional mode: run the base branch's test
   suite against the post-migration schema (cost U).
3. **Reader completeness:** SQLite text parsing, fixtures for every object kind, honest "blind" reporting.
4. **Sandbox fidelity:** PGlite versions and extensions; L findings cannot know table sizes.
5. **D1 unknowns:** pragma functions on production D1; atomicity of Payload's own D1 migrations.
6. **No out-of-process statement capture for SQLite and D1.**
7. **The production ledger table needs a write credential,** and some hosts or roles forbid creating it.
8. **Rule-ID churn:** adopt the permanent-ID policy from v1.

## 14. Bias and limits

- **Origin bias.** The brief and prior art came from a Payload-on-D1 project, and the scope moved to all engines
  mid-task. D1 detail may be over-weighted. MySQL and SQL Server are on paper only (R).
- **Relayed work.** Sub-agents' V claims are relayed as V(sub); only the Atlas, D1, wrangler, Miniflare, Payload,
  drizzle-kit, Flyway, Prisma and atlas.sum claims were re-checked.
- **Confirmation risk.** The first pass already favoured the schema-diff approach; this studied how more than whether.
  Risk 1 is the open counter-argument.
- **Measurements:** one machine (Apple M3 Pro, macOS 26.5.1), tiny schemas. Line numbers will drift.

## Checked by the main session (2026-11-19)

Re-ran the probes in `bench/arch/`: `hash-norm.mjs` passed all three checks and `hash-norm-sabotaged.mjs` failed the
line-ending check, as designed (the script still exits 0 on a FAIL line; the verdict is in its output).
`sqlite-introspect.mjs` ran and printed indexes, expression parts and foreign keys. `pglite/probe.mjs` exited 0 in 1.9 s
end to end and listed the trigger, enum, partial index, generated column, sequences, function and view. Atlas's open
inspectors were checked independently through the GitHub API: `sql/sqlite/inspect.go` 0 mentions of trigger or view,
`sql/postgres/inspect.go` 0 of trigger, `sql/schema/schema.go` a `View` type and no trigger type.
