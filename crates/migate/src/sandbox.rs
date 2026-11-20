//! The throwaway-database run (`[sandbox]` in migate.toml), SQLite for now.
//!
//! Two git worktrees, one database: migrate it at the base commit, read the schema, migrate it at HEAD, read it again;
//! diff and classify; then run the base version's tests against the migrated database, which is what the version still
//! serving traffic meets after the deploy (`docs/research/spike-7.md`, `spike-31.md`).

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};
use std::time::Duration;

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use migate_core::contract;
use migate_core::diff::{Change, classify_with, diff};
use migate_core::engine::{self, Database, TriggerProbe};
use migate_core::schema::Kind;
use migate_core::{Finding, Severity};
use migate_migrators::Migrator;

use crate::config::{Engine, Sandbox};
use crate::project;

/// Variables passed from migate's own environment without being asked for: the toolchain's, never a connection or a
/// credential (`BUNDLE_<host>` holds gem-server passwords, so Bundler's settings are named one by one).
const PASSED: [&str; 18] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "TMPDIR",
    "TEMP",
    "TMP",
    "LANG",
    "GEM_HOME",
    "GEM_PATH",
    "BUNDLE_PATH",
    "BUNDLE_GEMFILE",
    "BUNDLE_APP_CONFIG",
    "BUNDLE_WITHOUT",
    "BUNDLE_DEPLOYMENT",
    "BUNDLE_FROZEN",
];
const PASSED_PREFIXES: [&str; 9] = [
    "LC_",
    "XDG_",
    "RBENV_",
    "PYENV_",
    "VIRTUAL_ENV",
    "CONDA_",
    "NVM_",
    "VOLTA_",
    "COREPACK_",
];

