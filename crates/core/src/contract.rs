//! Contract markers: accepting one intended breaking change, with a reason (`docs/research/architecture.md` §6.3).
//!
//! A marker is a comment line in a migration the pull request adds, in any comment style:
//!
//! ```text
//! -- migate: contract drop-column users.legacy -- Rails ignores it since v42 (ignored_columns)
//! ```
//!
//! It names one rule (by ID or name) and one object. The finding it matches stays in the report as a note saying who
//! accepted it and why. A marker that matches nothing, has no reason, or names a rule that cannot be accepted is MG901.
//! Evidence cannot be overruled: a failing old-version test, a failed or invalid run, and the migration record are never
//! accepted, so a marker an agent adds cannot hide a break the tests saw.

use crate::rules::rule;
use crate::{Finding, Severity};

/// Rules no marker can accept.
pub const FINAL: [&str; 14] = [
    "MG001", "MG002", "MG003", "MG007", "MG010", "MG011", "MG012", "MG212", "MG803", "MG804",
    "MG810", "MG811", "MG812", "MG813",
];

const TAG: &str = "migate: contract";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    /// `path:line` of the marker.
    pub at: String,
    pub rule: String,
    pub object: String,
    pub reason: String,
}

/// The markers in one migration file, and an MG901 finding for each malformed one.
pub fn parse(path: &str, text: &str) -> (Vec<Marker>, Vec<Finding>) {
    let (mut markers, mut bad) = (Vec::new(), Vec::new());
    for (n, line) in text.lines().enumerate() {
        let Some((_, rest)) = line.split_once(TAG) else {
            continue;
        };
        let at = format!("{path}:{}", n + 1);
        let (spec, reason) = rest.split_once(" -- ").unwrap_or((rest, ""));
        // A block comment may close on the same line.
        let reason = reason
            .trim()
            .trim_end_matches("*/")
            .trim_end_matches("-->")
            .trim();
        let words: Vec<&str> = spec.split_whitespace().collect();
        let problem = match (words.as_slice(), reason.is_empty()) {
            ([_, _], true) => Some("has no reason: write one after ` -- `".to_string()),
            ([r, _], false) => {
                match rule(r).or_else(|| crate::rules::RULES.iter().find(|x| x.name == *r)) {
                    None => Some(format!("names no known rule: `{r}`")),
                    Some(x) if FINAL.contains(&x.id) => {
                        Some(format!("names {}, which no marker can accept", x.id))
                    }
                    Some(_) => None,
                }
            }
            _ => Some("must read `migate: contract <rule> <object> -- <reason>`".into()),
        };
        match problem {
            Some(p) => bad.push(Finding::new(
                "MG901",
                Severity::Error,
                at,
                format!("contract marker {p}"),
            )),
            None => {
                let id = rule(words[0])
                    .or_else(|| crate::rules::RULES.iter().find(|x| x.name == words[0]))
                    .map_or(words[0], |x| x.id);
                markers.push(Marker {
                    at,
                    rule: id.into(),
                    object: words[1].into(),
                    reason: reason.into(),
                });
            }
        }
    }
    (markers, bad)
}

