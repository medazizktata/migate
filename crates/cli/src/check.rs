//! `migate check` and `migate ledger record`: wiring the file system and git to the core's rules.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::Args;
use migate_core::ledger::{self, Ledger};
use migate_core::order::OutOfOrder;
use migate_core::{Finding, Severity, order};
use migate_migrators::Migrator;

use crate::Outcome;
use crate::git::{AtBase, file_at_base};
use crate::output::{self, Context, Format};
use crate::{config, ledger_file, project};

#[derive(Debug, Args)]
pub struct Common {
    /// Migration tool: payload, prisma, drizzle, wrangler-d1, rails, django, supabase, dbmate (or `migrator` in migate.toml)
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
pub struct HistoryArgs {
    #[command(flatten)]
    pub common: Common,
    /// A command that prints production's applied migrations, one per line, as its history table stores them (run
    /// by the release job, with its own read-only credentials; for example `psql "$PROD_URL" -XAtc "<history query>"`)
    #[arg(long)]
    pub history_command: String,
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
    ledger_format: ledger_file::Format,
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
        migrator: {
            let mut m = migrator(&id)?;
            if let Some(level) = &file.out_of_order {
                m.set_out_of_order(match level.as_str() {
                    "block" => OutOfOrder::Block,
                    "warn" => OutOfOrder::Warn,
                    "allow" => OutOfOrder::Allow,
                    other => {
                        return Err(format!(
                            "out_of_order must be block, warn or allow, not {other:?}"
                        ));
                    }
                });
            }
            if let Some(dir) = &file.migrations_dir {
                m.set_migrations_dir(dir);
            }
            if let Some(query) = &file.history_query {
                m.history.query = query.clone();
                m.history.postgres = None;
            }
            m
        },
        root: c.root.clone(),
        ledger: c
            .ledger
            .clone()
            .or(file.ledger)
            .unwrap_or_else(|| "migate-ledger.json".into()),
        ledger_format: ledger_file::Format::from_config(file.ledger_format.as_deref())?,
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
        Ok((s, c)) => {
            let cx = Context {
                migrator: &s.migrator.id,
                base: &s.base,
                checked: c.checked,
                sandbox: c.sandbox.as_deref(),
                not_checked: c.not_checked.as_deref(),
                files: &c.files,
                ledger: &s.ledger,
            };
            if let Some(e) = &c.not_checked {
                eprintln!("migate: could not check: {e}");
            }
            emit(&output::render(args.format, &c.findings, &cx));
            // An error found is a failure whatever else happened; only then does "could not check" decide.
            if c.findings.iter().any(|f| f.severity == Severity::Error) {
                Outcome::Errors
            } else if c.not_checked.is_some() {
                Outcome::CouldNotCheck
            } else {
                Outcome::Clean
            }
        }
        Err(e) => {
            eprintln!("migate: could not check: {e}");
            if let Some(report) = output::render_failure(args.format, &e) {
                emit(&report);
            }
            Outcome::CouldNotCheck
        }
    }
}

/// What a check found.
#[derive(Debug)]
struct Checked {
    findings: Vec<Finding>,
    /// Migration id -> its file.
    files: BTreeMap<String, String>,
    /// How many migrations were read, so an empty pass shows as one.
    checked: usize,
    /// The sandbox's engine and version, when one ran (`postgres 18.6`).
    sandbox: Option<String>,
    /// Why the sandbox could not check, when it could not. The other findings still stand.
    not_checked: Option<String>,
}

/// The findings, each migration's file, and how many migrations were read (so an empty pass is visible as one).
fn run_check(s: &Settings) -> Result<Checked, String> {
    let project = project::read(&s.root, &s.migrator)?;
    let head = ledger_file::read(&s.root.join(&s.ledger), s.ledger_format)?;
    let base = match file_at_base(&s.root, &s.base, &s.ledger)? {
        AtBase::Found(text) => {
            ledger_file::parse(&text, &format!("{}:{}", s.base, s.ledger), s.ledger_format)?
        }
        AtBase::Absent => Ledger::new(),
    };
    // None left is a removal when the base had some under this migrator (each recorded one is MG002), and "nothing to
    // check", usually the wrong migrator, when it had none either.
    if project.migrations.is_empty()
        && project::migrations_at(&s.root, &s.base, &s.migrator)?.is_empty()
    {
        found_any(&project, s)?;
    }

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
    let applied: Vec<order::Ran> = base
        .keys()
        .chain(head.keys())
        .map(|id| {
            let version = version_of
                .get(id.as_str())
                .map_or(id.clone(), |v| (*v).to_string());
            (id.clone(), version)
        })
        .chain(at_base.into_iter().map(|m| (m.id, m.version)))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|(id, version)| order::Ran {
            id: Some(id),
            version,
        })
        .collect();
    findings.extend(order::check(
        s.migrator.policy(),
        &project.migrations,
        &applied,
    ));

    let (mut sandbox, mut not_checked) = (None, None);
    if let Some(cfg) = &s.sandbox {
        // A sandbox that cannot check must not hide what the ledger and the order rules found without it.
        match crate::sandbox::run(cfg, &s.migrator, &s.root, &s.base) {
            Ok((found, engine)) => {
                findings.extend(found);
                sandbox = Some(engine);
            }
            Err(e) => not_checked = Some(e),
        }
    }
    let files = project
        .files
        .iter()
        .map(|f| (f.id.clone(), f.path.clone()))
        .collect();
    Ok(Checked {
        findings,
        files,
        checked: project.migrations.len(),
        sandbox,
        not_checked,
    })
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