/// The findings, and which engine and version the throwaway database ran (`postgres 18.6`).
pub fn run(
    cfg: &Sandbox,
    migrator: &Migrator,
    root: &Path,
    base: &str,
) -> Result<(Vec<Finding>, String), String> {
    on_signals();
    let tmp = TempDir::create()?;
    sweep(root, &tmp.0);
    let dir = tmp.0.join("db");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = cfg.database_name.clone().unwrap_or_default();
    // Where the migrator records what ran, on this engine.
    let history = match cfg.engine {
        Engine::Postgres => migrator
            .history
            .postgres
            .as_deref()
            .unwrap_or(&migrator.history.query),
        Engine::Sqlite => &migrator.history.query,
    };
    // The migrator's location goes with its own command; a project that replaces the command says where it writes.
    let tool_location = migrator
        .migrate
        .database
        .as_deref()
        .filter(|_| cfg.migrate.is_none());
    let in_dir = |s: &str| s.replace("{dir}", &dir.to_string_lossy());
    let sandbox: Box<dyn engine::Sandbox> = match cfg.engine {
        Engine::Sqlite => Box::new(migate_engine_sqlite::Throwaway::new(
            dir.join("sandbox.db"),
            cfg.database.as_deref().or(tool_location).map(in_dir),
        )),
        // Started and recorded under the lock, as `sh` does: an interrupt either stops it or comes before it starts.
        Engine::Postgres => Box::new(migate_engine_postgres::Server::start(
            &dir,
            cfg.target_version.as_deref(),
            &mut |command| {
                let mut l = live();
                let child = command.spawn()?;
                l.servers.push(child.id());
                Ok(child)
            },
        )?),
    };
    let placeholders = sandbox.placeholders();
    let vars = |s: &str| {
        let mut s = in_dir(s).replace("{database}", &name);
        for (key, value) in &placeholders {
            s = s.replace(&format!("{{{key}}}"), value);
        }
        s
    };

    let command = cfg.migrate.as_deref().unwrap_or(&migrator.migrate.command);
    if command.contains("{database}") && cfg.database_name.is_none() {
        return Err(format!(
            "the {} migrate command needs the database's name: set `database_name` under [sandbox] in migate.toml \
             (the name or binding in the project's own configuration)",
            migrator.id
        ));
    }
    let migrate = vars(command);
    let mut env: BTreeMap<String, String> = std::env::vars()
        .filter(|(k, _)| {
            PASSED.contains(&k.as_str())
                || PASSED_PREFIXES.iter().any(|p| k.starts_with(p))
                || cfg.pass_env.contains(k)
        })
        .collect();
    env.extend(
        migrator
            .migrate
            .env
            .iter()
            .chain(&cfg.env)
            .map(|(k, v)| (k.clone(), vars(v))),
    );
    // The migrate command also gets the migrator's run mode; the config's own values still win.
    // The project's own temp files (test runners, Wrangler) go in the run's folder, so its cleanup takes them too.
    let scratch = tmp.0.join("tmp");
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    for var in ["TMPDIR", "TEMP", "TMP"] {
        env.insert(var.into(), scratch.to_string_lossy().into_owned());
    }
    let mut migrate_env = env.clone();
    migrate_env.extend(
        migrator
            .migrate
            .migrate_env
            .iter()
            .chain(&cfg.env)
            .map(|(k, v)| (k.clone(), vars(v))),
    );

    let prefix = git(root, &["rev-parse", "--show-prefix"])?;
    if !git(root, &["status", "--porcelain", "--", "."])?.is_empty() {
        eprintln!(
            "migate: note: uncommitted changes are not part of the sandbox run, which uses HEAD"
        );
    }
    let base_co = Checkout::add(root, base, &tmp.0.join("base"))?;
    let head_co = Checkout::add(root, "HEAD", &tmp.0.join("head"))?;
    let (base_dir, head_dir) = (base_co.0.join(prefix.trim()), head_co.0.join(prefix.trim()));

    // The migrations this change adds: where findings point, and the only place contract markers count (an old marker
    // would otherwise accept future changes).
    let at_base: BTreeSet<String> = project::read(&base_dir, migrator)?
        .files
        .into_iter()
        .map(|f| f.path)
        .collect();
    let added: Vec<(String, String)> = project::read(&head_dir, migrator)?
        .files
        .into_iter()
        .filter(|f| !at_base.contains(&f.path))
        .map(|f| (f.path, String::from_utf8_lossy(&f.bytes).into_owned()))
        .collect();
    // Syntax production's engine version lacks, in the SQL migrations this change adds.
    let mut too_new = Vec::new();
    if let Some(target) = &cfg.target_version {
        for (path, text) in added.iter().filter(|(p, _)| p.ends_with(".sql")) {
            for (line, feature, since) in sandbox.newer_syntax(text, target)? {
                too_new.push(
                    Finding::new(
                        "MG701",
                        Severity::Error,
                        feature,
                        format!("needs version {since}; production runs {target} (target_version), so it fails at deploy"),
                    )
                    .at(path.clone(), line),
                );
            }
        }
    }
    let (mut markers, mut bad_markers) = (Vec::new(), Vec::new());
    for (path, text) in &added {
        let (m, bad) = contract::parse(path, text);
        markers.extend(m);
        bad_markers.extend(bad);
    }

    if let Some(rel) = &cfg.shared {
        share(rel, &dir.join("shared"), &[&base_dir, &head_dir])?;
    }

    if let Some(setup) = &cfg.setup {
        for (name, d) in [("base", &base_dir), ("head", &head_dir)] {
            let out = sh(&vars(setup), d, &env)?;
            if !out.status.success() {
                return Err(format!(
                    "the setup command failed in the {name} checkout:\n{}",
                    tail(&out)
                ));
            }
        }
    }

    eprintln!("migate: migrating a throwaway database to {base}");
    let out = sh(&migrate, &base_dir, &migrate_env)?;
    if !out.status.success() {
        return Err(format!(
            "the base migrations fail on an empty database, so nothing was checked:\n{}",
            tail(&out)
        ));
    }
    let not_migrated = |why: String| {
        format!(
            "MG804 sandbox-not-migrated: {why}. The migrate command probably wrote somewhere else: check that it is \
             told where the database is ({{db}} in a variable under [sandbox] env, or {{dir}} in a flag such as \
             --persist-to), and that [sandbox] database names the file it writes"
        )
    };
    let db = sandbox.open(history).map_err(not_migrated)?;
    let ran = project::read(&base_dir, migrator)?.migrations.len();
    if ran > 0 {
        let rows = db
            .count_rows(history)
            .map_err(|e| not_migrated(format!("its migration history cannot be read ({e})")))?;
        if rows == 0 {
            return Err(not_migrated(format!(
                "its migration history is empty after {ran} base migrations"
            )));
        }
    }
    let before = db.snapshot()?;
    let invalid =
        |why: String| format!("MG813 old-version-run-invalid: {why}. Nothing was checked");

    if let Some(seed) = &cfg.seed {
        let out = sh(&vars(seed), &base_dir, &env)?;
        if !out.status.success() {
            return Err(format!("the seed command failed:\n{}", tail(&out)));
        }
    }
    // Calibration, before the check: the tests must pass on their own version's database.
    if let Some(test) = &cfg.test {
        eprintln!("migate: running the base version's tests on its own database first");
        let mut out = sh(&vars(test), &base_dir, &env)?;
        if !out.status.success() {
            // One retry: a one-off flake (a busy database while a runtime starts) should not cost the verdict.
            eprintln!(
                "migate: the base version's tests failed on their own database; running them once again:\n{}",
                tail(&out)
            );
            out = sh(&vars(test), &base_dir, &env)?;
        }
        if !out.status.success() {
            return Err(invalid(format!(
                "the base version's tests fail on the database at {base}, before any new migration, so their \
                 result after one would mean nothing:\n{}",
                tail(&out)
            )));
        }
    }
    let rows_before = db.row_counts()?;
    let unmodelled_before = db.unmodelled()?;
    let values_before = db.value_counts()?;

    eprintln!("migate: migrating it to HEAD");
    let out = sh(&migrate, &head_dir, &migrate_env)?;
    let after = db.snapshot()?;
    // Read now: migate's own calibration writes to the file later, which would record migate's version instead.
    let newer_writer = db.newer_writer();
    let target_gap = match &cfg.target_version {
        Some(target) => db.target_gap(target)?,
        None => None,
    };
    let mut findings = too_new;
    findings.extend(target_gap);
    if let Some(why) = newer_writer {
        findings.push(Finding::new(
            "MG805",
            Severity::Warning,
            "sandbox",
            format!("{why}: a feature newer than migate's SQLite may be misread or unreadable; update migate"),
        ));
    }
    let migrated = out.status.success();
    if !migrated {
        findings.push(Finding::new(
            "MG803",
            Severity::Error,
            "head migrations",
            format!(
                "the migrations fail on a database at {base}; any schema change they made before failing is a finding \
                 of its own.{}\n{}",
                held_rows(&rows_before),
                tail(&out)
            ),
        ));
    }
    let values_after = db.value_counts()?;
    let filled = values_after
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(k, _)| k.clone())
        .collect();
    let changes = diff(&before, &after);
    findings.extend(classify_with(&changes, &filled));
    judge_triggers(&mut findings, &changes, db.as_ref());
    let unmodelled_after = db.unmodelled()?;
    for key in unmodelled_before
        .keys()
        .chain(unmodelled_after.keys())
        .collect::<BTreeSet<_>>()
    {
        let what = match (unmodelled_before.get(key), unmodelled_after.get(key)) {
            (Some(was), Some(now)) if was == now => continue,
            (None, _) => "added",
            (_, None) => "removed",
            _ => "changed",
        };
        findings.push(Finding::new(
            "MG801",
            Severity::Warning,
            key.clone(),
            format!("{what}; migate's schema model does not cover it, so only the base version's tests judged it"),
        ));
    }
    let mut lost_rows = BTreeSet::new();
    for (table, now) in db.row_counts()? {
        if let Some(&was) = rows_before.get(&table)
            && now < was
        {
            findings.push(Finding::new(
                "MG105",
                Severity::Error,
                table.clone(),
                format!("{was} rows before the migrations, {now} after"),
            ));
            lost_rows.insert(table);
        }
    }
    // Values a migration blanked without losing a row; a table that lost rows is already reported.
    for (column, &now) in &values_after {
        let table = column.rsplit_once('.').map_or(column.as_str(), |(t, _)| t);
        if let Some(&was) = values_before.get(column)
            && now < was
            && !lost_rows.contains(table)
        {
            findings.push(Finding::new(
                "MG106",
                Severity::Error,
                column.clone(),
                format!("{was} values before the migrations, {now} after"),
            ));
        }
    }

    match (&cfg.test, migrated) {
        (Some(test), true) => {
            let recorded = db.count_rows(history)?;
            eprintln!("migate: running the base version's tests against it");
            let out = sh(&vars(test), &base_dir, &env)?;
            if !out.status.success() {
                findings.push(Finding::new(
                    "MG810",
                    Severity::Error,
                    "base tests",
                    format!(
                        "the version at {base}, still serving traffic during the deploy, fails its tests on the migrated \
                         database:\n{}",
                        tail(&out)
                    ),
                ));
            } else {
                // Calibration, after a pass: the database must still be the migrated one, and the tests must notice
                // when its tables are gone.
                let still = db.count_rows(history).unwrap_or(0);
                if still < recorded {
                    return Err(invalid(format!(
                        "the migration history went from {recorded} rows to {still} during the tests: they rebuilt the \
                     database instead of using the migrated one"
                    )));
                }
                db.drop_all_but(history)?;
                eprintln!("migate: checking the tests notice a database with its tables dropped");
                if sh(&vars(test), &base_dir, &env)?.status.success() {
                    return Err(invalid(
                    "the base version's tests still pass with every table but the migration history dropped, so they \
                     never used the migrated database (a stub command, or a framework that makes its own test \
                     database)"
                        .into(),
                ));
                }
            }
        }
        (None, _) => findings.push(Finding::new(
            "MG810",
            Severity::Note,
            "base tests",
            "not run: no `test` under [sandbox], so only the schema diff judged this change",
        )),
        (Some(_), false) => {}
    }
    let findings: Vec<Finding> = findings.into_iter().map(|f| place(f, &added)).collect();
    let mut findings = contract::apply(findings, &markers);
    findings.extend(bad_markers);
    let engine = format!(
        "{} {}",
        before.engine(),
        before.engine_version().unwrap_or("(version unknown)")
    );
    Ok((findings, engine))
}

