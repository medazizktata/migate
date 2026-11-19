# Language and stack, measured

Date: 2026-11-19. Machine: Apple M3 Pro (12 cores, 36 GB), macOS 26.5.1. Scope: all databases and all hosts. Only
SQLite and PostgreSQL were measured: MySQL, MariaDB and SQL Server have no local binaries or images here, and image
pulls were not allowed.

**How this was made.** A research agent benchmarked with hyperfine and read registries and source. The benchmark
sources and raw results are in `bench/stack/` (`results/` holds the hyperfine JSON and markdown). The main session
re-ran the headline numbers (see the end).

**Labels.** **M** measured here. **V** read the primary source (registry API, repo, source file). **R** relayed. **U**
unknown.

## The answer in short

- **The guard's own language barely changes a run.** A Postgres check takes about 1.0-2.5 s; Go or Rust versus Node
  changes that by about 0.1 s (3-10 %). Calling the guard through `npx` adds another 0.3-0.6 s.
- **The time goes to the throwaway database and the user's migrate command.** Postgres takes 0.6-2.1 s to start however
  it is started; a real Node migrator (node-pg-migrate) took about 0.5 s for 100 tables.
- **So the choice turns on ecosystem and distribution, not speed.** Recommendation: **Rust**, with Go a close second.
  Rust has the only single SQL parser covering Postgres, MySQL, SQLite and SQL Server (sqlparser-rs), a native
  Postgres parser, and free tooling to ship one binary through npm, pip and Homebrew (cargo-dist, maturin), as Squawk and
  ruff do.

## 1. Where wall-clock time goes (M)

Method: hyperfine 1.21.0 with `-N` (no shell) and warm-up runs; warm OS file cache. The machine was busy (load average
15-18, other benchmarks running), so compare medians and minimums.

**Calibration.** A Go hello-world against the same program with a 5 ms sleep: medians 5.2 vs 11.4 ms (n=100 each). The
benchmark sees a 5 ms difference; differences under about 3 ms are noise.

### 1a. Cold start (n=100, `results/hello.json`)

| Runtime | mean ± sd | median | min |
|---|---|---|---|
| Go | 5.7 ± 2.1 ms | 5.2 | 3.3 |
| Rust release | 6.2 ± 3.8 | 4.6 | 3.0 |
| Bun .mjs | 10.9 ± 1.0 | 10.8 | 9.3 |
| Bun .ts | 19.4 ± 8.6 | 17.0 | 9.5 |
| Deno .ts | 20.4 ± 4.0 | 19.4 | 17.4 |
| Deno .mjs | 22.3 ± 5.0 | 20.5 | 18.0 |
| Node 26 .mjs | 97.8 ± 19.6 | 92.5 | 76.6 |
| Node 26 .ts (type stripping) | 109.6 ± 4.9 | 108.9 | 99.1 |
| Calibration: Go + 5 ms sleep | 12.1 ± 2.1 | 11.4 | 9.2 |

npx overhead for an already-installed local package (n=40, `results/npx.json`): `node -e 0` 82.1 ± 9.6 ms; the bin
directly 83.8 ± 3.3; `npx mg-hello` 427 ± 183; `npx --no-install mg-hello` 573 ± 157. Homebrew's Node 26 is a 130 KB
launcher linked to 100 shared libraries (V, `otool -L`); official Node builds may start faster (R).

### 1b. SQLite introspection of 100 tables (M)

Schema from `gen_schema.py`: 100 tables, 200 indexes, 197 foreign keys, 100 CHECK constraints, 5 triggers, 5 views. Every program
runs the same ~1,600 queries (`sqlite_schema`, `pragma_table_xinfo` on every table and view, `pragma_index_list`,
`pragma_index_xinfo`, `pragma_foreign_key_list`) and printed the expected counts
`{"objects":310,"tables":100,"views":5,"triggers":5,"columns":1306,"indexes":200,"indexColumns":500,"fks":197}`,
computed separately with the `sqlite3` CLI. Calibration: on an empty database every program printed zeros, and the
assertion in `inproc.py` raised `EMPTY RUN CAUGHT` (`results/calibration.txt`).

Whole process: start, open, introspect, print (n=60):

