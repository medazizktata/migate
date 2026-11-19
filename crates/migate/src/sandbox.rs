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
use migate_core::diff::{classify, diff};
use migate_core::{Finding, Severity};
use migate_migrators::Migrator;

use crate::config::Sandbox;
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

pub fn run(
    cfg: &Sandbox,
    migrator: &Migrator,
    root: &Path,
    base: &str,
) -> Result<Vec<Finding>, String> {
    on_signals();
    let tmp = TempDir::create()?;
    sweep(root, &tmp.0);
    let dir = tmp.0.join("db");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let db = dir.join("sandbox.db");
    let name = cfg.database_name.clone().unwrap_or_default();
    let vars = |s: &str| {
        s.replace("{db}", &db.to_string_lossy())
            .replace("{dir}", &dir.to_string_lossy())
            .replace("{database}", &name)
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
    let (mut markers, mut bad_markers) = (Vec::new(), Vec::new());
    for (path, text) in &added {
        let (m, bad) = contract::parse(path, text);
        markers.extend(m);
        bad_markers.extend(bad);
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
    // The migrator's location goes with its own command; a project that replaces the command says where it writes.
    let tool_location = migrator
        .migrate
        .database
        .as_deref()
        .filter(|_| cfg.migrate.is_none());
    let pattern = cfg.database.as_deref().or(tool_location).map(vars);
    let path = locate(pattern, &db, &migrator.history.query)?;
    let not_migrated = |why: String| {
        format!(
            "MG804 sandbox-not-migrated: {why}. The migrate command probably wrote somewhere else: check that it is \
             told where the database is ({{db}} in a variable under [sandbox] env, or {{dir}} in a flag such as \
             --persist-to), and that [sandbox] database names the file it writes"
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
    let sql_err = |e: migate_engine_sqlite::Error| e.to_string();
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
        let out = sh(&vars(test), &base_dir, &env)?;
        if !out.status.success() {
            return Err(invalid(format!(
                "the base version's tests fail on the database at {base}, before any new migration, so their \
                 result after one would mean nothing:\n{}",
                tail(&out)
            )));
        }
    }
    let rows_before = migate_engine_sqlite::row_counts(&path).map_err(sql_err)?;

    eprintln!("migate: migrating it to HEAD");
    let out = sh(&migrate, &head_dir, &migrate_env)?;
    let after = migate_engine_sqlite::snapshot(&path).map_err(|e| e.to_string())?;
    let mut findings = Vec::new();
    let migrated = out.status.success();
    if !migrated {
        findings.push(Finding::new(
            "MG803",
            Severity::Error,
            "head migrations",
            format!(
                "the migrations fail on a database at {base}; any schema change they made before failing is a finding \
                 of its own:\n{}",
                tail(&out)
            ),
        ));
    }
    findings.extend(classify(&diff(&before, &after)));
    for (table, now) in migate_engine_sqlite::row_counts(&path).map_err(sql_err)? {
        if let Some(&was) = rows_before.get(&table)
            && now < was
        {
            findings.push(Finding::new(
                "MG105",
                Severity::Error,
                format!("main.{table}"),
                format!("{was} rows before the migrations, {now} after"),
            ));
        }
    }

    match (&cfg.test, migrated) {
        (Some(test), true) => {
            let history = migate_engine_sqlite::count_rows(&path, &migrator.history.query)
                .map_err(sql_err)?;
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
                let still =
                    migate_engine_sqlite::count_rows(&path, &migrator.history.query).unwrap_or(0);
                if still < history {
                    return Err(invalid(format!(
                        "the migration history went from {history} rows to {still} during the tests: they rebuilt the \
                     database instead of using the migrated one"
                    )));
                }
                migate_engine_sqlite::drop_all_but(&path, &migrator.history.query)
                    .map_err(sql_err)?;
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
    Ok(findings)
}

/// Point a finding at the added migration that names its object (the latest one, at the first line naming it), else at
/// the latest added migration. Findings stay unplaced when the change adds no migration.
fn place(f: Finding, added: &[(String, String)]) -> Finding {
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

/// The database file: `{db}`, or the file `database` names (one `*` in the file name may match several; the one holding
/// the migration history is the database).
fn locate(database: Option<String>, db: &Path, history: &str) -> Result<Option<PathBuf>, String> {
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
    if hits.len() > 1 {
        hits.retain(|p| migate_engine_sqlite::count_rows(p, history).is_ok());
    }
    match hits.len() {
        0 => Ok(None),
        1 => Ok(hits.pop()),
        _ => Err(format!(
            "the database pattern `{}` matches {} files holding a migration history; it must match one",
            pattern.display(),
            hits.len()
        )),
    }
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

/// What an interrupted run must undo: running commands' process groups, worktrees (with their repository), folders.
/// The lock is held while each is made or undone, and by the signal handler until it exits, so the two never race.
#[derive(Debug)]
struct Live {
    groups: Vec<u32>,
    worktrees: Vec<(PathBuf, PathBuf)>,
    dirs: Vec<PathBuf>,
}

static LIVE: Mutex<Live> = Mutex::new(Live {
    groups: Vec::new(),
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
