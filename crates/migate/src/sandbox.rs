//! The throwaway-database run (`[sandbox]` in migate.toml), SQLite for now.
//!
//! Two git worktrees, one database: migrate it at the base commit, read the schema, migrate it at HEAD, read it again;
//! diff and classify; then run the base version's tests against the migrated database, which is what the version still
//! serving traffic meets after the deploy (`docs/research/spike-7.md`, `spike-31.md`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use migate_core::diff::{classify, diff};
use migate_core::{Finding, Severity};
use migate_migrators::Migrator;

use crate::config::Sandbox;
use crate::project;

/// Variables passed from migate's own environment without being asked for: the toolchain's, never a connection.
const PASSED: [&str; 10] = [
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "TERM", "TMPDIR", "TEMP", "TMP", "LANG",
];
const PASSED_PREFIXES: [&str; 12] = [
    "LC_",
    "XDG_",
    "GEM_",
    "BUNDLE_",
    "RBENV_",
    "PYENV_",
    "VIRTUAL_ENV",
    "CONDA_",
    "NVM_",
    "VOLTA_",
    "COREPACK_",
    "CARGO_",
];

pub fn run(
    cfg: &Sandbox,
    migrator: &Migrator,
    root: &Path,
    base: &str,
) -> Result<Vec<Finding>, String> {
    let tmp = TempDir::create()?;
    let dir = tmp.0.join("db");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let db = dir.join("sandbox.db");
    let vars = |s: &str| {
        s.replace("{db}", &db.to_string_lossy())
            .replace("{dir}", &dir.to_string_lossy())
    };

    let migrate = vars(cfg.migrate.as_deref().unwrap_or(&migrator.migrate.command));
    if migrate.contains("{database}") {
        return Err(format!(
            "the {} migrate command needs the database name: set `migrate` under [sandbox] in migate.toml \
             (for example `{}`)",
            migrator.id,
            migrate
                .replace("{database}", "DB")
                .replace("--local", "--local --persist-to {dir}")
        ));
    }
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

    let prefix = git(root, &["rev-parse", "--show-prefix"])?;
    if !git(root, &["status", "--porcelain", "--", "."])?.is_empty() {
        eprintln!(
            "migate: note: uncommitted changes are not part of the sandbox run, which uses HEAD"
        );
    }
    let base_co = Checkout::add(root, base, &tmp.0.join("base"))?;
    let head_co = Checkout::add(root, "HEAD", &tmp.0.join("head"))?;
    let (base_dir, head_dir) = (base_co.0.join(prefix.trim()), head_co.0.join(prefix.trim()));

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
    let out = sh(&migrate, &base_dir, &env)?;
    if !out.status.success() {
        return Err(format!(
            "the base migrations fail on an empty database, so nothing was checked:\n{}",
            tail(&out)
        ));
    }
    let path = locate(cfg.database.as_deref().map(vars), &db)?;
    let not_migrated = |why: String| {
        format!(
            "MG804 sandbox-not-migrated: {why}. The migrate command probably wrote somewhere else: check that \
             [sandbox] sets the variable the project reads its database from"
        )
    };
    let path = path.ok_or_else(|| not_migrated(format!("no database at {}", db.display())))?;
    let ran = project::read(&base_dir, migrator)?.migrations.len();
    if ran > 0 {
        let rows = migate_engine_sqlite::count_rows(&path, &migrator.history.query)
            .map_err(|e| not_migrated(format!("its migration history cannot be read ({e})")))?;
        if rows == 0 {
            return Err(not_migrated(format!(
                "its migration history is empty after {ran} base migrations"
            )));
        }
    }
    let before = migate_engine_sqlite::snapshot(&path).map_err(|e| e.to_string())?;

    eprintln!("migate: migrating it to HEAD");
    let out = sh(&migrate, &head_dir, &env)?;
    let after = migate_engine_sqlite::snapshot(&path).map_err(|e| e.to_string())?;
    let mut findings = Vec::new();
    let migrated = out.status.success();
    if !migrated {
        findings.push(Finding::new(
            "MG803",
            Severity::Error,
            "head migrations",
            format!(
                "the migrations fail on a database at {base}; schema changes listed here were left behind:\n{}",
                tail(&out)
            ),
        ));
    }
    findings.extend(classify(&diff(&before, &after)));

    match (&cfg.test, migrated) {
        (Some(test), true) => {
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
    Ok(findings)
}

/// The database file: `{db}` unless `[sandbox] database` names another place; `None` if nothing is there.
fn locate(database: Option<String>, db: &Path) -> Result<Option<PathBuf>, String> {
    let Some(pattern) = database else {
        return Ok(db.is_file().then(|| db.to_path_buf()));
    };
    let pattern = PathBuf::from(pattern);
    let name = pattern
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some((pre, post)) = name.split_once('*') else {
        return Ok(pattern.is_file().then_some(pattern));
    };
    // shortcut: one `*`, in the file name only; a full glob when a project needs one.
    let parent = pattern.parent().unwrap_or(Path::new("."));
    let mut hits: Vec<PathBuf> = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            p.is_file()
                && n.len() >= pre.len() + post.len()
                && n.starts_with(pre)
                && n.ends_with(post)
        })
        .collect();
    match hits.len() {
        0 => Ok(None),
        1 => Ok(hits.pop()),
        _ => Err(format!(
            "[sandbox] database `{}` matches {} files; it must match one",
            pattern.display(),
            hits.len()
        )),
    }
}

// shortcut: commands run through `sh` with no time limit; Windows and a timeout when someone needs them.
fn sh(command: &str, dir: &Path, env: &BTreeMap<String, String>) -> Result<Output, String> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(dir)
        .env_clear()
        .envs(env)
        .output()
        .map_err(|e| format!("cannot run `{command}`: {e}"))
}

/// The last lines of a command's output, for the report.
fn tail(out: &Output) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = text.lines().collect();
    let code = out
        .status
        .code()
        .map_or("a signal".into(), |c| c.to_string());
    format!(
        "(exit {code})\n{}",
        lines[lines.len().saturating_sub(15)..].join("\n")
    )
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

/// A detached worktree at `rev`, removed on drop.
#[derive(Debug)]
struct Checkout(PathBuf, PathBuf);

impl Checkout {
    fn add(root: &Path, rev: &str, dir: &Path) -> Result<Checkout, String> {
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
        Ok(Checkout(dir.to_path_buf(), root.to_path_buf()))
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = git(
            &self.1,
            &["worktree", "remove", "--force", &self.0.to_string_lossy()],
        );
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
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(TempDir(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