/// Make `rel` in each checkout a link to one folder, so base and head migrate the same database.
fn share(rel: &str, target: &Path, checkouts: &[&PathBuf]) -> Result<(), String> {
    let path = Path::new(rel);
    if rel.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "[sandbox] shared must be a folder inside the project, not `{rel}`"
        ));
    }
    std::fs::create_dir_all(target).map_err(|e| format!("{}: {e}", target.display()))?;
    for checkout in checkouts {
        let link = checkout.join(path);
        if link.symlink_metadata().is_ok() {
            return Err(format!(
                "[sandbox] shared `{rel}` is in the repository; it must be a folder git does not track"
            ));
        }
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::os::unix::fs::symlink(target, &link)
            .map_err(|e| format!("{}: {e}", link.display()))?;
    }
    Ok(())
}

/// Point a finding at the added migration that names its object (the latest one, at the first line naming it), else at
/// the latest added migration. Findings stay unplaced when the change adds no migration.
fn place(f: Finding, added: &[(String, String)]) -> Finding {
    if f.at.is_some() {
        return f;
    }
    let Some((last, _)) = added.last() else {
        return f;
    };
    let name = f
        .object
        .rsplit('.')
        .next()
        .unwrap_or(&f.object)
        .to_ascii_lowercase();
    let word = |line: &str| {
        let line = line.to_ascii_lowercase();
        line.match_indices(&name).any(|(i, _)| {
            let ok = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            ok(line[..i].chars().next_back()) && ok(line[i + name.len()..].chars().next())
        })
    };
    for (path, text) in added.iter().rev() {
        if let Some(n) = text.lines().position(word) {
            return f.at(path.clone(), u32::try_from(n + 1).unwrap_or(1));
        }
    }
    f.at(last.clone(), 1)
}

