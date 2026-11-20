//! The ledger: which migrations production ran, and the hashes they had (P1, "frozen").
//!
//! Rules:
//! - MG001 an applied migration was edited (code changed);
//! - MG002 an applied migration was removed;
//! - MG003 a ledger entry was rewritten or removed compared with the base branch (the ledger is append-only, which closes
//!   the bypass where one pull request edits a migration and rewrites its hash);
//! - MG007 an applied migration's comments or layout changed but its code did not: an error where the migrator checksums
//!   the whole file (Flyway fails `validate` in production), a note elsewhere.

use std::collections::BTreeMap;

use crate::finding::{Finding, Severity};
use crate::hash::{Lang, digests, raw_sha256};

/// One applied migration as production ran it. An empty `code_sha256` means only the whole file's raw hash is known
/// (a project's own `{name: sha256}` record): any edit then counts as code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub sha256: String,
    pub code_sha256: String,
}

/// Migration id -> its entry.
pub type Ledger = BTreeMap<String, Entry>;

/// A migration file in the repository.
#[derive(Debug, Clone)]
pub struct MigrationFile {
    pub id: String,
    /// Path relative to the project, used to pick the comment syntax.
    pub path: String,
    pub bytes: Vec<u8>,
}

impl MigrationFile {
    /// The entry a project's own record would hold: the raw bytes' hash only.
    pub fn raw_entry(&self) -> Entry {
        Entry {
            sha256: raw_sha256(&self.bytes),
            code_sha256: String::new(),
        }
    }

    /// The entry this file would get if production ran it now.
    pub fn entry(&self) -> Entry {
        let d = digests(&self.bytes, Lang::from_path(&self.path));
        Entry {
            sha256: d.sha256,
            code_sha256: d.code_sha256,
        }
    }
}

/// What the migrator itself hashes, which decides how serious a comment-only edit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigratorChecksum {
    /// The migrator hashes every byte and refuses to run when it changed (Flyway, sqlx, db-migrate).
    WholeFile,
    /// The migrator does not refuse to run on an edit (Payload, Rails, Django, Wrangler; Prisma only warns).
    None,
}

