//! Reading a file as it is on the base branch, through the `git` command (no git library in the project).

use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub enum AtBase {
    Found(String),
    /// The base branch exists but has no such file (the ledger was added in this change).
    Absent,
}

/// `rel` is relative to `root`. An unreadable base (a missing ref, as in a shallow CI checkout) is an error, never an
/// empty ledger: that would let a pull request rewrite the ledger unchecked.
pub fn file_at_base(root: &Path, base: &str, rel: &str) -> Result<AtBase, String> {
    let commit = format!("{base}^{{commit}}");
    let ok = git(root, &["rev-parse", "--verify", "--quiet", &commit])?
        .status
        .success();
    if !ok {
        return Err(format!(
            "cannot read the base branch `{base}`: the ref is missing. In CI, fetch it (for example \
             `git fetch origin main`, or `fetch-depth: 0` on actions/checkout)"
        ));
    }
    let spec = format!("{base}:./{rel}");
    if !git(root, &["cat-file", "-e", &spec])?.status.success() {
        return Ok(AtBase::Absent);
    }
    let out = git(root, &["show", &spec])?;
    if !out.status.success() {
        return Err(format!(
            "git show {spec}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(AtBase::Found(
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

fn git(root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))
}