// shortcut: commands run through `sh` with no time limit; Windows and a timeout when someone needs them.
/// Each command runs in its own process group, so an interrupt can stop it and everything it started.
fn sh(command: &str, dir: &Path, env: &BTreeMap<String, String>) -> Result<Output, String> {
    let child = {
        let mut l = live();
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(dir)
            .env_clear()
            .envs(env)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run `{command}`: {e}"))?;
        l.groups.push(child.id());
        child
    };
    let id = child.id();
    let out = child
        .wait_with_output()
        .map_err(|e| format!("cannot run `{command}`: {e}"));
    live().groups.retain(|g| *g != id);
    out
}

/// Evidence for a new or changed trigger's rule, never a reason to soften it: when rows the database already holds would
/// be refused if written again, the error says how many and why. Stored rows cannot show the opposite, that the old
/// app's writes pass: a migration that fills a new column and then requires it satisfies every stored row and refuses
/// every insert from the old app (seen on a real app, and in a blind test). A probe that cannot judge adds nothing.
fn judge_triggers(findings: &mut [Finding], changes: &[Change], db: &dyn Database) {
    for change in changes {
        let o = match change {
            Change::Added(o) | Change::Changed { head: o, .. } => o,
            Change::Dropped(_) => continue,
        };
        let (Kind::Trigger, Some(table)) = (o.kind(), o.parent.as_deref()) else {
            continue;
        };
        let label = format!("{}.{table}.{}", o.namespace, o.name);
        let Some(f) = findings
            .iter_mut()
            .find(|f| matches!(f.rule, "MG503" | "MG504") && f.object == label)
        else {
            continue;
        };
        if let Ok(TriggerProbe::Refused {
            refused,
            checked,
            held,
            message,
        }) = db.probe_trigger(table, &o.name)
        {
            let of = if checked == held {
                format!("the {held} rows")
            } else {
                format!("the first {checked} of the {held} rows")
            };
            f.message.push_str(&format!(
                "; {refused} of {of} the database holds would already be refused if written again: \"{message}\""
            ));
        }
    }
}

