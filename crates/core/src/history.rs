//! Production's migration history against the repository and the ledger (P3, at release). Pure: the release job reads
//! what production recorded and passes it in; MG010-MG012 come out.

use crate::ledger::Ledger;
use crate::order::Migration;
use crate::{Finding, Severity};

/// Whether `value`, as a history table stores it, records `m`: by version (Rails), by full name (Payload, Prisma,
/// Wrangler) or as `group/version` (Django).
fn records(value: &str, m: &Migration) -> bool {
    value == m.version
        || value == m.id
        || (!m.group.is_empty() && value == format!("{}/{}", m.group, m.version))
}

/// Every difference between what production ran (`applied`, as its history table stores it), the repository's
/// migrations and the ledger the releases wrote.
pub fn compare(applied: &[String], repo: &[Migration], ledger: &Ledger) -> Vec<Finding> {
    // A graph migrator records third-party apps' migrations too (Django's auth, admin...); the project ships none of
    // their files, so only the groups it ships are compared.
    let groups: std::collections::BTreeSet<&str> = repo
        .iter()
        .map(|m| m.group.as_str())
        .filter(|g| !g.is_empty())
        .collect();
    let shipped = |value: &str| {
        groups.is_empty()
            || value
                .split_once('/')
                .is_none_or(|(group, _)| groups.contains(group))
    };
    let mut out = Vec::new();
    for value in applied.iter().filter(|v| shipped(v)) {
        match repo.iter().find(|m| records(value, m)) {
            None => out.push(Finding::new(
                "MG010",
                Severity::Error,
                value.as_str(),
                "production ran this migration, and the repository has no file for it: it came from another branch, \
                 or its file was deleted"
                    .to_string(),
            )),
            Some(m) if !ledger.contains_key(&m.id) => out.push(Finding::new(
                "MG011",
                Severity::Error,
                m.id.as_str(),
                "production ran this migration, but no release recorded it in the ledger: it ran outside a release"
                    .to_string(),
            )),
            Some(_) => {}
        }
    }
    for id in ledger.keys() {
        let ran = applied
            .iter()
            .any(|value| value == id || repo.iter().any(|m| &m.id == id && records(value, m)));
        if !ran {
            out.push(Finding::new(
                "MG012",
                Severity::Error,
                id.as_str(),
                "the ledger records this migration as run, but production has no record of it: it was rolled back, \
                 or the ledger was written without it running"
                    .to_string(),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Entry;

    fn ledger(ids: &[&str]) -> Ledger {
        ids.iter()
            .map(|id| {
                (
                    id.to_string(),
                    Entry {
                        sha256: String::new(),
                        code_sha256: String::new(),
                    },
                )
            })
            .collect()
    }

    fn rules(f: &[Finding]) -> Vec<(&str, &str)> {
        f.iter().map(|f| (f.rule, f.object.as_str())).collect()
    }

    #[test]
    fn each_difference_has_its_rule_and_agreement_has_none() {
        let repo = [
            Migration::linear("20260101_orders", "20260101"),
            Migration::linear("20260102_total", "20260102"),
        ];
        let both = ledger(&["20260101_orders", "20260102_total"]);
        let ran = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(compare(&ran(&["20260101", "20260102"]), &repo, &both).is_empty());
        assert_eq!(
            rules(&compare(
                &ran(&["20260101", "20260102", "20991231"]),
                &repo,
                &both
            )),
            [("MG010", "20991231")]
        );
        assert_eq!(
            rules(&compare(
                &ran(&["20260101", "20260102"]),
                &repo,
                &ledger(&["20260101_orders"])
            )),
            [("MG011", "20260102_total")]
        );
        assert_eq!(
            rules(&compare(&ran(&["20260101"]), &repo, &both)),
            [("MG012", "20260102_total")]
        );
    }

    #[test]
    fn django_history_names_app_and_migration() {
        let m = Migration {
            id: "shop.0001_initial".into(),
            version: "0001_initial".into(),
            group: "shop".into(),
            parents: Vec::new(),
        };
        let f = compare(
            &["shop/0001_initial".to_string()],
            std::slice::from_ref(&m),
            &ledger(&["shop.0001_initial"]),
        );
        assert!(f.is_empty(), "{f:?}");
        // Django's own apps (auth, admin...) record their migrations too; the project ships none of their files.
        let f = compare(
            &[
                "auth/0001_initial".to_string(),
                "shop/0001_initial".to_string(),
                "shop/0002_gone".to_string(),
            ],
            &[m],
            &ledger(&["shop.0001_initial"]),
        );
        assert_eq!(rules(&f), [("MG010", "shop/0002_gone")]);
    }
}
