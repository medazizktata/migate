# Production versions: when the sandbox's database is newer than production's

Research for TASK-67, 2026-11-20. Question: a migration passes migate's sandbox and fails at deploy because production
runs an older engine version. How common is it, how do other tools handle it, and what should migate do?

## The gap

The sandbox applies migrations through the project's own tool, so with that tool's SQLite (3.54.1 in Wrangler's local
D1, the same build migate carries), or with the newest Postgres installed on the machine (`bindir()` picks the highest
major). Production may be older. A migration using a newer feature passes the gate and fails in production.

## How common: what production actually runs

System SQLite on Linux servers, from the distributions' own archives (original release version; security updates keep
the feature set):

| Server OS | SQLite |
|---|---|
| Ubuntu 20.04 (focal) | 3.31.1 |
| Debian 11 (bullseye) | 3.34.1 |
| Ubuntu 22.04 (jammy) | 3.37.2 |
| Debian 12 (bookworm) | 3.40.1 |
| Ubuntu 24.04 (noble) | 3.45.1 |
| Debian 13 (trixie) | 3.46.1 |
| Ubuntu 26.04 (resolute, LTS) | 3.46.1 |
| Ubuntu 26.10 (not LTS) | 3.53.4 |

Sources: `sources.debian.org/api/src/sqlite3/`, Launchpad `getPublishedSources` for `sqlite3` (Release pocket),
series names mapped to numbers through Launchpad's series records.
Android maps API levels to SQLite versions in its own reference (API 30: 3.28; API 35: 3.44), with the caveat that
device makers ship their own builds. Runtimes that bundle SQLite (Node's better-sqlite3, the Rails `sqlite3` gem's
native builds, Python's official installers) carry their own version regardless of the OS.

**Cloudflare D1 does not publish its production SQLite version.** Its full documentation (the `llms-full.txt` export,
622 KB) mentions a SQLite version once, historically (3.24.0, `legacy_alter_table`). It has to be read from the live
database (`select sqlite_version()`), and it can change without notice.

## Features by version (SQLite change log, `sqlite.org/changes.html`, checked entry by entry)

| Feature | Since | Note |
|---|---|---|
| UPSERT | 3.24.0 | |
| `RENAME COLUMN`, window functions | 3.25.0 | |
| generated columns | 3.31.0 | |
| `iif()` | 3.32.0 | |
| `DROP COLUMN` | 3.35.0 | defects that could corrupt the file fixed in 3.35.5: the safe floor is 3.35.5 |
| `RETURNING` | 3.35.0 | (one web source says 3.35.2; the change log says 3.35.0) |
| `STRICT` tables | 3.37.0 | |
| `->`, `->>`, `unixepoch()` | 3.38.0 | |
| `RIGHT` / `FULL OUTER JOIN` | 3.39.0 | |
| `concat()`, `string_agg()` | 3.44.0 | |
| JSONB | 3.45.0 | |
| `json_pretty()` | 3.46.0 | |
| `ALTER TABLE` adding or removing `NOT NULL` and `CHECK` | 3.53.0 | newer than every LTS server above, Ubuntu 26.04 included |

Version alone does not decide everything: JSON functions before 3.38.0 existed only when compiled with
`SQLITE_ENABLE_JSON1`, and the math functions (3.35.0) still need a compile option. A version-only rule on
`json_extract()` would raise false alarms; syntax (`->>`, `DROP COLUMN`, `RETURNING`, `STRICT`) is reliable.

## Prior art

- **strong_migrations** (Rails): `StrongMigrations.target_version = 16` "if your development database version is
  different from production"; honoured only in development and test, where the real server version is otherwise used.
- **Squawk** (Postgres linter): `pg_version = "11.0"` in `.squawk.toml`, or `--pg-version`.
- No tool found that checks SQLite migrations against a target version.

Both select which checks apply by the production version; neither replays migrations on that version.

## Options

- **A. Declared version plus a syntax scan.** `target_version` in `migate.toml`; flag SQL-file migrations using
  syntax newer than it, with the existing SQLite tokenizer and the table above. Cheap. Blind to migrators that generate
  SQL at runtime (Django, Rails, Payload) and to behaviour changes with no syntax.
- **B. Say what the sandbox cannot vouch for.** Compare the sandbox's version (the SQLite file header, as MG805 does;
  the server version for Postgres) with `target_version`, and warn that features in between were not checked.
- **C. Run on the exact version.**
  - Postgres: nearly free. With `target_version = 15`, start the installed 15 instead of the newest; if it is not
    installed, B's warning.
  - SQLite: replay SQL-file migrations through a `sqlite3` shell of that version against a copy of the base database.
    migate should not download or compile one; the project points at one (`sqlite_bin`), most simply by running CI on
    the production OS image, whose `apt` `sqlite3` is the production version exactly. Not measured here: fetching and
    compiling an old SQLite needs the owner's approval.

## Recommendation

1. `target_version` under `[sandbox]` (strong_migrations' name), per engine.
2. Postgres: C, falling back to B when that major is not installed.
3. SQLite: A for syntax features plus B always; C when the project supplies `sqlite_bin`.
4. Document for D1 how to read the production version, and that it can change.

Open: whether a feature newer than `target_version` is an error or a warning (it fails the deploy, so an error is
consistent with MG803), and whether migrators that generate SQL at runtime should get a capture step (Postgres can log
every statement on the throwaway server; SQLite cannot).
