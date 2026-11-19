//! End to end: real git repositories in a temp directory, the real `migate` binary, exit codes and output.

// The whole file is test code; clippy's allow-unwrap-in-tests covers #[test] functions but not their helpers.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new() -> Repo {
        let dir = std::env::temp_dir().join(format!(
            "migate-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let repo = Repo { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.dir.join(rel)).unwrap()
    }

    fn delete(&self, rel: &str) {
        fs::remove_file(self.dir.join(rel)).unwrap();
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn migate(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_migate"))
            .current_dir(&self.dir)
            .args(args)
            .output()
            .unwrap()
    }

    /// `migate check` against `main`: (exit code, stdout, stderr).
    fn check(&self, migrator: &str) -> (i32, String, String) {
        let out = self.migate(&["check", "--migrator", migrator, "--base", "main"]);
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn record(&self, migrator: &str, ids: &[&str]) -> Output {
        let mut args = vec!["ledger", "record", "--migrator", migrator];
        args.extend_from_slice(ids);
        self.migate(&args)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const ORDERS: &str = "db/migrate/20260101000000_create_orders.rb";
const TOTAL: &str = "db/migrate/20260102000000_add_total.rb";

/// A Rails project whose two migrations production ran, recorded in the ledger on `main`.
fn rails() -> Repo {
    let r = Repo::new();
    r.write(ORDERS, "class CreateOrders < ActiveRecord::Migration[8.1]\n  def change\n    create_table :orders\n  end\nend\n");
    r.write(TOTAL, "class AddTotal < ActiveRecord::Migration[8.1]\n  def change\n    add_column :orders, :total, :integer\n  end\nend\n");
    let out = r.record(
        "rails",
        &["20260101000000_create_orders", "20260102000000_add_total"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    r.commit("base");
    r
}

#[test]
fn a_clean_project_passes() {
    let r = rails();
    let (code, out, _) = r.check("rails");
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("0 error(s), 0 warning(s), 0 note(s) in 2 migration(s)"),
        "{out}"
    );
}

#[test]
fn a_project_without_a_ledger_yet_passes() {
    let r = Repo::new();
    r.write(ORDERS, "create_table :orders\n");
    r.commit("adopting migate");
    let (code, out, err) = r.check("rails");
    assert_eq!(code, 0, "{out}{err}");
}

#[test]
fn mg001_editing_an_applied_migration_fails() {
    let r = rails();
    r.write(TOTAL, &r.read(TOTAL).replace(":integer", ":bigint"));
    let (code, out, _) = r.check("rails");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("MG001  20260102000000_add_total"), "{out}");
}

#[test]
fn mg002_removing_an_applied_migration_fails() {
    let r = rails();
    r.delete(TOTAL);
    let (code, out, _) = r.check("rails");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("MG002"), "{out}");
}

#[test]
fn mg003_rewriting_the_ledger_fails() {
    let r = rails();
    let ledger = r.read("migate-ledger.json");
    let first = ledger.find("\"sha256\": \"").unwrap() + 11;
    let mut rewritten = ledger.clone();
    rewritten.replace_range(
        first..first + 1,
        if &ledger[first..first + 1] == "0" {
            "1"
        } else {
            "0"
        },
    );
    r.write("migate-ledger.json", &rewritten);
    let (code, out, _) = r.check("rails");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("MG003"), "{out}");
}

#[test]
fn mg007_a_comment_only_edit_is_a_note() {
    let r = rails();
    r.write(
        TOTAL,
        &format!("# why: totals for invoices\n{}", r.read(TOTAL)),
    );
    let (code, out, _) = r.check("rails");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("note    MG007"), "{out}");
}

#[test]
fn mg004_an_older_migration_merged_late_warns_for_rails() {
    let r = rails();
    r.write(
        "db/migrate/20251231000000_late_branch.rb",
        "add_column :orders, :note, :text\n",
    );
    let (code, out, _) = r.check("rails");
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("warning MG004  20251231000000_late_branch"),
        "{out}"
    );
}