/// The rows the database held when the head migrations ran (the base migrations', the seed's and the base tests'):
/// a constraint or NOT NULL column those rows break fails the migration, though its SQL is sound.
fn held_rows(rows: &BTreeMap<String, u64>) -> String {
    let held: Vec<String> = rows
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(t, n)| format!("{t} ({n} rows)"))
        .collect();
    if held.is_empty() {
        return String::new();
    }
    format!(
        " The database held rows when they ran, which production will too: {}. A constraint, unique index or \
         NOT NULL column those rows break fails this way.",
        held.join(", ")
    )
}

/// What a failed command said, for the report: exit status, then [`excerpt`] of its output.
fn tail(out: &Output) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let code = out
        .status
        .code()
        .map_or("a signal".into(), |c| c.to_string());
    format!("(exit {code})\n{}", excerpt(&text))
}

/// The lines a reader needs from a failing command: those reporting a failure (up to 10), then the last 5 for the
/// summary; the last 15 when nothing reads as a failure. Colour codes are removed. A test runner prints its summary
/// after the error, so the last lines alone hid the cause.
fn excerpt(text: &str) -> String {
    let plain = strip_ansi(text);
    let lines: Vec<&str> = plain.lines().filter(|l| !l.trim().is_empty()).collect();
    let failing = |l: &&str| {
        // A passing test named after a failure case ("✓ a failure at item 9 ...") is not a failure.
        if l.trim_start().starts_with(['\u{2713}', '\u{2714}']) {
            return false;
        }
        let l = l.to_ascii_lowercase();
        [
            "error",
            "fail",
            "exception",
            "traceback",
            "assert",
            "does not exist",
            "no such",
        ]
        .iter()
        .any(|w| l.contains(w))
    };
    let tail_from = lines.len().saturating_sub(5);
    let mut keep: Vec<&str> = lines[..tail_from]
        .iter()
        .copied()
        .filter(failing)
        .take(10)
        .collect();
    if keep.is_empty() {
        return lines[lines.len().saturating_sub(15)..].join("\n");
    }
    keep.push("...");
    keep.extend(&lines[tail_from..]);
    keep.join("\n")
}

