//! migate's command line, the composition root: it reads files and git and hands plain data to the core
//! (`docs/architecture-decisions/0001-hexagonal-core.md`).

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod check;
mod config;
mod engines;
mod git;
mod ledger_file;
mod output;
mod project;
mod sandbox;

/// Exit codes: 0 clean (warnings and notes allowed), 2 errors found, 3 could not check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Clean,
    Errors,
    CouldNotCheck,
}

impl From<Outcome> for ExitCode {
    fn from(o: Outcome) -> Self {
        ExitCode::from(match o {
            Outcome::Clean => 0,
            Outcome::Errors => 2,
            Outcome::CouldNotCheck => 3,
        })
    }
}

/// A release gate for database migrations.
#[derive(Debug, Parser)]
#[command(name = "migate", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check migrations against the ledger and the base branch: frozen (MG001-MG003, MG007) and in order (MG004-MG006)
    Check(check::CheckArgs),
    /// The ledger of migrations production has run
    #[command(subcommand)]
    Ledger(LedgerCommand),
}

#[derive(Debug, Subcommand)]
enum LedgerCommand {
    /// Record migrations production has just run (append-only)
    Record(check::RecordArgs),
}

fn main() -> ExitCode {
    // clap exits 2 on a usage error, which CI would read as "errors found": a bad invocation is "could not check".
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return if e.use_stderr() {
                Outcome::CouldNotCheck.into()
            } else {
                ExitCode::SUCCESS // --help, --version
            };
        }
    };
    match &cli.command {
        Command::Check(args) => check::check(args),
        Command::Ledger(LedgerCommand::Record(args)) => check::record(args),
    }
    .into()
}