/// Accept the findings markers match (each becomes a note), and report every marker that matched none as MG901.
pub fn apply(findings: Vec<Finding>, markers: &[Marker]) -> Vec<Finding> {
    let matches = |m: &Marker, f: &Finding| {
        m.rule == f.rule && (f.object == m.object || f.object.ends_with(&format!(".{}", m.object)))
    };
    let mut out: Vec<Finding> = findings
        .into_iter()
        .map(|f| match markers.iter().find(|m| matches(m, &f)) {
            Some(m) if !FINAL.contains(&f.rule) && f.severity > Severity::Note => Finding::new(
                f.rule,
                Severity::Note,
                f.object,
                format!(
                    "accepted by the contract at {}: {} (was: {})",
                    m.at, m.reason, f.message
                ),
            ),
            _ => f,
        })
        .collect();
    for m in markers {
        if !out.iter().any(|f| {
            f.message
                .starts_with(&format!("accepted by the contract at {}:", m.at))
        }) {
            out.push(Finding::new(
                "MG901",
                Severity::Error,
                m.at.clone(),
                format!(
                    "contract marker matches no finding ({} {}): remove it or fix the object name",
                    m.rule, m.object
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop_col() -> Finding {
        Finding::new(
            "MG201",
            Severity::Error,
            "main.users.legacy",
            "column dropped",
        )
    }

    fn rules(f: &[Finding]) -> Vec<(&str, Severity)> {
        f.iter().map(|f| (f.rule, f.severity)).collect()
    }

    #[test]
    fn a_marker_accepts_its_finding_by_id_or_name_in_any_comment_style() {
        for line in [
            "-- migate: contract drop-column users.legacy -- unused since v42",
            "# migate: contract MG201 users.legacy -- unused since v42",
            "/* migate: contract MG201 main.users.legacy -- unused since v42 */",
        ] {
            let (markers, bad) = parse("m.sql", line);
            assert!(bad.is_empty(), "{line}: {bad:?}");
            let out = apply(vec![drop_col()], &markers);
            assert_eq!(rules(&out), [("MG201", Severity::Note)], "{line}");
            assert!(
                out[0].message.contains("unused since v42") && out[0].message.contains("m.sql:1")
            );
        }
    }

    #[test]
    fn a_marker_without_a_reason_or_with_a_bad_rule_is_mg901() {
        for line in [
            "-- migate: contract drop-column users.legacy",
            "-- migate: contract drop-column users.legacy -- ",
            "-- migate: contract no-such-rule users.legacy -- why",
            "-- migate: contract drop-column -- why",
            "-- migate: contract MG810 base -- tests are flaky",
        ] {
            let (markers, bad) = parse("m.sql", line);
            assert!(
                markers.is_empty() && rules(&bad) == [("MG901", Severity::Error)],
                "{line}: {bad:?}"
            );
        }
    }

    #[test]
    fn a_marker_on_a_note_is_stale_and_says_why_when_it_has_no_reason() {
        // A note needs no acceptance, so a marker aimed at one accepts nothing.
        let added = Finding::new(
            "MG210",
            Severity::Note,
            "main.users.nickname",
            "column added",
        );
        let (markers, _) = parse(
            "m.sql",
            "-- migate: contract add-column users.nickname -- why",
        );
        assert_eq!(
            rules(&apply(vec![added], &markers)),
            [("MG210", Severity::Note), ("MG901", Severity::Error)]
        );
        let (_, bad) = parse("m.sql", "-- migate: contract drop-column users.legacy");
        assert_eq!(
            bad[0].message,
            "contract marker has no reason: write one after ` -- `"
        );
    }

    #[test]
    fn a_stale_marker_is_mg901_and_leaves_other_findings_alone() {
        let (markers, _) = parse(
            "m.sql",
            "-- migate: contract drop-column users.other -- why",
        );
        let out = apply(vec![drop_col()], &markers);
        assert_eq!(
            rules(&out),
            [("MG201", Severity::Error), ("MG901", Severity::Error)]
        );
    }

    #[test]
    fn evidence_is_never_accepted() {
        let tests_fail = Finding::new("MG810", Severity::Error, "base tests", "fails");
        let m = Marker {
            at: "m.sql:1".into(),
            rule: "MG810".into(),
            object: "base tests".into(),
            reason: "x".into(),
        };
        assert_eq!(
            rules(&apply(vec![tests_fail], &[m])),
            [("MG810", Severity::Error), ("MG901", Severity::Error)]
        );
    }

    #[test]
    fn a_marker_names_one_object_not_a_prefix() {
        let (markers, _) = parse("m.sql", "-- migate: contract drop-column legacy -- why");
        let other = Finding::new(
            "MG201",
            Severity::Error,
            "main.users.not_legacy",
            "column dropped",
        );
        assert_eq!(
            rules(&apply(vec![other], &markers))[0],
            ("MG201", Severity::Error)
        );
    }
}
