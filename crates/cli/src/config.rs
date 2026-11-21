//! `migate.toml`: the project's settings, so the flags need not be typed every time. Flags win over the file.
//!
//! ```toml
//! migrator = "payload"
//! ledger = "db/migate-ledger.json"            # relative to the project directory
//! base = "origin/main"
//! out_of_order = "block"                    # optional: the project's runner refuses migrations older than the last
//!
//! # Optional: migrate a throwaway SQLite database at base and at head, diff the schemas, run the base version's tests.
//! [sandbox]
//! engine = "sqlite"                            # or "postgres": a local server, {url} is its connection URL
//! env = { DATABASE_URL = "sqlite:///{db}" }   # how the project is told where the database is; {db} is its path
//! database_name = "app"                        # Wrangler D1: the database's name in wrangler.toml
//! shared = "apps/web/.wrangler"                # a database the project keeps inside its own checkout
//! setup = "pnpm install --frozen-lockfile"     # run in each checkout first (a fresh checkout has no dependencies)
//! seed = "pnpm run seed"                       # optional: rows for the data-loss check
//! test = "pnpm test"                           # the base version's tests, run against the migrated database
//! readback = "pnpm run readback"               # optional: the base version reads every row the head version wrote
//! target_version = "3.37.2"                     # optional: production's engine version (SQLite 3.37.2, PostgreSQL 15)
//! keep_output = "migate-output"                 # optional: every command's full output, one file per step
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub migrator: Option<String>,
    pub ledger: Option<String>,
    /// How the ledger file stores entries: "migate" (the default) or "sha256", a project's own `{name: sha256 of the
    /// file}` record read as it is (any edit is then MG001: there is no code-only hash).
    pub ledger_format: Option<String>,
    pub base: Option<String>,
    /// Overrides the migrator's out-of-order policy (block, warn or allow) when the project's own runner differs.
    pub out_of_order: Option<String>,
    pub sandbox: Option<Sandbox>,
}

/// The database engine the throwaway database runs on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Sqlite,
    /// A local PostgreSQL server started for the run; `{url}` is its connection URL.
    Postgres,
}

/// `[sandbox]`. In commands and values, `{db}` is the throwaway SQLite database's path, `{url}` the throwaway Postgres
/// server's connection URL, and `{dir}` the run's folder.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sandbox {
    #[serde(default)]
    pub engine: Engine,
    /// Variables for every command, on top of the migrator's own.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Names of variables to pass through from migate's own environment. Nothing else is passed, except the
    /// toolchain's (PATH, HOME, language managers): an inherited connection URL could point at production.
    #[serde(default)]
    pub pass_env: Vec<String>,
    /// Where the migrate command writes the database, when the tool picks the file name itself; a `*` in the file name
    /// matches one file. Default: `{db}`.
    pub database: Option<String>,
    /// Replaces the migrator's migrate command.
    pub migrate: Option<String>,
    /// A folder inside the project where the migrate command keeps its database when it cannot be told where (a local
    /// D1 under `.wrangler`, say). Both checkouts get it as a link to `{dir}/shared`, so base and head migrate one
    /// database; name the file with `database = "{dir}/shared/..."`.
    pub shared: Option<String>,
    /// The database's name in the project's own configuration, for migrate commands that need one (`{database}`;
    /// Wrangler D1's database name or binding).
    pub database_name: Option<String>,
    pub setup: Option<String>,
    /// Run in the base checkout after the base migrations, to put rows in the database; with the rows the base tests
    /// leave behind, they show data a migration loses.
    pub seed: Option<String>,
    /// The base version's tests. Run three times: on the database at base (they must pass, or their verdict means
    /// nothing), at head (the check), and after a pass, on the same throwaway database with every table but the migration
    /// history dropped (they must fail, or they never used the database).
    pub test: Option<String>,
    /// Rollback compatibility: run in the base checkout after the head version's tests wrote rows to the migrated
    /// database, it reads every row with the base version's code (Django's `full_clean()` on each, a Rails runner
    /// loading each model) and fails on one it cannot read (MG812).
    pub readback: Option<String>,
    /// The engine version production runs (`3.37.2` for SQLite, `15` for PostgreSQL), when it differs from the
    /// sandbox's: syntax production lacks is an error (MG701), and PostgreSQL runs on that major when installed.
    pub target_version: Option<String>,
    /// A folder (relative to the repository root) to keep every command's full output in, one file per step, since
    /// the run's own folder is removed when it ends. For CI, upload it as an artifact.
    pub keep_output: Option<String>,
    /// A project's own contract marker word (`CONTRACT` for `// CONTRACT: <why>`). A migration of the change holding
    /// such a line, with a reason, is an intended contract step: its findings that point at a known line are accepted,
    /// as a `migate: contract` marker would accept each. The rules no marker can accept stay.
    pub contract_marker: Option<String>,
}

/// `explicit` is `--config`; without it, `<root>/migate.toml` is read if present.
pub fn load(root: &Path, explicit: Option<&Path>) -> Result<Config, String> {
    let (path, required) = match explicit {
        Some(p) => (p.to_path_buf(), true),
        None => (root.join("migate.toml"), false),
    };
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == ErrorKind::NotFound && !required => Ok(Config::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