/// Text without terminal escape sequences (`ESC [ ... letter`).
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::excerpt;

    #[test]
    fn the_excerpt_keeps_the_failure_and_the_summary_without_colour() {
        let mut out = String::from(
            "\u{1b}[32m RUN \u{1b}[39m v3\n\u{1b}[31mFAIL\u{1b}[39m tests/users.test.ts > creates a user\n",
        );
        out.push_str("\u{1b}[31mError: table users has no column named legacy\u{1b}[39m\n");
        for i in 0..20 {
            out.push_str(&format!("    at frame {i}\n"));
        }
        out.push_str(" Test Files  1 failed | 4 passed (5)\n");
        let got = excerpt(&out);
        assert_eq!(
            got,
            "FAIL tests/users.test.ts > creates a user\nError: table users has no column named legacy\n...\n    at frame 16\n    at frame 17\n    at frame 18\n    at frame 19\n Test Files  1 failed | 4 passed (5)"
        );
    }

    #[test]
    fn passing_tests_whose_names_say_fail_do_not_crowd_out_the_failures() {
        // From a real vitest run: six failures, with passing tests named after failure cases in between.
        let mut out = String::new();
        for i in 0..6 {
            out.push_str(&format!(
                "   \u{2713} a failure at item {i} leaves nothing behind\n"
            ));
            out.push_str(&format!(" FAIL  tests/quotes.spec.ts > case {i}\n"));
        }
        out.push_str(&"summary\n".repeat(5));
        let got = excerpt(&out);
        assert!(
            (0..6).all(|i| got.contains(&format!("> case {i}"))),
            "{got}"
        );
        assert!(!got.contains('\u{2713}'), "{got}");
    }

    #[test]
    fn a_missing_column_line_counts_as_a_failure() {
        // Prisma's wording carries none of the usual failure words.
        let mut out =
            String::from("The column `users.legacy` does not exist in the current database.\n");
        out.push_str(&"noise\n".repeat(20));
        assert!(
            excerpt(&out).contains("does not exist"),
            "{}",
            excerpt(&out)
        );
    }

    #[test]
    fn output_with_no_failure_line_keeps_its_last_fifteen_lines() {
        let out: String = (0..30).map(|i| format!("line {i}\n")).collect();
        let got = excerpt(&out);
        assert!(
            got.starts_with("line 15\n") && got.ends_with("line 29"),
            "{got}"
        );
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = crate::git::git(root, args)?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What an interrupted run must undo: running commands' process groups, worktrees (with their repository), folders.
/// The lock is held while each is made or undone, and by the signal handler until it exits, so the two never race.
#[derive(Debug)]
struct Live {
    groups: Vec<u32>,
    /// Database servers, stopped their own way ([`stop_server`]).
    servers: Vec<u32>,
    worktrees: Vec<(PathBuf, PathBuf)>,
    dirs: Vec<PathBuf>,
}

static LIVE: Mutex<Live> = Mutex::new(Live {
    groups: Vec::new(),
    servers: Vec::new(),
    worktrees: Vec::new(),
    dirs: Vec::new(),
});

fn live() -> MutexGuard<'static, Live> {
    LIVE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// On SIGINT, SIGTERM or SIGHUP (Ctrl-C, a cancelled CI job, a closed terminal): stop the running command, remove the
/// worktrees and folders, exit 3. Without it the process dies at once and nothing is removed (test drive 1).
fn on_signals() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(mut signals) = Signals::new([SIGINT, SIGTERM, SIGHUP]) else {
            return;
        };
        std::thread::spawn(move || {
            if signals.forever().next().is_some() {
                let mut l = live();
                for g in &l.groups {
                    stop_group(*g);
                }
                for s in &l.servers {
                    stop_server(*s);
                }
                for (dir, root) in l.worktrees.drain(..) {
                    remove_worktree(&root, &dir);
                }
                for d in l.dirs.drain(..) {
                    let _ = std::fs::remove_dir_all(d);
                }
                eprintln!("migate: interrupted; the sandbox run was removed. Nothing was checked");
                std::process::exit(3);
            }
        });
    });
}