| Driver | mean ± sd | median | min |
|---|---|---|---|
| Rust rusqlite 0.37, bundled SQLite | 11.4 ± 0.8 ms | 11.2 | 10.1 |
| Go mattn/go-sqlite3 (cgo) | 15.2 ± 0.9 | 15.0 | 14.1 |
| Go modernc.org/sqlite v1.62.0 (pure Go) | 17.9 ± 1.3 | 17.6 | 16.3 |
| Bun `--compile` | 25.1 ± 6.7 | 23.6 | 18.5 |
| Bun bun:sqlite | 27.2 ± 12.3 | 24.4 | 17.8 |
| Deno `compile` (node:sqlite) | 46.4 ± 5.9 | 45.4 | 38.3 |
| Deno node:sqlite | 55.6 ± 16.4 | 50.0 | 41.4 |
| Node 26 node:sqlite | 113.2 ± 33.1 | 100.9 | 92.1 |

Introspection alone, timed inside the process (median, n=30): Rust 7.4 ms, Bun 10.8, Go mattn 11.6, Go modernc 14.8,
Node 21.6, Deno 28.3.

### 1c. Throwaway Postgres (n=10, `results/throwaway.json`)

"schema" means: start, apply the 100-table Postgres version (`gen_pg.py`: a plpgsql trigger function, 5 triggers, 5
views), read counts, stop. Every "schema" run returned the expected counts; on a fresh database with no migrate run, the
same counts query returns all zeros.

| Option | start + select 1 + stop, mean ± sd (median) | start + schema + stop, mean ± sd (median) |
|---|---|---|
| SQLite `:memory:` via the `sqlite3` CLI (reference) | – | 28.6 ± 18.6 ms (22.3) |
| Native PG 18, APFS clone of a template cluster, `pg_ctl` | 670 ± 125 ms (608) | 1355 ± 315 ms (1359) |
| Native PG 18, fresh `initdb` | 1233 ± 145 (1268) | 2078 ± 399 (1983) |
| Docker postgres:18, local image, tmpfs, fsync off | 1225 ± 101 (1230) | 1464 ± 110 (1436) |
| PGlite 0.5.16 in Node | 1357 ± 260 (1273) | 1492 ± 227 (1416) |
| PGlite in Bun | – | 1538 ± 213 (1507) |
| PGlite + pglite-socket 0.2.19, `psql` over TCP | 1320 ± 223 (1315) | 1696 ± 436 (1524) |

- **PGlite is not faster than a real Postgres.** Its in-process advantage is only "no Docker, no binaries".
- **pglite-socket's `--run` flag is unreliable here (V, `dist/scripts/server.js`):** it splits the command on whitespace
  with no shell, and when the command exits 0 the server keeps running. A Node guard would use the library API instead
  (`socket_cycle.mjs`).
- **Postgres binaries on CI runners:** the clone option needs them; that GitHub's Ubuntu runners ship them is R.

### 1d. The user's migrate command (`results/migrators.json`, `results/realmigrate.json`)

| Subprocess | mean ± sd | n |
|---|---|---|
| `psql --version` (C) | 11.4 ± 0.5 ms | 15 |
| `node -e 0` | 83.6 ± 4.4 ms | 15 |
| `node-pg-migrate --help` | 196 ± 53 ms | 15 |
| `docker run dbmate --version` (local image) | 249 ± 27 ms | 15 |
| Cloned PG + `node-pg-migrate up` (100 tables) + stop | 1060 ± 155 ms | 10 |
| Cloned PG + `psql -f` (same 100 tables) + stop | 834 ± 64 ms | 10 |

Prisma, Payload, Rails and Django were not installed or cached, so not measured (U).

### 1e. Does the language matter?

The Postgres path is one throwaway database, two migrate runs (base, then the pull request) and two introspections:

| Step | Go or Rust | Node 26 |
|---|---|---|
| Start | ~5 ms | ~100 ms (+0.3-0.6 s through npx) |
| Throwaway Postgres | 0.6-1.3 s | same |
| Migrate command, twice | 2 × 0.2-0.5 s or more | same |
| Introspection, twice | 2 × ~10 ms | 2 × ~20 ms |
| **Total** | **~1.0-2.5 s** | **+0.1-0.15 s (+0.4-0.75 s through npx)** |

Postgres path: the language changes the total by 3-10 %. SQLite path: 10-30 %, about 0.1 s. **What actually saves
time:** reuse one database for both migrate runs, start Postgres from a cloned template instead of `initdb`, and avoid
`npx`.

## 2. Ecosystem fit (V unless labelled; registry data from 2026-11-19)

### Drivers