#[test]
fn mg004_an_older_migration_is_an_error_where_drizzle_would_skip_it() {
    let r = Repo::new();
    let journal = |entries: &[(&str, u64)]| {
        let list: Vec<String> = entries
            .iter()
            .map(|(tag, when)| {
                format!(
                    r#"{{"idx":0,"version":"6","when":{when},"tag":"{tag}","breakpoints":true}}"#
                )
            })
            .collect();
        format!(
            r#"{{"version":"7","dialect":"sqlite","entries":[{}]}}"#,
            list.join(",")
        )
    };
    r.write("drizzle/0000_init.sql", "CREATE TABLE a (id integer);\n");
    r.write("drizzle/0001_b.sql", "ALTER TABLE a ADD b integer;\n");
    r.write(
        "drizzle/meta/_journal.json",
        &journal(&[("0000_init", 1000), ("0001_b", 3000)]),
    );
    assert!(
        r.record("drizzle", &["0000_init", "0001_b"])
            .status
            .success()
    );
    r.commit("base");
    r.write("drizzle/0002_old.sql", "ALTER TABLE a ADD c integer;\n");
    r.write(
        "drizzle/meta/_journal.json",
        &journal(&[("0000_init", 1000), ("0001_b", 3000), ("0002_old", 2000)]),
    );
    let (code, out, _) = r.check("drizzle");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("error   MG004  0002_old"), "{out}");
}

#[test]
fn mg006_duplicate_versions_fail() {
    let r = Repo::new();
    r.write("migrations/0001_init.sql", "CREATE TABLE a (id integer);\n");
    r.write(
        "migrations/0002_add_b.sql",
        "ALTER TABLE a ADD b integer;\n",
    );
    r.write(
        "migrations/0002_add_c.sql",
        "ALTER TABLE a ADD c integer;\n",
    );
    r.commit("base");
    let (code, out, _) = r.check("wrangler-d1");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("MG006"), "{out}");
}

#[test]
fn an_unreadable_base_is_exit_3_in_a_shallow_clone() {
    // CI often checks out only the pull request's branch, one commit deep: origin/main does not exist there.
    let r = rails();
    r.git(&["checkout", "-q", "-b", "feature"]);
    r.write(
        "db/migrate/20260103000000_add_note.rb",
        "add_column :orders, :note, :text\n",
    );
    r.commit("feature");
    let clone = std::env::temp_dir().join(format!(
        "migate-cli-clone-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&clone);
    let url = format!("file://{}", r.dir.display());
    let ok = Command::new("git")
        .args([
            "clone",
            "-q",
            "--depth",
            "1",
            "--single-branch",
            "--branch",
            "feature",
            &url,
        ])
        .arg(&clone)
        .status()
        .unwrap();
    assert!(ok.success());
    let out = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&clone)
        .args(["check", "--migrator", "rails", "--base", "origin/main"])
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&clone);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(3), "{err}");
    assert!(
        err.contains("cannot read the base branch `origin/main`"),
        "{err}"
    );
}

#[test]
fn an_unknown_migrator_is_exit_3() {
    let r = Repo::new();
    let (code, _, err) = r.check("liquibase");
    assert_eq!(code, 3);
    assert!(err.contains("unknown migrator `liquibase`"), "{err}");
}

#[test]
fn an_old_format_ledger_is_exit_3_with_a_hint() {
    let r = Repo::new();
    r.write(
        "migate-ledger.json",
        r#"{"20260101000000_create_orders": "abc123"}"#,
    );
    let (code, _, err) = r.check("rails");
    assert_eq!(code, 3);
    assert!(err.contains("older {name: hash} format"), "{err}");
}

