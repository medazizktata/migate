//! The rule catalogue: permanent IDs, a short name and a description for every rule the core enforces today
//! (`docs/research/architecture.md` §6). IDs are never reused or renumbered.

/// One rule's documentation, used by reporters (SARIF needs it for every rule it mentions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        id: "MG001",
        name: "applied-migration-edited",
        description: "A migration production already ran was edited. Production keeps the old version; add a new migration instead.",
    },
    Rule {
        id: "MG002",
        name: "applied-migration-removed",
        description: "A migration production already ran was removed from the repository. Restore its file.",
    },
    Rule {
        id: "MG003",
        name: "ledger-rewritten",
        description: "A ledger entry was rewritten or removed compared with the base branch. The ledger is append-only.",
    },
    Rule {
        id: "MG004",
        name: "migration-out-of-order",
        description: "A pending migration sorts before the last one production ran. Depending on the migration tool it runs out of order or is silently skipped forever.",
    },
    Rule {
        id: "MG005",
        name: "migration-graph-inconsistent",
        description: "The migration dependency graph has several heads or a missing parent.",
    },
    Rule {
        id: "MG006",
        name: "duplicate-migration-version",
        description: "Several migrations share one version; some tools treat the second as already applied.",
    },
    Rule {
        id: "MG007",
        name: "applied-migration-comment-edit",
        description: "Only comments or layout changed in a migration production already ran. Harmless unless the migration tool checksums the whole file.",
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
        }
    }

    #[test]
    fn every_rule_the_core_emits_is_catalogued() {
        // Grep-free: the rules the ledger and order modules emit are listed here; a new one must be added.
        for id in [
            "MG001", "MG002", "MG003", "MG004", "MG005", "MG006", "MG007",
        ] {
            assert!(rule(id).is_some(), "{id} missing from the catalogue");
        }
        assert!(rule("MG999").is_none());
    }
}
