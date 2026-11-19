//! Rendering findings: text for people, JSON for scripts, SARIF 2.1.0 for GitHub code scanning.

use std::collections::BTreeMap;

use migate_core::rules::rule;
use migate_core::{Finding, Severity};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Text,
    Json,
    Sarif,
}

/// What a report needs besides the findings.
#[derive(Debug)]
pub struct Context<'a> {
    pub migrator: &'a str,
    pub base: &'a str,
    pub checked: usize,
    /// Migration id -> its file, relative to the project; findings without one point at the ledger.
    pub files: &'a BTreeMap<String, String>,
    pub ledger: &'a str,
}

fn level(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}

fn sorted(findings: &[Finding]) -> Vec<&Finding> {
    let mut v: Vec<&Finding> = findings.iter().collect();
    v.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.rule.cmp(b.rule))
            .then(a.object.cmp(&b.object))
    });
    v
}

fn counts(findings: &[Finding]) -> (usize, usize, usize) {
    let n = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    (n(Severity::Error), n(Severity::Warning), n(Severity::Note))
}

pub fn render(format: Format, findings: &[Finding], cx: &Context) -> String {
    match format {
        Format::Text => text(findings, cx),
        Format::Json => json_report(findings, cx).to_string() + "\n",
        Format::Sarif => sarif(findings, cx).to_string() + "\n",
    }
}

fn text(findings: &[Finding], cx: &Context) -> String {
    let mut out = String::new();
    for f in sorted(findings) {
        out.push_str(&format!(
            "{:<8}{}  {}: {}\n",
            level(f.severity),
            f.rule,
            f.object,
            f.message
        ));
    }
    let (e, w, n) = counts(findings);
    out.push_str(&format!(
        "migate: {e} error(s), {w} warning(s), {n} note(s) in {} migration(s) (migrator: {}, base: {})\n",
        cx.checked, cx.migrator, cx.base
    ));
    out
}

fn json_report(findings: &[Finding], cx: &Context) -> Value {
    let (e, w, n) = counts(findings);
    json!({
        "migate": env!("CARGO_PKG_VERSION"),
        "migrator": cx.migrator,
        "base": cx.base,
        "checked": cx.checked,
        "summary": { "errors": e, "warnings": w, "notes": n },
        "findings": sorted(findings).iter().map(|f| json!({
            "rule": f.rule,
            "severity": level(f.severity),
            "object": f.object,
            "file": file_of(f, cx),
            "message": f.message,
        })).collect::<Vec<_>>(),
    })
}

fn file_of<'a>(f: &Finding, cx: &'a Context) -> &'a str {
    cx.files.get(&f.object).map_or(cx.ledger, String::as_str)
}

fn sarif(findings: &[Finding], cx: &Context) -> Value {
    let mut used: Vec<&str> = findings.iter().map(|f| f.rule).collect();
    used.sort_unstable();
    used.dedup();
    let rules: Vec<Value> = used
        .iter()
        .map(|id| {
            let (name, description) = rule(id)
                .map_or(("unknown", "Not in migate's rule catalogue."), |r| {
                    (r.name, r.description)
                });
            json!({
                "id": id,
                "name": name,
                "shortDescription": { "text": name },
                "fullDescription": { "text": description },
                "help": { "text": description },
            })
        })
        .collect();
    let results: Vec<Value> = sorted(findings)
        .iter()
        .map(|f| {
            json!({
                "ruleId": f.rule,
                "ruleIndex": used.iter().position(|id| *id == f.rule).unwrap_or(0),
                "level": level(f.severity),
                "message": { "text": format!("{}: {}", f.object, f.message) },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": file_of(f, cx) },
                        "region": { "startLine": 1 }
                    }
                }],
                // Stable across runs and line moves, so code scanning tracks one alert per rule and object.
                "partialFingerprints": { "migate/v1": format!("{}:{}", f.rule, f.object) },
            })
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "migate",
                "version": env!("CARGO_PKG_VERSION"),
                "rules": rules,
            }},
            "results": results,
        }],
    })
}