/// `migate history`: production's history against the repository and the ledger (MG010-MG012), at release.
pub fn history(args: &HistoryArgs) -> Outcome {
    match settings(&args.common, None).and_then(|s| run_history(args, &s).map(|r| (s, r))) {
        Ok((s, (findings, files, applied))) => {
            let cx = Context {
                migrator: &s.migrator.id,
                base: "production",
                checked: applied,
                sandbox: None,
                not_checked: None,
                files: &files,
                ledger: &s.ledger,
            };
            emit(&output::render(args.format, &findings, &cx));
            if findings.iter().any(|f| f.severity == Severity::Error) {
                Outcome::Errors
            } else {
                Outcome::Clean
            }
        }
        Err(e) => {
            let e = redact(&e);
            eprintln!("migate: could not check: {e}");
            if let Some(report) = output::render_failure(args.format, &e) {
                emit(&report);
            }
            Outcome::CouldNotCheck
        }
    }
}

type History = (Vec<Finding>, BTreeMap<String, String>, usize);

fn run_history(args: &HistoryArgs, s: &Settings) -> Result<History, String> {
    let project = project::read(&s.root, &s.migrator)?;
    let ledger = ledger_file::read(&s.root.join(&s.ledger), s.ledger_format)?;
    let out = std::process::Command::new("sh")
        .args(["-c", &args.history_command])
        .current_dir(&s.root)
        .output()
        .map_err(|e| format!("the history command could not start: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "the history command failed ({}):\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim_end()
        ));
    }
    let applied: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    if applied.is_empty() {
        return Err(
            "the history command printed no migration, so production's history could not be compared (a wrong \
             query or database reads as empty)"
                .into(),
        );
    }
    let findings = migate_core::history::compare(&applied, &project.migrations, &ledger);
    let files = project
        .files
        .iter()
        .map(|f| (f.id.clone(), f.path.clone()))
        .collect();
    Ok((findings, files, applied.len()))
}

/// `text` with the credentials in any URL hidden: `postgres://user:secret@host` becomes `postgres://***@host`.
fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c == '/' || c == '?' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => {
                out.push_str("***");
                rest = &tail[at..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
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
    let mut ledger = ledger_file::read(&path, s.ledger_format)?;
    if args.all {
        found_any(&project, s)?;
    }
    let ids: Vec<String> = if args.all {
        project.files.iter().map(|f| f.id.clone()).collect()
    } else {
        args.ids.clone()
    };
    let (mut refused, mut said) = (false, String::new());
    for id in &ids {
        let Some(file) = project.files.iter().find(|f| &f.id == id) else {
            let known: Vec<&str> = project.files.iter().map(|f| f.id.as_str()).collect();
            let shown = known
                .iter()
                .take(10)
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            let more = known.len().saturating_sub(10);
            return Err(format!(
                "no migration file for `{id}`; the {} ids here are: {shown}{}",
                s.migrator.id,
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            ));
        };
        let entry = match s.ledger_format {
            ledger_file::Format::Sha256 => file.raw_entry(),
            ledger_file::Format::Migate => file.entry(),
        };
        match ledger.get(id) {
            Some(existing) if *existing == entry => {
                said.push_str(&format!("already recorded: {id}\n"))
            }
            Some(_) => {
                eprintln!(
                    "error: `{id}` is already in the ledger with other hashes; the ledger is append-only"
                );
                refused = true;
            }
            None => {
                ledger.insert(id.clone(), entry);
                said.push_str(&format!("recorded: {id}\n"));
            }
        }
    }
    if refused {
        emit(&said);
        return Ok(Outcome::Errors);
    }
    // Written before anything is said, so a reader that leaves early (`| head`) cannot cost the ledger.
    ledger_file::write(&path, &ledger, s.ledger_format)?;
    emit(&said);
    Ok(Outcome::Clean)
}

/// Standard output that tolerates a reader leaving early (`migate ... | head`): what is left is dropped, not a panic.
fn emit(text: &str) {
    use std::io::Write;
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
}