| Engine | Go | Rust | Node/TS |
|---|---|---|---|
| SQLite | modernc.org/sqlite v1.62.0, pure Go (BSD-3, R); mattn v1.14.53, cgo (MIT); ncruces v0.37.0, WASM (MIT) | rusqlite 0.43.0 (MIT) | node:sqlite (built in); better-sqlite3 13.2.0 (MIT); bun:sqlite |
| Postgres | pgx v5.13.0 (MIT) | postgres 0.19.14 / tokio-postgres 0.7.18 (MIT/Apache); sqlx 0.9.1 | pg 8.23.1 (MIT); postgres 4.1.1 (Unlicense) |
| MySQL / MariaDB | go-sql-driver/mysql v1.10.2 (MPL-2.0) | mysql 28.0.4 / mysql_async 0.37.1 (MIT/Apache) | mysql2 3.24.6 (MIT) |
| SQL Server | microsoft/go-mssqldb v1.12.0 (BSD-3) | tiberius 0.13.0 (MIT/Apache, needs async); sqlx has no SQL Server (R) | mssql 12.7.15 / tedious 20.3.9 (MIT) |

Size check (M): one Go binary with all four drivers, stripped, is 7.14 MB (3.08 MB gzipped), no cgo. The Rust equivalent
(postgres, mysql, tiberius, rusqlite) is 7.89 MB (3.66 MB gzipped); a clean build took 1 m 22 s.

### SQL parsers

| Dialect | Go | Rust | Node |
|---|---|---|---|
| Postgres (the real parser) | pg_query_go v6.2.5 (BSD-3, cgo); wasilibs/go-pgquery (MIT, WASM) | pg_query 18.1.0 (MIT) | libpg-query 18.2.4 (MIT, WASM, 1.8 MB) |
| MySQL | TiDB parser (Apache-2.0); vitess sqlparser v0.24.5 (Apache-2.0) | sqlparser 0.64.0, MySqlDialect | node-sql-parser 5.4.0 (Apache-2.0, 92 MB unpacked) |
| SQL Server | none mature found (U) | sqlparser, MsSqlDialect | node-sql-parser (transactsql) |
| SQLite | none dedicated found (U) | sqlparser, SQLiteDialect | node-sql-parser |

Parser speed on the 311-statement Postgres schema (M, whole process, n=20, `results/pgparse.json`): Rust pg_query.rs +
sqlparser-rs in one process 16.6 ± 1.0 ms (both parsed all 311 statements); Go pg_query_go (cgo) 20.5 ± 1.3 ms (cgo
build 8.4 s); Node libpg-query (WASM) 232.5 ± 10.2 ms; Go wasilibs/go-pgquery (cgo off) 966.7 ± 89.5 ms (it compiles
the WASM module at every start; a compilation cache might help, U).

**Why a parser matters here** (from `architecture.md` §6.4): SQLite and D1 have no out-of-process statement capture, so
data migrations there are found by parsing the migration text. A parser covering every dialect keeps that one code path.

### Introspection

**No library covers triggers on every engine,** so in any language the per-engine catalog SQL is written by hand
(SQLite pragmas, `pg_catalog`, `information_schema`, SQL Server `sys.*`). Atlas (Go) covers tables, columns, indexes and
foreign keys in its open code, not triggers, views or SQL Server (V). sea-schema 0.18.1 (Rust) covers MySQL, Postgres
and SQLite with no trigger or view support and no SQL Server. Node options are extract-pg-schema (Postgres only),
Kysely's introspector (tables and columns) and `drizzle-kit pull` (R).

### Throwaway engines, hosts

- **PGlite outside JavaScript:** Rust has pglite-oxide 0.5.1 (young); Go none found (U). PGlite was not faster anyway.
- **Embedded real Postgres:** Go embedded-postgres v1.34.0 (MIT); Rust postgresql_embedded 0.22.0 (Apache/MIT); npm
  embedded-postgres 18.6.0-beta.19 (MIT). All download Postgres binaries (R).
- **Miniflare and wrangler are Node only,** but the guard runs the user's migrate command as a subprocess, so
  `wrangler d1 migrations apply --local` works from any language. This matters for one host, not for the core.
- **D1 REST:** a plain HTTPS call, so any language.

## 3. Distribution

- **npm, one package per platform (V, npm registry):** the main package lists a native package per platform in
  `optionalDependencies`, and npm installs only the matching one. esbuild (Go) 26 platform packages plus a postinstall
  fallback; Biome (Rust) 10; oxlint (Rust) 19; squawk-cli (Rust) 7; Turbo (Rust) 6. None of the four Rust tools uses a
  postinstall script, so they work with `--ignore-scripts`.
