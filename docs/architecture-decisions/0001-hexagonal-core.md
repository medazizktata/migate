# 0001: A functional core with ports at the edges, added only when needed

Status: accepted, 2026-11-19. Applies to every crate in this repository.

## Context

migate has to work with any database, any host and any migration tool (`docs/product.md`). Every rule it enforces must
be shown failing on a known-bad case and passing on a known-good one, cheaply and often. Database engines, hosts, git,
the file system and the teams' own test commands are slow, varied and hard to fake. `docs/research/architecture.md`
(sections 0 and 3) sketches the interfaces between them.

## Decision

1. **A functional core.** `migate-core` holds the rules: the schema model, diff and classifier (to come), the ledger,
   the order models. Its functions take data and return findings. It does no I/O at all: no file system, processes,
   network, environment, clock or git.
2. **Ports and adapters at the edges.** Everything that touches the world (files, git, databases, sandboxes, hosts, test
   runners, reporters) lives in other crates, behind a trait (a port) when, and only when, there are two real
   implementations or a test needs a stand-in.
3. **Migration tools are data, not adapters.** Each is a TOML file in `migate-migrators`; one interpreter reads them all. So is
   what migate knows about the other tools a project's commands run (variables to pass or set, folders to skip, a
   package manager's noise lines, a host's internal tables): `migate-migrators/tools.toml`. The runner reads it and
   names no tool; the only tool-specific code is an engine finding its own server programs.
   A new migration tool should be a new file; code changes only for a new order model or file layout.
4. **The CLI is the composition root.** `migate` reads configuration, picks the adapters and wires them to the core.
   No other crate decides which adapter to use.
5. **No skeleton up front.** The ports are not written before the task that first needs them (see "Why" below).

## Crate map

| Crate | Role | Exists |
|---|---|---|
| `migate-core` | the rules; pure functions; no I/O | yes |
| `migate-migrators` | migration-tool definitions (TOML) and their interpreter | yes |
| `migate` | CLI, composition root | yes: `check`, `ledger record` |
| (in `migate`: `project.rs`, `git.rs`, `ledger_file.rs`) | project files, the git base lookup, the ledger file | done in TASK-37; a separate crate when a second consumer needs it |
| (in `migate-core`: `engine.rs`) | the engine port: `Sandbox` (placeholders, open the migrated database) and `Database` (snapshot, row counts, drop all but the history); declarations only | yes, with the second engine |
| `migate-engine-sqlite` | SQLite (and D1's local files) schema reader and a temp-file sandbox, behind the engine port | done in TASK-16 |
| `migate-engine-postgres` | PostgreSQL: a throwaway local server (initdb, `postgres` as a child) and a catalog reader through `psql` JSON, behind the engine port | done in TASK-17 |
| later MySQL and SQL Server | schema reader, transport, sandbox per engine, each implementing the engine port | TASK-27, TASK-28 |
| (in `migate`: `sandbox.rs`) | the two-checkout throwaway-database run: migrate at base and HEAD, diff, the base version's tests (fresh mode) | SQLite; a `migate-runner` crate when the pre-boot mode or a second engine needs it (TASK-32) |
| `migate-host-*` | production history, read-only checks, restore points | with TASK-21, TASK-22 |
| `migate-report` | text, JSON, SARIF, PR comment | with TASK-24 |

Adapters depend on `migate-core`; `migate-core` depends on no migate crate. The CLI depends on all of them.

## The dependency rule, and how it is enforced

`migate-core`'s dependency tree may contain only crates on an explicit allowlist in `scripts/check-core-deps.sh` (today
`sha2` and its tree). The script runs in `scripts/check.sh`, the pre-commit hook's full counterpart, and in CI. It was
shown failing with `rusqlite` and with `tokio` added to the core (commit 4eae4c6). Adding a crate to the core means
adding it to the allowlist in the same change, on purpose, and saying why in the commit.

The workspace lints add `unsafe_code = "forbid"` and refuse `unwrap()` outside tests.

## Which ports come when

| Port | Arrives with | Its two sides |
|---|---|---|
| project source: migration files, the ledger file, the base branch's copy | TASK-37 (`migate check`) | **no trait yet**: the core takes plain data, and the end-to-end tests build real temporary git repositories, so no double was needed. A trait comes when a second source does (for example reading a remote repository). |
| transport and schema reader | TASK-16 (SQLite) | SQLite; Postgres in TASK-17 |
| sandbox | TASK-16, TASK-17 | temp-file SQLite; cloned Postgres cluster; later containers |
| version runner | TASK-32 | fresh run; pre-boot run (Rails) |
| history and host | TASK-21, TASK-22 | a connection URL; Wrangler/D1; later Neon, RDS and others |
| reporter | TASK-24 | text, JSON, SARIF, PR comment |

Out-of-tree plug-ins (third-party engines or hosts) talk to migate as subprocesses speaking JSON
(`architecture.md` section 3); that protocol is itself an adapter behind the same ports, built when the first outside
plug-in is wanted.

## Why no skeleton up front

- **Interfaces designed before their first use are usually wrong.** The spikes already changed the sketches: migrators
  need environment extras and a database location, not just a URL (`spike-7.md`); the runner needs a pre-boot mode
  (`spike-31.md`). A skeleton written earlier would have encoded the wrong shapes.
- **Every port must be testable both ways when it lands.** A port without a real implementation and a double cannot be
  shown failing and passing, which this project requires of every check.
- **Unused traits cost reading time and refactors** and make the code look further along than it is.

The cost of waiting is small: the core's functions already take plain data, so a port is added by wrapping where that
data comes from, without changing the core.

## Consequences

- A contributor adding an engine, host or reporter adds a crate behind the existing port, or creates the port in the same
  change if it does not exist yet, with a real implementation and a test double.
- A contributor adding a migration tool adds a TOML file and fixture tests in `migate-migrators`.
- Anything that needs I/O inside `migate-core` is a design error; the dependency rule and review catch it.
- The crate names above are provisional until each crate is created.

## References

`docs/research/architecture.md` (sections 0, 3, 10); `docs/research/spike-7.md`; `docs/research/spike-31.md`;
`scripts/check-core-deps.sh`; the discussion of hexagonal design on 2026-11-19 (ports only where two real implementations
or a test double exist; migrators as data; a CI check keeping drivers out of the core).
