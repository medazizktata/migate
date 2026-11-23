//! Reading a project's migration files (an adapter: the core never touches the file system).
//!
//! The candidates are git's files (tracked, plus new ones not yet added), not a walk of the disk: an untracked folder
//! such as a virtualenv holds migrations that a CI checkout never has (test drive 1, `docs/end-to-end-testing/2026-11-19-test-drive-1.md`).

use std::fs;
use std::path::Path;

use migate_core::ledger::MigrationFile;
use migate_core::order::Migration;
use migate_migrators::Migrator;

/// Whether a path's folder never holds the project's own migrations, even when git does not ignore it: Git's own, or a
/// tool's (dependencies, builds and state, listed in the bundled tools.toml).
fn skipped(part: &str) -> bool {
    part == ".git"
        || migate_migrators::tools()
            .skip_dirs
            .iter()
            .any(|d| d == part)
}

/// The project's migrations, and the files that hold them (a migration whose file is missing has no entry in `files`).
#[derive(Debug)]
pub struct Project {
    pub migrations: Vec<Migration>,
    pub files: Vec<MigrationFile>,
}

/// The project's text files at `root`, path (relative, with '/') and contents: what the base version's code says
/// (MG212). Files over 1 MB or not UTF-8 are left out.
pub fn source_files(root: &Path) -> Result<Vec<(String, String)>, String> {
    Ok(git_files(root)?
        .into_iter()
        .filter(|rel| {
            std::fs::metadata(root.join(rel)).is_ok_and(|m| m.is_file() && m.len() <= 1 << 20)
        })
        .filter_map(|rel| {
            let text = std::fs::read_to_string(root.join(&rel)).ok()?;
            Some((rel, text))
        })
        .collect())
}

pub fn read(root: &Path, migrator: &Migrator) -> Result<Project, String> {
    let mut matched: Vec<String> = git_files(root)?
        .into_iter()
        .filter(|rel| !rel.split('/').any(skipped))
        .filter(|rel| migrator.matches(rel) && root.join(rel).is_file())
        .collect();
    matched.sort();
    let inputs: Vec<(String, String)> = matched
        .iter()
        .map(|rel| {
            let bytes = fs::read(root.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
            Ok((rel.clone(), String::from_utf8_lossy(&bytes).into_owned()))
        })
        .collect::<Result<_, String>>()?;
    let found = migrator
        .migration_files(&inputs)
        .map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for (migration, path) in &found {
        if let Ok(bytes) = fs::read(root.join(path)) {
            files.push(MigrationFile {
                id: migration.id.clone(),
                path: path.clone(),
                bytes,
            });
        }
    }
    Ok(Project {
        migrations: found.into_iter().map(|(m, _)| m).collect(),
        files,
    })
}

/// Folders that would hold this migrator's migrations as `migrations_dir`: a project with one set per database keeps
/// each in a folder of its own, which the migrator's layout alone does not find.
pub fn migration_dirs(root: &Path, migrator: &Migrator) -> Result<Vec<String>, String> {
    let files: Vec<String> = git_files(root)?
        .into_iter()
        .filter(|rel| !rel.split('/').any(skipped))
        .collect();
    let folders: std::collections::BTreeSet<&str> = files
        .iter()
        .filter_map(|f| f.rsplit_once('/').map(|(dir, _)| dir))
        .filter(|dir| dir.split('/').any(|part| part.contains("migration")))
        .collect();
    Ok(folders
        .into_iter()
        .filter(|dir| {
            let mut m = migrator.clone();
            m.set_migrations_dir(dir);
            files.iter().any(|f| m.matches(f))
        })
        .map(String::from)
        .collect())
}

/// The migrations at a git revision (the base branch), read from git without a checkout.
pub fn migrations_at(
    root: &Path,
    rev: &str,
    migrator: &Migrator,
) -> Result<Vec<Migration>, String> {
    let listed = crate::git::git(
        root,
        &["ls-tree", "-r", "-z", "--name-only", rev, "--", "."],
    )?;
    if !listed.status.success() {
        return Err(format!(
            "cannot list the files at `{rev}`: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        ));
    }
    let mut inputs = Vec::new();
    for rel in String::from_utf8_lossy(&listed.stdout).split('\0') {
        if rel.is_empty() || rel.split('/').any(skipped) || !migrator.matches(rel) {
            continue;
        }
        let shown = crate::git::git(root, &["show", &format!("{rev}:./{rel}")])?;
        if !shown.status.success() {
            return Err(format!("cannot read {rel} at `{rev}`"));
        }
        inputs.push((
            rel.to_string(),
            String::from_utf8_lossy(&shown.stdout).into_owned(),
        ));
    }
    inputs.sort();
    let found = migrator
        .migration_files(&inputs)
        .map_err(|e| e.to_string())?;
    Ok(found.into_iter().map(|(m, _)| m).collect())
}

/// Paths relative to `root`: tracked files, and untracked ones git does not ignore.
fn git_files(root: &Path) -> Result<Vec<String>, String> {
    let out = crate::git::git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--deduplicate",
            "--",
            ".",
        ],
    )?;
    if !out.status.success() {
        return Err(format!(
            "cannot list the project's files with git (migate needs a git repository): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}