- **npm through a postinstall download:** GoReleaser's npm feature is **GoReleaser Pro only, since v2.8**, and its docs
  say it breaks with `--ignore-scripts` (V, goreleaser.com/customization/npm/). cargo-dist's npm installer fetches the
  release archives after install (R).
- **pip wheels with the binary inside:** ruff 0.17.5 ships 18 files, squawk-cli 2.71.0 ships 7 (V, PyPI JSON). maturin
  builds these for Rust (Apache-2.0). For Go no standard tool was found (U); a short script would build the wheels.
- **Homebrew:** goreleaser OSS (MIT) and cargo-dist 0.34.0 (MIT/Apache) both generate formulas (R).
- **GitHub Action:** a composite action that downloads the release binary works for any language.
- **JavaScript single binaries:** `bun build --compile` and `deno compile` work (M); Node SEA fails on Homebrew Node 26
  ("Single executable application is disabled", M).

Sizes (M, `results/sizes.txt`):

| Binary | Size | gzip -9 |
|---|---|---|
| Rust hello | 0.33 MB | 0.15 MB |
| Go hello, stripped | 1.51 MB | 0.65 MB |
| Rust + rusqlite | 2.00 MB | 1.01 MB |
| Go + mattn, stripped | 3.58 MB | 1.64 MB |
| Go + modernc, stripped | 6.17 MB | 2.64 MB |
| Go, four drivers, stripped | 7.14 MB | 3.08 MB |
| Rust, four drivers | 7.89 MB | 3.66 MB |
| Rust pg_query + sqlparser-rs | 8.35 MB | – |
| Go pg_query_go (cgo), stripped | 8.57 MB | – |
| Bun compiled | 55.37 MB | 22.29 MB |
| Deno compiled | 64.53 MB | 27.36 MB |
| PGlite npm package | 27 MB unpacked | – |

## 4. What comparable tools chose (V: GitHub API)

| Tool | Language and licence | Notes |
|---|---|---|
| Atlas, pgroll, dbmate, golang-migrate, goose | Go (Apache-2.0 / MIT) | |
| sqldef | Go, MIT except its parser | |
| Bytebase | Go backend, TypeScript frontend; MIT core plus enterprise directories | |
| Squawk | Rust, Apache-2.0 | Now its own parser, `squawk_parser`, built like rust-analyzer's |
| Eugene | Rust, MIT | Uses pg_query 6.1 |
| sqlfluff | Python, MIT | |
| MigrationPilot | TypeScript, MIT, 10 stars | libpg-query and pgsql-parser |
| Biome, oxlint, ruff | Rust | Speed rewrites of CPU-bound parsing of many files |

**Prisma (R, vendor blog 2025-03-03 and the Prisma 7 announcement):** Prisma 7 replaced its Rust query engine with a
TypeScript plus WASM query compiler, because moving data between Rust and TypeScript inside one process cost more than
Rust saved, the engine was large (about 14 MB), and the separate binary failed to install on strict networks. A guard
that runs once per CI job as its own process has no such boundary.

**Pattern:** tools that orchestrate, introspect and diff databases are mostly Go; tools whose product is parsing lots of
code are mostly Rust.

**Contributor pool.** Expected contributions are rules and migrator adapters. If both are data files (`architecture.md`
§5-6), the implementation language stops mattering to contributors. If rules had to be code, TypeScript has the largest
pool (R, GitHub Octoverse 2025), Go is next and is what migration-tool authors write, and Rust is the hardest.

## 5. Recommendation: Rust, with rules and migrator adapters as data files

| Piece | Choice (version, licence) |
|---|---|
| Language | Rust stable (1.99 here) |
| SQLite | rusqlite 0.43.0, bundled (MIT) |
| Postgres | postgres 0.19.14, sync (MIT/Apache) |
| MySQL | mysql 28.0.4 (MIT/Apache) |
| SQL Server | tiberius 0.13.0 (MIT/Apache) on a tokio current-thread runtime |
| Parsers | sqlparser 0.64.0 (Apache-2.0) for all dialects; pg_query 18.1.0 (MIT) for exact Postgres and lock analysis |
| Throwaway Postgres | native Postgres from a cloned template cluster; postgresql_embedded 0.22.0 (Apache/MIT) or a container as fallback; PGlite only through a Node helper, if ever |
| Release | cargo-dist 0.34.0: GitHub releases, shell and PowerShell installers, Homebrew |
| npm | per-platform optionalDependencies (the Biome/oxlint/squawk pattern) |
| pip | maturin wheels (`bindings = "bin"`), like ruff and squawk-cli |
| GitHub Action | composite action downloading the release binary |
| Tests | `cargo test`; a known-bad and a known-good fixture per rule; an engine matrix in CI (Postgres as a service container; MySQL and SQL Server via Testcontainers). CLI and snapshot crates (assert_cmd or trycmd, insta) are R, not checked. |

