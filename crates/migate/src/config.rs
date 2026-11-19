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
//! env = { DATABASE_URL = "sqlite:///{db}" }   # how the project is told where the database is; {db} is its path
//! database_name = "app"                        # Wrangler D1: the database's name in wrangler.toml
//! shared = "apps/web/.wrangler"                # a database the project keeps inside its own checkout
//! setup = "npm ci"                             # run in each checkout first (a fresh checkout has no dependencies)
//! seed = "npm run seed"                        # optional: rows for the data-loss check
//! test = "npm test"                            # the base version's tests, run against the migrated database
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
    pub base: Option<String>,
    /// Overrides the migrator's out-of-order policy (block, warn or allow) when the project's own runner differs.
    pub out_of_order: Option<String>,
    pub sandbox: Option<Sandbox>,
}

/// `[sandbox]`. In commands and values, `{db}` is the throwaway database's path and `{dir}` the folder holding it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sandbox {
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
