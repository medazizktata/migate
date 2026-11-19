# Benchmarks and probes

The sources behind the numbers in `../stack.md` and `../architecture.md`, kept so every figure can be re-run. Built
binaries, `node_modules`, Postgres data directories and the generated SQLite files are not committed.

## stack/ (language and stack, `../stack.md`)

- `gen_schema.py`, `gen_pg.py`: generate the 100-table SQLite and Postgres schemas (`schema.sql`, `schema_pg.sql`).
- `hello/`: hello-world in Go, Rust, Node (.mjs, .ts), Bun, Deno; `gosleep/` is the calibration (adds 5 ms).
- `introspect/`: the same full schema read in Rust (rusqlite), Go (mattn, modernc), Node and Bun; `inproc.py` times the
  read inside each process and refuses an empty run.
- `pg/`: throwaway Postgres cycles: native cloned cluster, fresh initdb, Docker, PGlite, PGlite + socket.
- `migrators/`: node-pg-migrate against a cloned Postgres.
- `eco/`: Postgres parser timings (Rust pg_query + sqlparser-rs, Go pg_query_go with and without cgo, Node libpg-query).
- `sizes/`: four-driver binaries in Go and Rust for size.
- `results/`: hyperfine JSON and markdown, calibration output, sizes.

Re-run, for example:

```sh
python3 -I gen_schema.py > schema.sql && sqlite3 schema.db < schema.sql   # seeded: reproduces the committed schema.sql
hyperfine -N -w 5 -r 40 'hello/go/hello-go' 'hello/gosleep/hello-go-sleep5' 'node hello/hello.mjs'
```

Cargo builds on this machine go through the `CARGO_TARGET_DIR` wrapper in `~/.zshenv`; run them as
`zsh -c 'cd <dir> && cargo build --release'`.

## validation/ (problem validation, `../validation.md`)

- `data/github_issue_counts.tsv`: issue counts per migrator and theme, with the exact search queries (tab separators
  are written as literal `\t`).
- `data/stackoverflow_counts.tsv`: Stack Overflow counts per tag and phrase.
- `data/rubygems_yearly.tsv`, `pypi_*.tsv`: download histories used for the demand trend.
- `data/repo_creation_counts.tsv`, `new2026_*.tsv`: GitHub repositories created per year for each search phrase.
- `data/watch_halfyear.tsv`, `repos.tsv`, `native_feature_requests.tsv`: star-event and feature-request data.
- `scripts/gh_counts.sh`, `scripts/so_counts.sh`: re-run the counts (`OUT=<dir>` sets the output folder; GitHub search
  is rate-limited, so the script sleeps between calls). `txt.py` strips a fetched page to text.

The fetched third-party pages and raw API responses (about 23 MB) are not committed.

## spike-7/ (real migrators in the sandbox, `../spike-7.md`)

- `flow.sh <repo> <base> <head> <outdir> <reset-cmd> <migrate-cmd> <snap-cmd>`: the two-checkout flow; prints the
  schema diff and exits with the head run's exit code.
- `snap_sqlite.sh <db>` (reads a copy, WAL included), `snap_pg.sh`: schema snapshots; `mkheads.sh`: the four head
  branches; `timeloop.sh`: repeated timings.
- `jail.sb`: `sandbox-exec -D WORKDIR=<dir> -f jail.sb <cmd>` denies non-loopback network and writes outside WORKDIR.

## arch/ (architecture probes, `../architecture.md` §11)

- `probe/sqlite-introspect.mjs`: full SQLite catalog read with node:sqlite, including partial and expression indexes and
  generated columns.
- `probe/hash-norm.mjs`: the ledger hash (BOM and line endings normalised); `hash-norm-sabotaged.mjs` drops the
  line-ending step and must report FAIL. Note: both exit 0; read the PASS/FAIL lines.
- `pglite/probe.mjs`: PGlite behind pglite-socket, a separate-process migrator using only a `DATABASE_URL`, then a
  `pg_catalog` read of all eight object kinds. `npm install` in `pglite/` first.
