//! `migate.toml`: the project's settings, so the flags need not be typed every time. Flags win over the file.
//!
//! ```toml
//! migrator = "payload"
//! ledger = "db/migate-ledger.json"            # relative to the project directory
//! base = "origin/main"
//! ```

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
