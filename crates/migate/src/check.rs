//! `migate check` and `migate ledger record`: wiring the file system and git to the core's rules.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Args;
use migate_core::ledger::{self, Ledger};
use migate_core::{Finding, Severity, order};
use migate_migrators::Migrator;

use crate::Outcome;
use crate::git::{AtBase, file_at_base};
use crate::{ledger_file, project};

#[derive(Debug, Args)]
pub struct Common {
    /// Migration tool: payload, prisma, drizzle, wrangler-d1, rails, django
    #[arg(long)]
    pub migrator: String,
    /// Project directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Ledger file, relative to the project directory
    #[arg(long, default_value = "migate-ledger.json")]
    pub ledger: String,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[command(flatten)]
    pub common: Common,
    /// The branch the pull request merges into; its ledger is the append-only reference
    #[arg(long, default_value = "origin/main")]
    pub base: String,
}

#[derive(Debug, Args)]
pub struct RecordArgs {
    #[command(flatten)]
    pub common: Common,
    /// Migration ids production has just run (as `migate check` names them)
    #[arg(required = true)]
    pub ids: Vec<String>,
}

fn migrator(id: &str) -> Result<Migrator, String> {
    match migate_migrators::get(id) {
        Some(Ok(m)) => Ok(m),
        Some(Err(e)) => Err(e.to_string()),
        None => {
            let known: Vec<String> = migate_migrators::bundled()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|m| m.id)
                .collect();
            Err(format!(
                "unknown migrator `{id}` (known: {})",
                known.join(", ")
            ))
        }
    }
}

pub fn check(args: &CheckArgs) -> Outcome {
    match run_check(args) {
        Ok((findings, checked)) => report(&findings, checked, args),
        Err(e) => {
            eprintln!("migate: could not check: {e}");
            Outcome::CouldNotCheck
        }
    }
}

/// The findings, and how many migrations were read (so an empty pass is visible as one).
fn run_check(args: &CheckArgs) -> Result<(Vec<Finding>, usize), String> {
    let c = &args.common;
    let migrator = migrator(&c.migrator)?;
    let project = project::read(&c.root, &migrator)?;
    let head = ledger_file::read(&c.root.join(&c.ledger))?;
    let base = match file_at_base(&c.root, &args.base, &c.ledger)? {
        AtBase::Found(text) => ledger_file::parse(&text, &format!("{}:{}", args.base, c.ledger))?,
        AtBase::Absent => Ledger::new(),
    };

    let mut findings = ledger::check(&base, &head, &project.files, migrator.checksum());

    // What production ran, as versions the order models compare (the ledger stores ids).
    let version_of: BTreeMap<&str, &str> = project
        .migrations
        .iter()
        .map(|m| (m.id.as_str(), m.version.as_str()))
        .collect();
    let applied: Vec<String> = base
        .keys()
        .chain(head.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|id| {
            version_of
                .get(id.as_str())
                .map_or(id.clone(), |v| (*v).to_string())
        })
        .collect();
    findings.extend(order::check(
        migrator.policy(),
        &project.migrations,
        &applied,
    ));
    Ok((findings, project.migrations.len()))
}

fn report(findings: &[Finding], checked: usize, args: &CheckArgs) -> Outcome {
    let mut sorted: Vec<&Finding> = findings.iter().collect();
    sorted.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.rule.cmp(b.rule))
            .then(a.object.cmp(&b.object))
    });
    for f in &sorted {
        let level = match f.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        println!("{level:<8}{}  {}: {}", f.rule, f.object, f.message);
    }
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let (errors, warnings, notes) = (
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Note),
    );
    println!(
        "migate: {errors} error(s), {warnings} warning(s), {notes} note(s) in {checked} migration(s) (migrator: {}, base: {})",
        args.common.migrator, args.base
    );
    if errors > 0 {
        Outcome::Errors
    } else {
        Outcome::Clean
    }
}

pub fn record(args: &RecordArgs) -> Outcome {
    match run_record(args) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("migate: could not record: {e}");
            Outcome::CouldNotCheck
        }
    }
}

fn run_record(args: &RecordArgs) -> Result<Outcome, String> {
    let c = &args.common;
    let migrator = migrator(&c.migrator)?;
    let project = project::read(&c.root, &migrator)?;
    let path = c.root.join(&c.ledger);
    let mut ledger = ledger_file::read(&path)?;
    let mut refused = false;
    for id in &args.ids {
        let Some(file) = project.files.iter().find(|f| &f.id == id) else {
            return Err(format!("no migration file for `{id}`"));
        };
        let entry = file.entry();
        match ledger.get(id) {
            Some(existing) if *existing == entry => println!("already recorded: {id}"),
            Some(_) => {
                eprintln!(
                    "error: `{id}` is already in the ledger with other hashes; the ledger is append-only"
                );
                refused = true;
            }
            None => {
                ledger.insert(id.clone(), entry);
                println!("recorded: {id}");
            }
        }
    }
    if refused {
        return Ok(Outcome::Errors);
    }
    ledger_file::write(&path, &ledger)?;
    Ok(Outcome::Clean)
}
