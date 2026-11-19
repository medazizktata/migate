//! Reading a project's migration files from disk (an adapter: the core never touches the file system).

use std::fs;
use std::path::Path;

use migate_core::ledger::MigrationFile;
use migate_core::order::Migration;
use migate_migrators::Migrator;

/// Directories that never hold migrations and can be huge.
const SKIP: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    ".venv",
    "venv",
    "dist",
    "build",
    ".next",
    ".wrangler",
    "__pycache__",
];

/// The project's migrations, and the files that hold them (a migration whose file is missing has no entry in `files`).
#[derive(Debug)]
pub struct Project {
    pub migrations: Vec<Migration>,
    pub files: Vec<MigrationFile>,
}

pub fn read(root: &Path, migrator: &Migrator) -> Result<Project, String> {
    let mut matched = Vec::new();
    walk(root, root, migrator, &mut matched)?;
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

fn walk(root: &Path, dir: &Path, migrator: &Migrator, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if kind.is_dir() {
            if !SKIP.contains(&entry.file_name().to_string_lossy().as_ref()) {
                walk(root, &path, migrator, out)?;
            }
        } else if kind.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if migrator.matches(&rel) {
                out.push(rel);
            }
        }
    }
    Ok(())
}
