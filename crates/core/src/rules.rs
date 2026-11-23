//! The rule catalogue: permanent IDs, a short name, a description and the risk axes of every rule the core enforces
//! today (`docs/research/architecture.md` §6). IDs are never reused or renumbered.

use crate::Severity;

/// One rule's documentation, used by reporters (SARIF needs it for every rule it mentions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// What the change risks, as letters: `B` breaks the running (old) app, `L` locks, `D` loses data, `F` may fail on
    /// rows production already has. Empty for rules about migration files and the ledger, which decide their own level.
    pub axes: &'static str,
}

impl Rule {
    /// The level a schema rule reports at (§6.2): an error if it breaks the old app or loses data, a warning if it only
    /// locks or may fail on existing rows (the sandbox is empty, so it cannot know), otherwise a note.
    pub fn severity(&self) -> Severity {
        if self.axes.contains(['B', 'D']) {
            Severity::Error
        } else if self.axes.contains(['L', 'F']) {
            Severity::Warning
        } else {
            Severity::Note
        }
    }
}

pub const RULES: &[Rule] = &[
    Rule {
        id: "MG001",
        name: "applied-migration-edited",
        description: "A migration production already ran was edited. Production keeps the old version; add a new migration instead.",
        axes: "",
    },
    Rule {
        id: "MG002",
        name: "applied-migration-removed",
        description: "A migration production already ran was removed from the repository. Restore its file.",
        axes: "",
    },
    Rule {
        id: "MG003",
        name: "ledger-rewritten",
        description: "A ledger entry was rewritten or removed compared with the base branch. The ledger is append-only.",
        axes: "",
    },
    Rule {
        id: "MG004",
        name: "migration-out-of-order",
        description: "A pending migration sorts before the last one production ran. Depending on the migration tool it runs out of order or is silently skipped forever.",
        axes: "",
    },
    Rule {
        id: "MG005",
        name: "migration-graph-inconsistent",
        description: "The migration dependency graph has several heads or a missing parent.",
        axes: "",
    },
    Rule {
        id: "MG006",
        name: "duplicate-migration-version",
        description: "Several migrations share one version; some tools treat the second as already applied.",
        axes: "",
    },
    Rule {
        id: "MG007",
        name: "applied-migration-comment-edit",
        description: "Only comments or layout changed in a migration production already ran. Harmless unless the migration tool checksums the whole file.",
        axes: "",
    },
    Rule {
        id: "MG010",
        name: "applied-migration-missing-from-repo",
        description: "Production's history records a migration the repository has no file for: it was applied from another branch, or its file was deleted. Found at release by `migate history`.",
        axes: "",
    },
    Rule {
        id: "MG011",
        name: "applied-outside-a-release",
        description: "Production ran a migration no release recorded in the ledger: someone migrated by hand, or a release forgot to record it. Found at release by `migate history`.",
        axes: "",
    },
    Rule {
        id: "MG012",
        name: "recorded-but-not-applied",
        description: "The ledger says production ran a migration its history does not hold: it was rolled back, or the ledger was written without it running. Found at release by `migate history`.",
        axes: "",
    },
    Rule {
        id: "MG101",
        name: "drop-table",
        description: "A table was dropped. The running app still reads and writes it, and its rows are gone.",
        axes: "BD",
    },
    Rule {
        id: "MG104",
        name: "rebuild-cascades",
        description: "The migration rebuilt a table (dropped it and renamed a copy into its place) that other tables reference with ON DELETE CASCADE or SET NULL. Wherever foreign keys are enforced during the migration, as Cloudflare D1 always does, dropping the original deletes or blanks every child row. Seen on any database, rows or not, when the copy is renamed into place; a rebuild that drops the table and creates it again under its own name (as Rails does) leaves no trace without rows, and only MG105 on a seeded database catches it. Tools that rebuild with foreign keys really off (Django) are exempt in their migrator file.",
        axes: "D",
    },
    Rule {
        id: "MG105",
        name: "rows-lost",
        description: "A table has fewer rows after the migration than before, on a database holding the rows the base version's tests and seed wrote. A rebuild of a parent table with ON DELETE CASCADE children deletes the children this way.",
        axes: "D",
    },
    Rule {
        id: "MG106",
        name: "values-lost",
        description: "A column holds fewer values after the migration than before, though no row was lost: an UPDATE that blanks it, or a parent rebuild whose ON DELETE SET NULL children lose their reference.",
        axes: "D",
    },
    Rule {
        id: "MG107",
        name: "delete-all-rows",
        description: "A migration deletes every row of a table: a DELETE with no WHERE, or a TRUNCATE. Read from the migration's text, so it is seen even when the sandbox's database holds no rows there (MG105 counts rows, and an empty table loses none). A table the same migration creates is exempt. Accept a deliberate emptying with a marker naming why.",
        axes: "D",
    },
    Rule {
        id: "MG201",
        name: "drop-column",
        description: "A column was dropped. The running app still selects or inserts it, and its values are gone. Stop using it in one release, drop it in the next.",
        axes: "BLD",
    },
    Rule {
        id: "MG212",
        name: "dropped-column-still-used",
        description: "A dropped (or renamed) column the version still serving traffic uses, by its ORM's own rule (a model field, a schema entry, a raw query naming it; Rails: any column not in ignored_columns). No contract marker can accept it: stop using the column in one release, drop it in the next.",
        axes: "BLD",
    },
    Rule {
        id: "MG203",
        name: "add-required-column-without-default",
        description: "A NOT NULL column without a default was added. Inserts from the running app, which does not know the column, fail; on a table with rows the migration itself can fail.",
        axes: "BF",
    },
    Rule {
        id: "MG204",
        name: "set-not-null",
        description: "A column became NOT NULL. Writes of NULL from the running app fail, and the migration fails if production has NULLs.",
        axes: "BLF",
    },
    Rule {
        id: "MG205",
        name: "narrow-type",
        description: "A column's type or size narrowed. Values production holds may not fit (the migration fails or truncates), and the running app may write values that no longer fit.",
        axes: "BLDF",
    },
    Rule {
        id: "MG206",
        name: "change-type",
        description: "A column's type or collation changed without narrowing. It can rewrite the table.",
        axes: "L",
    },
    Rule {
        id: "MG207",
        name: "drop-default",
        description: "A column lost its default. Inserts from the running app that omit the column now fail or write NULL.",
        axes: "B",
    },
    Rule {
        id: "MG208",
        name: "change-generated-or-identity",
        description: "A generated or identity column was added or changed. It can rewrite the table.",
        axes: "L",
    },
    Rule {
        id: "MG209",
        name: "change-default",
        description: "A column's default changed. The running app, and anything with a cached schema, may rely on the old value.",
        axes: "B",
    },
    Rule {
        id: "MG210",
        name: "add-column",
        description: "A column was added. Usually safe; the mixed-version test decides whether the running app copes.",
        axes: "",
    },
    Rule {
        id: "MG301",
        name: "add-check",
        description: "A CHECK constraint was added or changed. Writes from the running app may violate it, and existing rows may already.",
        axes: "BLF",
    },
    Rule {
        id: "MG302",
        name: "add-unique",
        description: "A UNIQUE constraint or unique index was added or changed. Writes from the running app may collide, and existing rows may already.",
        axes: "BLF",
    },
    Rule {
        id: "MG303",
        name: "add-foreign-key",
        description: "A foreign key was added or changed. Writes from the running app may break it, and existing rows may already.",
        axes: "BLF",
    },
    Rule {
        id: "MG304",
        name: "drop-constraint",
        description: "A constraint was dropped. Not a breaking change, but data it used to guard can now go wrong.",
        axes: "",
    },
    Rule {
        id: "MG305",
        name: "change-primary-key",
        description: "A primary key was added or changed. Existing rows may break it, and the running app may rely on the old key.",
        axes: "BLF",
    },
    Rule {
        id: "MG402",
        name: "drop-index",
        description: "An index was dropped or changed. Queries that used it may slow down.",
        axes: "",
    },
    Rule {
        id: "MG501",
        name: "drop-view",
        description: "A view was dropped. The running app may still query it.",
        axes: "B",
    },
    Rule {
        id: "MG502",
        name: "change-view",
        description: "A view's definition changed. The running app may rely on its old columns or rows.",
        axes: "B",
    },
    Rule {
        id: "MG503",
        name: "add-trigger",
        description: "A trigger was added. It can reject or change writes the running app makes.",
        axes: "B",
    },
    Rule {
        id: "MG504",
        name: "drop-or-change-trigger",
        description: "A trigger was dropped or changed. The running app may rely on what it did.",
        axes: "B",
    },
    Rule {
        id: "MG505",
        name: "drop-or-change-routine",
        description: "A routine was dropped or its arguments changed. The running app may still call the old signature. A routine dropped and created again with every old argument kept in place, the new ones defaulted and the same result type is a note: the old calls still resolve. What the result holds is not compared, so a JSON result that changes shape passes.",
        axes: "B",
    },
    Rule {
        id: "MG601",
        name: "remove-enum-value",
        description: "An enum value was removed or renamed. The running app may still write it, and rows that hold it no longer fit.",
        axes: "BD",
    },
    Rule {
        id: "MG603",
        name: "drop-sequence",
        description: "A sequence was dropped. The running app may still take values from it.",
        axes: "B",
    },
    Rule {
        id: "MG701",
        name: "newer-than-production",
        description: "A migration uses syntax the engine version production runs (target_version) does not have. It runs in the sandbox and fails at deploy.",
        axes: "F",
    },
    Rule {
        id: "MG801",
        name: "not-checked",
        description: "A check could not judge part of the change, and says so instead of passing it: the migration changed an object the schema model does not cover (a policy, a domain, a grant, an exclusion constraint and the like), so only the base version's tests judged it; or, with target_version set, a migration that is not a SQL file could not be read for syntax newer than production's engine (MG701).",
        axes: "",
    },
    Rule {
        id: "MG805",
        name: "newer-writer",
        description: "The migrations ran with a newer SQLite than the one migate reads the database with. A feature newer than migate's SQLite may be misread or unreadable; update migate.",
        axes: "",
    },
    Rule {
        id: "MG806",
        name: "sandbox-version-differs",
        description: "The migrations ran on a different engine version than production's (target_version): what differs between the two versions was not checked by running it.",
        axes: "",
    },
    Rule {
        id: "MG803",
        name: "head-migration-failed",
        description: "The pull request's migrations fail on a database migrated to the base branch. Deploying fails, and a tool without transactions leaves the changes made so far behind.",
        axes: "D",
    },
    Rule {
        id: "MG804",
        name: "sandbox-not-migrated",
        description: "The base migrations did not write to the throwaway database, so nothing was checked. Usually the configured variable is not the one the project reads.",
        axes: "",
    },
    Rule {
        id: "MG812",
        name: "rollback-incompatible-rows",
        description: "The base version cannot read rows the head version wrote: after a rollback, or while both versions serve traffic, the old code meets them (a new enum value, a changed format). Checked by the project's `readback` command after the head version's tests wrote rows; a note when they wrote none.",
        axes: "",
    },
    Rule {
        id: "MG813",
        name: "old-version-run-invalid",
        description: "The base version's test run proves nothing: it fails on its own database, cannot start, rebuilt the database, or passes with the tables gone. Nothing was checked.",
        axes: "",
    },
    Rule {
        id: "MG810",
        name: "old-version-tests-fail",
        description: "The base version's tests fail against the migrated database. The version still running in production would break when this deploys.",
        axes: "B",
    },
    Rule {
        id: "MG901",
        name: "bad-contract-marker",
        description: "A contract marker has no reason, names a rule that cannot be accepted, or matches no finding. Markers accept one intended change; a stale one would hide the next.",
        axes: "",
    },
];

/// The catalogue entry for a rule ID.
pub fn rule(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_well_formed() {
        let mut ids: Vec<&str> = RULES.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), RULES.len(), "duplicate rule id");
        for r in RULES {
            assert!(
                r.id.len() == 5
                    && r.id.starts_with("MG")
                    && r.id[2..].chars().all(|c| c.is_ascii_digit())
            );
            assert!(!r.name.is_empty() && !r.description.is_empty());
            assert!(r.axes.chars().all(|c| "BLDF".contains(c)), "{}", r.id);
        }
    }

    #[test]
    fn every_rule_the_core_emits_is_catalogued() {
        // The rules the ledger and order modules emit are listed here; the classifier lists its own.
        for id in [
            "MG001", "MG002", "MG003", "MG004", "MG005", "MG006", "MG007",
        ] {
            assert!(rule(id).is_some(), "{id} missing from the catalogue");
        }
        for id in crate::diff::EMITTED {
            assert!(rule(id).is_some(), "{id} missing from the catalogue");
        }
        assert!(rule("MG999").is_none());
    }
}
