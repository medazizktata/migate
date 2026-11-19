//! Migration order (P2). "Pending" means something different in each migrator, so the order model is chosen per
//! migrator from this closed set (`docs/research/first-pass.md` §2 studied 21 migrators).
//!
//! Rules:
//! - MG004 a pending migration sorts before the last one production ran. Severity per migrator: an error where the
//!   migrator would silently skip it forever (stable Drizzle, golang-migrate) or where the team blocks it; a warning
//!   where the migrator runs it anyway (Payload, Rails, Wrangler); nothing where out-of-order is the design (Drizzle v1).
//! - MG005 a dependency graph is inconsistent: several heads, or a parent that does not exist (Django, Alembic).
//! - MG006 two migrations share a version (Tuist 2026: the second was treated as already applied).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::finding::{Finding, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderModel {
    /// Plain string order.
    Lexical,
    /// The version is a number (`0001`, `12`); compared as an integer, so `10` comes after `9`.
    Numeric,
    /// The version is a timestamp; its digits are compared as a number (`20261118_232700` -> `20261118232700`).
    TimestampPrefix,
    /// Only the current version is recorded (golang-migrate); anything lower that is pending never runs.
    SingleVersion,
    /// A dependency graph (Django, Alembic): order comes from parents, not from names.
    Graph,
}

/// What to do with a pending migration that sorts before production's last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfOrder {
    Block,
    Warn,
    Allow,
}

/// A migration as the order check sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub id: String,
    pub version: String,
    /// The graph group (a Django app); empty for linear migrators.
    pub group: String,
    /// Graph parents as `group/version`; empty for linear migrators.
    pub parents: Vec<String>,
}

impl Migration {
    pub fn linear(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            group: String::new(),
            parents: Vec::new(),
        }
    }

    fn key(&self) -> String {
        if self.group.is_empty() {
            self.version.clone()
        } else {
            format!("{}/{}", self.group, self.version)
        }
    }
}

/// How the migrator orders and skips, from its migrator file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub model: OrderModel,
    pub out_of_order: OutOfOrder,
    /// The migrator never runs a migration older than its last applied one (stable Drizzle, golang-migrate).
    pub silent_skip: bool,
    /// What two migrations with one version do: `Block` where the migrator refuses or skips the second (Rails, Drizzle),
    /// `Warn` where it applies both by their full names (Wrangler D1, Payload, Prisma), their order then by name.
    pub duplicate_versions: OutOfOrder,
}

/// `repo`: every migration in the pull request. `applied`: the versions production ran (`group/version` for graphs).
pub fn check(policy: Policy, repo: &[Migration], applied: &[String]) -> Vec<Finding> {
    let mut findings = duplicates(policy.duplicate_versions, repo);
    match policy.model {
        OrderModel::Graph => findings.extend(graph(repo)),
        model => findings.extend(linear(policy, model, repo, applied)),
    }
    findings
}

fn duplicates(policy: OutOfOrder, repo: &[Migration]) -> Vec<Finding> {
    let severity = match policy {
        OutOfOrder::Block => Severity::Error,
        OutOfOrder::Warn => Severity::Warning,
        OutOfOrder::Allow => return Vec::new(),
    };
    let mut by_key: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for m in repo {
        by_key.entry(m.key()).or_default().push(&m.id);
    }
    by_key
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(key, ids)| {
            Finding::new(
                "MG006",
                severity,
                key,
                format!(
                    "several migrations share this version ({}): give each its own",
                    ids.join(", ")
                ),
            )
        })
        .collect()
}

fn compare(model: OrderModel, a: &str, b: &str) -> Ordering {
    // Only the leading digits (underscores allowed) are the version: `20261118_232700_add_v2` -> 20261118232700.
    let digits = |s: &str| {
        let lead: String = s
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '_')
            .filter(char::is_ascii_digit)
            .collect();
        lead.parse::<u128>().ok()
    };
    match model {
        OrderModel::Numeric | OrderModel::TimestampPrefix | OrderModel::SingleVersion => {
            match (digits(a), digits(b)) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => a.cmp(b),
            }
        }
        OrderModel::Lexical | OrderModel::Graph => a.cmp(b),
    }
}

