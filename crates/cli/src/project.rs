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
