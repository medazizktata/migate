//! The ledger file: JSON, migration id -> its two digests, sorted by id so diffs stay readable.
//!
//! ```json
//! { "20260101000000_create_orders": { "sha256": "…", "code_sha256": "…" } }
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use migate_core::ledger::{Entry, Ledger};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    sha256: String,
    code_sha256: String,
}

/// Parse ledger text; `origin` names where it came from, for messages.
pub fn parse(text: &str, origin: &str) -> Result<Ledger, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("{origin}: not valid JSON: {e}"))?;
    let old_format = value
        .as_object()
        .is_some_and(|map| map.values().any(serde_json::Value::is_string));
    if old_format {
        return Err(format!(
            "{origin}: entries are plain strings (the older {{name: hash}} format); migate needs \
             {{id: {{\"sha256\": …, \"code_sha256\": …}}}}: re-record them with `migate ledger record`"
        ));
    }
    let stored: BTreeMap<String, Stored> =
        serde_json::from_value(value).map_err(|e| format!("{origin}: not a migate ledger: {e}"))?;
    Ok(stored
        .into_iter()
        .map(|(id, s)| {
            (
                id,
                Entry {
                    sha256: s.sha256,
                    code_sha256: s.code_sha256,
                },
            )
        })
        .collect())
}

/// The ledger in the working tree; a missing file is an empty ledger (a project adopting migate).
pub fn read(path: &Path) -> Result<Ledger, String> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text, &path.display().to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Ledger::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

pub fn write(path: &Path, ledger: &Ledger) -> Result<(), String> {
    let stored: BTreeMap<&String, Stored> = ledger
        .iter()
        .map(|(id, e)| {
            (
                id,
                Stored {
                    sha256: e.sha256.clone(),
                    code_sha256: e.code_sha256.clone(),
                },
            )
        })
        .collect();
    let mut text = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    text.push('\n');
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