fn linear(
    policy: Policy,
    model: OrderModel,
    repo: &[Migration],
    applied: &[String],
) -> Vec<Finding> {
    let Some(last) = applied.iter().max_by(|a, b| compare(model, a, b)) else {
        return Vec::new(); // nothing ran yet: nothing can be out of order
    };
    let ran: BTreeSet<&str> = applied.iter().map(String::as_str).collect();
    let (severity, why) = if policy.silent_skip || model == OrderModel::SingleVersion {
        (
            Some(Severity::Error),
            "the migrator will silently skip it forever; renumber it after the last applied one",
        )
    } else {
        match policy.out_of_order {
            OutOfOrder::Block => (
                Some(Severity::Error),
                "this project does not allow out-of-order migrations",
            ),
            OutOfOrder::Warn => (
                Some(Severity::Warning),
                "the migrator will run it out of order; check it does not depend on later ones",
            ),
            OutOfOrder::Allow => (None, ""),
        }
    };
    let Some(severity) = severity else {
        return Vec::new();
    };
    // History tables store either the version (Rails) or the full migration name (Payload, Prisma, Wrangler).
    repo.iter()
        .filter(|m| !ran.contains(m.version.as_str()) && !ran.contains(m.id.as_str()))
        .filter(|m| compare(model, &m.version, last) == Ordering::Less)
        .map(|m| {
            Finding::new(
                "MG004",
                severity,
                &m.id,
                format!("pending, but sorts before {last}, the last one production ran: {why}"),
            )
        })
        .collect()
}