**Why Rust:**
- **Runtime is a tie with Go:** both start in about 5 ms and both are about 8 MB with four drivers.
- **Parsing:** sqlparser-rs is the only single parser covering Postgres, MySQL, SQLite and SQL Server; native pg_query
  runs in 16.6 ms.
- **Distribution:** cargo-dist and maturin cover npm, pip and Homebrew with free tools, as Squawk and ruff show.

**Trade-offs:** the hardest of the three to contribute to (data-file rules and adapters reduce that); slow builds (82 s
from clean for the four-driver probe); rusqlite and pg_query compile C, so each platform builds on its own runner
(cargo-dist does this by default); SQL Server's driver forces async code; Atlas cannot be reused.

**When Go is the right choice:** if SQL parsing stays secondary or Postgres-only, and easier contributing or Atlas reuse
matters more. Four pure-Go drivers mean one machine builds every platform. Stack: pgx v5.13.0 (MIT),
go-sql-driver/mysql v1.10.2 (MPL-2.0), go-mssqldb v1.12.0 (BSD-3), modernc sqlite v1.62.0 (BSD-3); goreleaser OSS
(MIT); a hand-written esbuild-style npm package (GoReleaser's npm feature is Pro); a pip wheel script; testscript and
testcontainers-go for tests. Cost: a fast Postgres parser needs cgo (losing single-machine cross builds; the pure-Go
alternative takes 0.97 s per run), and Go has no mature SQL Server or SQLite parser.

**When TypeScript is the right choice:** only if in-process PGlite or Miniflare is required (CI runners with neither
Postgres binaries nor Docker), or if rules must be written as JavaScript. Then ship plain JavaScript on Node or Bun, not
55-65 MB compiled binaries. The cost is a Node runtime for Rails, Django, Go and Java teams, which an any-database tool
mostly serves.

**Measure MySQL and SQL Server startup on a Linux runner before committing:** if they behave very differently from
Postgres, the time budget above changes.

## 6. Bias and limits

- **A Mac is not a Linux CI runner.** Docker here runs inside a colima VM (4 vCPU, 8 GB); native Postgres on Linux likely
  starts faster.
- **The machine was busy** (load average 15-21), so spreads are wide: trust medians and minimums.
- **Warm cache only;** first runs after install or boot were not measured (they would hurt the 55-65 MB Bun and Deno
  binaries most).
- **Not measured:** MySQL, MariaDB, SQL Server; the Prisma, Payload, Rails and Django migrate commands; Postgres catalog
  introspection per language; ledger hashing (expected negligible, U); remote D1 and wrangler (no credentials).
- **One synthetic schema shape.**
- **The agent's own bias:** it started from the first pass's Go recommendation; the parser and distribution findings
  moved it to Rust by a narrow margin, not a wide one.

## Checked by the main session (2026-11-19)

- **Re-ran the benchmarks** (hyperfine `-N`, 5 warm-ups, n=40, load average 21): Rust hello 3.1 ± 0.6 ms, Go 6.0 ± 1.0,
  Go + 5 ms sleep 10.0 ± 0.5 (the calibration still separates), Node .mjs 96.6 ± 26.6; whole-process introspection
  rusqlite 20.1 ± 22.1, Go modernc 28.0 ± 16.7, Node node:sqlite 121.4 ± 24.0. Same order and size as the agent's
  numbers; absolute values are noisier under load.
- **Every introspector read the real schema:** rusqlite, Go mattn, Go modernc and Node node:sqlite all printed
  `tables 100, views 5, triggers 5, columns 1306, indexes 200, fks 197`, matching the `sqlite3` CLI (100 tables, 5
  views, 5 triggers). On the empty database they printed zeros.
- **GoReleaser's npm page** says "Since v2.8. GoReleaser Pro." (curl).
- **sqlparser-rs** (apache/datafusion-sqlparser-rs, Apache-2.0, 3,482 stars, pushed 2026-11-18) has dialect files for
  mssql, mysql, postgresql, sqlite, oracle and 11 others (GitHub API).