#[test]
fn recording_is_append_only() {
    let r = rails();
    // Recording the same file again is a no-op.
    assert!(
        r.record("rails", &["20260102000000_add_total"])
            .status
            .success()
    );
    // Recording an edited file under an existing id is refused, and the ledger is left untouched.
    let before = r.read("migate-ledger.json");
    r.write(TOTAL, "add_column :orders, :total, :bigint\n");
    let out = r.record("rails", &["20260102000000_add_total"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("append-only"));
    assert_eq!(r.read("migate-ledger.json"), before);
}

#[test]
fn a_usage_error_is_exit_3_not_2() {
    // Exit 2 means "migration errors found"; a typo in the command must not look like one.
    let r = Repo::new();
    let out = r.migate(&["check", "--migrator", "rails", "--no-such-flag"]);
    assert_eq!(out.status.code(), Some(3));
    let help = r.migate(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
}

#[test]
fn the_config_file_supplies_the_settings_and_flags_win() {
    let r = rails();
    r.write("migate.toml", "migrator = \"rails\"\nbase = \"main\"\n");
    let out = r.migate(&["check"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("migrator: rails, base: main"), "{text}");
    // A flag overrides the file: the wrong migrator reads no Rails migrations.
    let out = r.migate(&["check", "--migrator", "payload"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("in 0 migration(s) (migrator: payload"));
    // Unknown keys are refused, so a typo cannot silently change nothing. The file is otherwise valid: without the
    // refusal this check would pass (exit 0) while ignoring the misspelled `ledgr`.
    r.write(
        "migate.toml",
        "migrator = \"rails\"\nbase = \"main\"\nledgr = \"other.json\"\n",
    );
    let out = r.migate(&["check"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown field `ledgr`"));
}

#[test]
fn no_migrator_anywhere_is_exit_3() {
    let r = Repo::new();
    let out = r.migate(&["check", "--base", "main"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no migrator"));
}

#[test]
fn json_output_lists_findings_with_their_files() {
    let r = rails();
    r.write(TOTAL, &r.read(TOTAL).replace(":integer", ":bigint"));
    let out = r.migate(&[
        "check",
        "--migrator",
        "rails",
        "--base",
        "main",
        "--format",
        "json",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["checked"], 2);
    assert_eq!(v["summary"]["errors"], 1);
    assert_eq!(v["findings"][0]["rule"], "MG001");
    assert_eq!(v["findings"][0]["file"], TOTAL);
}

#[test]
fn sarif_output_has_rules_results_and_stable_fingerprints() {
    let r = rails();
    r.write(TOTAL, &r.read(TOTAL).replace(":integer", ":bigint"));
    let out = r.migate(&[
        "check",
        "--migrator",
        "rails",
        "--base",
        "main",
        "--format",
        "sarif",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["version"], "2.1.0");
    let run = &v["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "migate");
    assert_eq!(run["tool"]["driver"]["rules"][0]["id"], "MG001");
    assert_eq!(run["results"][0]["ruleId"], "MG001");
    assert_eq!(run["results"][0]["level"], "error");
    assert_eq!(
        run["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        TOTAL
    );
    assert_eq!(
        run["results"][0]["partialFingerprints"]["migate/v1"],
        "MG001:20260102000000_add_total"
    );
}

#[test]
fn record_all_adopts_every_migration_and_conflicts_with_ids() {
    let r = Repo::new();
    r.write(ORDERS, "create_table :orders\n");
    r.write(TOTAL, "add_column :orders, :total, :integer\n");
    let out = r.migate(&["ledger", "record", "--migrator", "rails", "--all"]);
    assert!(out.status.success());
    let ledger = r.read("migate-ledger.json");
    assert!(
        ledger.contains("20260101000000_create_orders")
            && ledger.contains("20260102000000_add_total")
    );
    let both = r.migate(&[
        "ledger",
        "record",
        "--migrator",
        "rails",
        "--all",
        "20260101000000_create_orders",
    ]);
    assert_eq!(
        both.status.code(),
        Some(3),
        "--all with ids is a usage error"
    );
}
