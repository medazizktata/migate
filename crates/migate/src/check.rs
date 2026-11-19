//! `migate check` and `migate ledger record`: wiring the file system and git to the core's rules.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::Args;
use migate_core::ledger::{self, Ledger};
use migate_core::{Finding, Severity, order};
use migate_migrators::Migrator;

use crate::Outcome;
use crate::git::{AtBase, file_at_base};
use crate::output::{self, Context, Format};
use crate::{config, ledger_file, project};

#[derive(Debug, Args)]
pub struct Common {
    /// Migration tool: payload, prisma, drizzle, wrangler-d1, rails, django (or `migrator` in migate.toml)
    #[arg(long)]
    pub migrator: Option<String>,
    /// Project directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Ledger file, relative to the project directory [default: migate-ledger.json]
    #[arg(long)]
    pub ledger: Option<String>,
    /// Settings file (default: `<root>/migate.toml`, if present)
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[command(flatten)]
    pub common: Common,
    /// The branch the pull request merges into; its ledger is the append-only reference [default: origin/main]
    #[arg(long)]
    pub base: Option<String>,
    /// Output format
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,
}

#[derive(Debug, Args)]
pub struct RecordArgs {
    #[command(flatten)]
    pub common: Common,
    /// Migration ids production has just run (as `migate check` names them)
    #[arg(required_unless_present = "all")]
    pub ids: Vec<String>,
    /// Record every migration in the repository. Only when adopting migate right after production has run all of them.
    #[arg(long, conflicts_with = "ids")]
    pub all: bool,
}

/// Flags, then migate.toml, then defaults.
#[derive(Debug)]
struct Settings {
    migrator: Migrator,
    root: PathBuf,
    ledger: String,
    base: String,
    sandbox: Option<config::Sandbox>,
}

fn settings(c: &Common, base_flag: Option<&String>) -> Result<Settings, String> {
    let file = config::load(&c.root, c.config.as_deref())?;
    let id = c
        .migrator
        .clone()
        .or(file.migrator)
        .ok_or("no migrator: pass --migrator or set `migrator` in migate.toml")?;
    Ok(Settings {
        migrator: migrator(&id)?,
        root: c.root.clone(),
        ledger: c
            .ledger
            .clone()
            .or(file.ledger)
            .unwrap_or_else(|| "migate-ledger.json".into()),
        base: base_flag
            .cloned()
            .or(file.base)
            .unwrap_or_else(|| "origin/main".into()),
        sandbox: file.sandbox,
    })
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
    let result =
        settings(&args.common, args.base.as_ref()).and_then(|s| run_check(&s).map(|r| (s, r)));
    match result {
        Ok((s, (findings, files, checked))) => {
            let cx = Context {
                migrator: &s.migrator.id,
                base: &s.base,
                checked,
                files: &files,
                ledger: &s.ledger,
            };
            print!("{}", output::render(args.format, &findings, &cx));
            if findings.iter().any(|f| f.severity == Severity::Error) {
                Outcome::Errors
            } else {
                Outcome::Clean
            }
        }
        Err(e) => {
            eprintln!("migate: could not check: {e}");
            if let Some(report) = output::render_failure(args.format, &e) {
                print!("{report}");
            }
            Outcome::CouldNotCheck
        }
    }
}

type Checked = (Vec<Finding>, BTreeMap<String, String>, usize);

/// The findings, each migration's file, and how many migrations were read (so an empty pass is visible as one).
fn run_check(s: &Settings) -> Result<Checked, String> {
    let project = project::read(&s.root, &s.migrator)?;
    found_any(&project, s)?;
    let head = ledger_file::read(&s.root.join(&s.ledger))?;
    let base = match file_at_base(&s.root, &s.base, &s.ledger)? {
        AtBase::Found(text) => ledger_file::parse(&text, &format!("{}:{}", s.base, s.ledger))?,
        AtBase::Absent => Ledger::new(),
    };

    let mut findings = ledger::check(&base, &head, &project.files, s.migrator.checksum());

    // What production ran, as versions the order models compare (the ledger stores ids).
    let version_of: BTreeMap<&str, &str> = project
        .migrations
        .iter()
        .map(|m| (m.id.as_str(), m.version.as_str()))
        .collect();
    // Production has run what the ledger records, and will have run what the base branch holds by the time this change
    // ships; without a ledger the base branch alone still shows a migration merged from an old branch.
    let at_base = project::migrations_at(&s.root, &s.base, &s.migrator)?;
    let applied: Vec<String> = base
        .keys()
        .chain(head.keys())
        .map(|id| {
            version_of
                .get(id.as_str())
                .map_or(id.clone(), |v| (*v).to_string())
        })
        .chain(at_base.into_iter().map(|m| m.version))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    findings.extend(order::check(
        s.migrator.policy(),
        &project.migrations,
        &applied,
    ));

    if let Some(cfg) = &s.sandbox {
        findings.extend(crate::sandbox::run(cfg, &s.migrator, &s.root, &s.base)?);
    }
    let files = project
        .files
        .iter()
        .map(|f| (f.id.clone(), f.path.clone()))
        .collect();
    Ok((findings, files, project.migrations.len()))
}

/// No migration found means nothing was checked: usually the wrong migrator, so name the ones whose layout matches.
fn found_any(project: &project::Project, s: &Settings) -> Result<(), String> {
    if !project.migrations.is_empty() {
        return Ok(());
    }
    let fitting: Vec<String> = migate_migrators::bundled()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|m| project::read(&s.root, m).is_ok_and(|p| !p.migrations.is_empty()))
        .map(|m| m.id)
        .collect();
    let hint = if fitting.is_empty() {
        "no bundled migrator matches any file here either".to_string()
    } else {
        format!("files here match: {}", fitting.join(", "))
    };
    Err(format!(
        "no {} migrations found in {}, so nothing was checked; {hint}",
        s.migrator.id,
        s.root.display()
    ))
}

pub fn record(args: &RecordArgs) -> Outcome {
    match settings(&args.common, None).and_then(|s| run_record(args, &s)) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("migate: could not record: {e}");
            Outcome::CouldNotCheck
        }
    }
}

fn run_record(args: &RecordArgs, s: &Settings) -> Result<Outcome, String> {
    let project = project::read(&s.root, &s.migrator)?;
    let path = s.root.join(&s.ledger);
    let mut ledger = ledger_file::read(&path)?;
    if args.all {
        found_any(&project, s)?;
    }
    let ids: Vec<String> = if args.all {
        project.files.iter().map(|f| f.id.clone()).collect()
    } else {
        args.ids.clone()
    };
    let mut refused = false;
    for id in &ids {
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