/// Check the files and the ledger of a pull request (`head`) against the ledger on the base branch (`base`).
pub fn check(
    base: &Ledger,
    head: &Ledger,
    files: &[MigrationFile],
    checksum: MigratorChecksum,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (id, was) in base {
        match head.get(id) {
            None => findings.push(Finding::new(
                "MG003",
                Severity::Error,
                id,
                "removed from the ledger, but production ran it: ledger entries are never removed",
            )),
            Some(now) if now != was => findings.push(Finding::new(
                "MG003",
                Severity::Error,
                id,
                "its ledger hash was rewritten, but production ran the original: add a new migration instead",
            )),
            Some(_) => {}
        }
    }

    // Every migration production ran, as the base recorded it where it did (a rewritten head entry must not hide an edit).
    let applied = head
        .iter()
        .chain(base.iter())
        .fold(BTreeMap::new(), |mut m, (id, e)| {
            m.insert(id.as_str(), e);
            m
        });

    for (id, expected) in applied {
        let Some(file) = files.iter().find(|f| f.id == id) else {
            findings.push(Finding::new(
                "MG002",
                Severity::Error,
                id,
                "production ran this migration, but its file is gone: restore it",
            ));
            continue;
        };
        let whole_file_only = expected.code_sha256.is_empty();
        let actual = if whole_file_only {
            file.raw_entry()
        } else {
            file.entry()
        };
        if actual.sha256 == expected.sha256 {
            continue;
        }
        if whole_file_only {
            findings.push(Finding::new(
                "MG001",
                Severity::Error,
                id,
                "edited after production ran it: production keeps the old version, so add a new migration instead (the \
                 record holds the whole file's hash only, so a comment-only edit counts too)",
            ));
            continue;
        }
        if actual.code_sha256 == expected.code_sha256 {
            let (severity, why) = match checksum {
                MigratorChecksum::WholeFile => (
                    Severity::Error,
                    "the migrator checksums the whole file, so production will refuse to migrate",
                ),
                MigratorChecksum::None => (
                    Severity::Note,
                    "the migrator does not check this, so it is harmless",
                ),
            };
            findings.push(Finding::new(
                "MG007",
                severity,
                id,
                format!(
                    "only comments or layout changed in a migration production already ran; {why}"
                ),
            ));
        } else {
            findings.push(Finding::new(
                "MG001",
                Severity::Error,
                id,
                "edited after production ran it: production keeps the old version, so add a new migration instead",
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, text: &str) -> MigrationFile {
        MigrationFile {
            id: id.into(),
            path: format!("migrations/{id}.sql"),
            bytes: text.as_bytes().to_vec(),
        }
    }

    fn ledger(files: &[&MigrationFile]) -> Ledger {
        files.iter().map(|f| (f.id.clone(), f.entry())).collect()
    }

    fn rules(findings: &[Finding]) -> Vec<(&'static str, Severity)> {
        findings.iter().map(|f| (f.rule, f.severity)).collect()
    }

    const ONE: &str = "CREATE TABLE a (id int);\n";
    const TWO: &str = "ALTER TABLE a ADD b int;\n";

    #[test]
    fn untouched_migrations_pass() {
        let (a, b) = (file("0001", ONE), file("0002", TWO));
        let l = ledger(&[&a, &b]);
        assert_eq!(check(&l, &l, &[a, b], MigratorChecksum::None), vec![]);
    }

    #[test]
    fn a_new_unapplied_migration_passes() {
        let (a, b) = (file("0001", ONE), file("0002", TWO));
        let l = ledger(&[&a]);
        assert_eq!(check(&l, &l, &[a, b], MigratorChecksum::None), vec![]);
    }

    #[test]
    fn appending_to_the_ledger_passes() {
        let (a, b) = (file("0001", ONE), file("0002", TWO));
        assert_eq!(
            check(
                &ledger(&[&a]),
                &ledger(&[&a, &b]),
                &[a, b],
                MigratorChecksum::None
            ),
            vec![]
        );
    }

    #[test]
    fn mg001_an_edited_migration_fails() {
        let a = file("0001", ONE);
        let l = ledger(&[&a]);
        let edited = file("0001", "CREATE TABLE a (id bigint);\n");
        assert_eq!(
            rules(&check(&l, &l, &[edited], MigratorChecksum::None)),
            vec![("MG001", Severity::Error)]
        );
    }

    #[test]
    fn mg001_line_endings_alone_are_not_an_edit() {
        let a = file("0001", ONE);
        let l = ledger(&[&a]);
        let crlf = file("0001", "CREATE TABLE a (id int);\r\n");
        assert_eq!(check(&l, &l, &[crlf], MigratorChecksum::WholeFile), vec![]);
    }

    #[test]
    fn mg002_a_removed_migration_fails() {
        let a = file("0001", ONE);
        let l = ledger(&[&a]);
        assert_eq!(
            rules(&check(&l, &l, &[], MigratorChecksum::None)),
            vec![("MG002", Severity::Error)]
        );
    }

    #[test]
    fn mg003_a_removed_ledger_entry_fails() {
        let a = file("0001", ONE);
        let base = ledger(&[&a]);
        assert_eq!(
            rules(&check(&base, &Ledger::new(), &[a], MigratorChecksum::None)),
            vec![("MG003", Severity::Error)]
        );
    }

    #[test]
    fn mg003_the_rehash_bypass_fails() {
        // One pull request edits an applied migration AND rewrites its ledger hash to match: both must be caught.
        let a = file("0001", ONE);
        let base = ledger(&[&a]);
        let edited = file("0001", "DROP TABLE a;\n");
        let head = ledger(&[&edited]);
        assert_eq!(
            rules(&check(&base, &head, &[edited], MigratorChecksum::None)),
            vec![("MG003", Severity::Error), ("MG001", Severity::Error)]
        );
    }

    #[test]
    fn mg007_a_comment_only_edit_is_a_note_where_the_migrator_does_not_check() {
        let a = file("0001", ONE);
        let l = ledger(&[&a]);
        let commented = file("0001", "-- explain why\nCREATE TABLE a (id int);\n");
        assert_eq!(
            rules(&check(&l, &l, &[commented], MigratorChecksum::None)),
            vec![("MG007", Severity::Note)]
        );
    }

    #[test]
    fn mg007_a_comment_only_edit_is_an_error_where_the_migrator_checksums_the_file() {
        let a = file("0001", ONE);
        let l = ledger(&[&a]);
        let commented = file("0001", "-- explain why\nCREATE TABLE a (id int);\n");
        assert_eq!(
            rules(&check(&l, &l, &[commented], MigratorChecksum::WholeFile)),
            vec![("MG007", Severity::Error)]
        );
    }
}