fn graph(repo: &[Migration]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let keys: BTreeSet<String> = repo.iter().map(Migration::key).collect();
    let mut groups: BTreeMap<&str, Vec<&Migration>> = BTreeMap::new();
    for m in repo {
        groups.entry(&m.group).or_default().push(m);
    }
    for m in repo {
        for p in &m.parents {
            // A parent in another group may be a third-party app the project does not ship; only same-group parents
            // must exist here.
            let same_group = p.split_once('/').is_none_or(|(g, _)| g == m.group);
            if same_group && !keys.contains(p) {
                findings.push(Finding::new(
                    "MG005",
                    Severity::Error,
                    &m.id,
                    format!("depends on {p}, which does not exist"),
                ));
            }
        }
    }
    for (group, members) in groups {
        let referenced: BTreeSet<&str> = members
            .iter()
            .flat_map(|m| m.parents.iter().map(String::as_str))
            .collect();
        let heads: Vec<&str> = members
            .iter()
            .filter(|m| !referenced.contains(m.key().as_str()))
            .map(|m| m.id.as_str())
            .collect();
        if heads.len() > 1 {
            findings.push(Finding::new(
                "MG005",
                Severity::Error,
                group,
                format!(
                    "several heads ({}): add a merge migration",
                    heads.join(", ")
                ),
            ));
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(findings: &[Finding]) -> Vec<(&'static str, Severity)> {
        findings.iter().map(|f| (f.rule, f.severity)).collect()
    }

    fn lin(versions: &[&str]) -> Vec<Migration> {
        versions.iter().map(|v| Migration::linear(*v, *v)).collect()
    }

    fn ran(versions: &[&str]) -> Vec<String> {
        versions.iter().map(|v| v.to_string()).collect()
    }

    const WARN: Policy = Policy {
        model: OrderModel::TimestampPrefix,
        out_of_order: OutOfOrder::Warn,
        silent_skip: false,
        duplicate_versions: OutOfOrder::Block,
    };

    #[test]
    fn in_order_pending_migrations_pass_every_model() {
        for model in [
            OrderModel::Lexical,
            OrderModel::Numeric,
            OrderModel::TimestampPrefix,
            OrderModel::SingleVersion,
        ] {
            let p = Policy {
                model,
                out_of_order: OutOfOrder::Block,
                silent_skip: true,
                duplicate_versions: OutOfOrder::Block,
            };
            assert_eq!(
                check(p, &lin(&["0001", "0002", "0003"]), &ran(&["0001", "0002"])),
                vec![],
                "{model:?}"
            );
        }
    }

    #[test]
    fn nothing_applied_yet_passes() {
        assert_eq!(check(WARN, &lin(&["2", "1"]), &[]), vec![]);
    }

    #[test]
    fn mg004_out_of_order_warns_where_the_migrator_runs_it() {
        // Payload/Rails style: a branch merged later carries an older timestamp.
        let repo = lin(&["20261118_232700", "20261117_090000", "20261119_100000"]);
        let f = check(WARN, &repo, &ran(&["20261118_232700"]));
        assert_eq!(rules(&f), vec![("MG004", Severity::Warning)]);
        assert_eq!(f[0].object, "20261117_090000");
    }

    #[test]
    fn mg004_out_of_order_is_an_error_where_the_migrator_skips_silently() {
        let p = Policy {
            model: OrderModel::TimestampPrefix,
            out_of_order: OutOfOrder::Allow,
            silent_skip: true,
            duplicate_versions: OutOfOrder::Block,
        };
        assert_eq!(
            rules(&check(p, &lin(&["200", "100"]), &ran(&["200"]))),
            vec![("MG004", Severity::Error)]
        );
    }

    #[test]
    fn mg004_single_version_is_always_an_error() {
        // golang-migrate records only the current version; a lower pending one never runs.
        let p = Policy {
            model: OrderModel::SingleVersion,
            out_of_order: OutOfOrder::Warn,
            silent_skip: false,
            duplicate_versions: OutOfOrder::Block,
        };
        assert_eq!(
            rules(&check(p, &lin(&["5", "3"]), &ran(&["5"]))),
            vec![("MG004", Severity::Error)]
        );
    }

    #[test]
    fn mg004_allowed_out_of_order_passes() {
        // Drizzle v1 allows out-of-order by design.
        let p = Policy {
            model: OrderModel::TimestampPrefix,
            out_of_order: OutOfOrder::Allow,
            silent_skip: false,
            duplicate_versions: OutOfOrder::Block,
        };
        assert_eq!(check(p, &lin(&["200", "100"]), &ran(&["200"])), vec![]);
    }

    #[test]
    fn numeric_order_is_by_number_not_by_text() {
        let p = Policy {
            model: OrderModel::Numeric,
            out_of_order: OutOfOrder::Block,
            silent_skip: false,
            duplicate_versions: OutOfOrder::Block,
        };
        // "10" > "9" numerically, so 10 pending after 9 is in order...
        assert_eq!(check(p, &lin(&["9", "10"]), &ran(&["9"])), vec![]);
        // ...while lexically "10" < "9" would have been flagged.
        let lex = Policy {
            model: OrderModel::Lexical,
            ..p
        };
        assert_eq!(
            rules(&check(lex, &lin(&["9", "10"]), &ran(&["9"]))),
            vec![("MG004", Severity::Error)]
        );
    }

    #[test]
    fn applied_may_be_full_names_and_trailing_digits_do_not_count() {
        // Payload stores the file stem; the `2` in `v2` must not join the timestamp.
        let repo = vec![
            Migration::linear("20261118_232700_add_v2", "20261118_232700"),
            Migration::linear("20261119_080000_more", "20261119_080000"),
        ];
        assert_eq!(
            check(WARN, &repo, &ran(&["20261118_232700_add_v2"])),
            vec![]
        );
        let late = vec![
            Migration::linear("20261117_000000_old_v9", "20261117_000000"),
            repo[0].clone(),
        ];
        assert_eq!(
            rules(&check(WARN, &late, &ran(&["20261118_232700_add_v2"]))),
            vec![("MG004", Severity::Warning)]
        );
    }

    #[test]
    fn mg006_duplicate_versions_fail() {
        let repo = vec![
            Migration::linear("0002_add_a", "0002"),
            Migration::linear("0002_add_b", "0002"),
        ];
        assert_eq!(
            rules(&check(WARN, &repo, &[])),
            vec![("MG006", Severity::Error)]
        );
    }

    fn node(group: &str, version: &str, parents: &[&str]) -> Migration {
        Migration {
            id: format!("{group}.{version}"),
            version: version.into(),
            group: group.into(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
        }
    }

    const GRAPH: Policy = Policy {
        model: OrderModel::Graph,
        out_of_order: OutOfOrder::Block,
        silent_skip: false,
        duplicate_versions: OutOfOrder::Block,
    };

    #[test]
    fn a_linear_graph_and_a_merged_graph_pass() {
        let linear = vec![
            node("shop", "0001", &[]),
            node("shop", "0002", &["shop/0001"]),
        ];
        assert_eq!(check(GRAPH, &linear, &[]), vec![]);
        let merged = vec![
            node("shop", "0001", &[]),
            node("shop", "0002_a", &["shop/0001"]),
            node("shop", "0002_b", &["shop/0001"]),
            node("shop", "0003_merge", &["shop/0002_a", "shop/0002_b"]),
            node("blog", "0001", &["auth/0001"]), // another app's parent need not ship here
        ];
        assert_eq!(check(GRAPH, &merged, &[]), vec![]);
    }

    #[test]
    fn mg005_two_heads_fail() {
        let forked = vec![
            node("shop", "0001", &[]),
            node("shop", "0002_a", &["shop/0001"]),
            node("shop", "0002_b", &["shop/0001"]),
        ];
        assert_eq!(
            rules(&check(GRAPH, &forked, &[])),
            vec![("MG005", Severity::Error)]
        );
    }

    #[test]
    fn mg005_a_missing_parent_fails() {
        // The missing parent also leaves the graph with two heads (0001 and 0003): both are reported.
        let broken = vec![
            node("shop", "0001", &[]),
            node("shop", "0003", &["shop/0002"]),
        ];
        let f = check(GRAPH, &broken, &[]);
        assert_eq!(
            rules(&f),
            vec![("MG005", Severity::Error), ("MG005", Severity::Error)]
        );
        assert!(f[0].message.contains("shop/0002, which does not exist"));
    }
}