/// SIGINT to a database server: PostgreSQL's fast shutdown, which ends the sessions and exits only once its children
/// have. A process-group signal would reach the server alone, since each child runs in a session of its own
/// (`setsid()` in PostgreSQL's miscinit.c). SIGKILL only if it is still running after 30 seconds.
fn stop_server(pid: u32) {
    let send = |sig: &str| {
        let _ = Command::new("kill")
            .args([sig, &pid.to_string()])
            .stderr(Stdio::null())
            .status();
    };
    // An exited child stays a zombie until reaped, and a zombie still answers `kill -0`: read its state instead.
    let running = || {
        Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .is_ok_and(|o| {
                let state = String::from_utf8_lossy(&o.stdout);
                let state = state.trim();
                !state.is_empty() && !state.starts_with('Z')
            })
    };
    send("-INT");
    for _ in 0..300 {
        if !running() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    send("-KILL");
}

/// SIGTERM to a process group, then SIGKILL if it is still there after two seconds.
fn stop_group(group: u32) {
    let signal = |sig: &str| {
        Command::new("kill")
            .args([sig, &format!("-{group}")])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    signal("-TERM");
    for _ in 0..20 {
        if !signal("-0") {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    signal("-KILL");
}

fn remove_worktree(root: &Path, dir: &Path) {
    if git(
        root,
        &["worktree", "remove", "--force", &dir.to_string_lossy()],
    )
    .is_err()
    {
        let _ = std::fs::remove_dir_all(dir);
        let _ = git(root, &["worktree", "prune"]);
    }
}

/// Remove what runs killed outright left behind (SIGKILL, a crash, power loss): this user's run folders whose process
/// is gone, then this repository's records of worktrees that no longer exist.
fn sweep(root: &Path, own: &Path) {
    let Ok(uid) = std::fs::metadata(own).map(|m| m.uid()) else {
        return;
    };
    let me = std::process::id();
    for entry in std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = name
            .strip_prefix("migate-run-")
            .and_then(|rest| rest.split('-').next())
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        let mine = entry.metadata().is_ok_and(|m| m.uid() == uid);
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if mine && pid != me && !alive {
            // A Postgres the killed run started outlives it; stop it before its folder goes.
            migate_engine_postgres::stop_left_server(&entry.path().join("db"));
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    let _ = git(root, &["worktree", "prune"]);
}

/// A detached worktree at `rev`, removed on drop.
#[derive(Debug)]
struct Checkout(PathBuf, PathBuf);

impl Checkout {
    fn add(root: &Path, rev: &str, dir: &Path) -> Result<Checkout, String> {
        let mut l = live();
        git(
            root,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                &dir.to_string_lossy(),
                rev,
            ],
        )
        .map_err(|e| format!("cannot check out `{rev}`: {e}"))?;
        l.worktrees.push((dir.to_path_buf(), root.to_path_buf()));
        Ok(Checkout(dir.to_path_buf(), root.to_path_buf()))
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let mut l = live();
        remove_worktree(&self.1, &self.0);
        l.worktrees.retain(|w| w.0 != self.0);
    }
}

/// The run's folder, removed on drop (after the checkouts inside it, which are dropped first).
#[derive(Debug)]
struct TempDir(PathBuf);

impl TempDir {
    fn create() -> Result<TempDir, String> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "migate-run-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let mut l = live();
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        l.dirs.push(dir.clone());
        Ok(TempDir(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let mut l = live();
        let _ = std::fs::remove_dir_all(&self.0);
        l.dirs.retain(|d| *d != self.0);
    }
}
