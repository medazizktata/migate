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
fn mg006_duplicate_versions_by_migrator() {
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
    // Wrangler applies both (d1_migrations stores whole file names): a warning, not an error.
    let (code, out, _) = r.check("wrangler-d1");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("warning MG006"), "{out}");
    // Rails refuses to run with two migrations of one version: an error.
    let r = Repo::new();
    r.write(
        "db/migrate/20260101000000_a.rb",
        "class A < ActiveRecord::Migration[8.1]; end\n",
    );
    r.write(
        "db/migrate/20260101000000_b.rb",
        "class B < ActiveRecord::Migration[8.1]; end\n",
    );
    r.commit("base");
    let (code, out, _) = r.check("rails");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("error   MG006"), "{out}");
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

// The sandbox run, on a project shaped like Wrangler D1 whose migrate command is a small Python script (Python's
// standard sqlite3 module, present on CI runners): it applies migrations/*.sql not yet in d1_migrations, statement by
// statement with no transaction around a file, like D1.
const APPLY: &str = r#"import os, pathlib, sqlite3, sys
if "DATABASE_URL" in os.environ:
    sys.exit("DATABASE_URL leaked into the sandbox: " + os.environ["DATABASE_URL"])
con = sqlite3.connect(os.environ.get("D1_PATH", "local.db"), isolation_level=None)
con.execute("PRAGMA foreign_keys = ON")  # D1 enforces foreign keys
con.execute("CREATE TABLE IF NOT EXISTS d1_migrations (id INTEGER PRIMARY KEY, name TEXT UNIQUE)")
done = {r[0] for r in con.execute("SELECT name FROM d1_migrations")}
for f in sorted(pathlib.Path("migrations").glob("*.sql")):
    if f.name not in done:
        con.executescript(f.read_text())
        con.execute("INSERT INTO d1_migrations (name) VALUES (?)", (f.name,))
"#;

/// The base version's "tests": the queries its code runs.
const OLD_APP: &str = r#"import os, sqlite3
con = sqlite3.connect(os.environ["D1_PATH"])
con.execute("INSERT INTO users (email, legacy) VALUES ('a@b.c', 'x')")
print(con.execute("SELECT id, email, legacy FROM users").fetchall())
"#;

const SANDBOX_TOML: &str = r#"migrator = "wrangler-d1"
base = "main"
[sandbox]
migrate = "python3 apply.py"
env = { D1_PATH = "{db}" }
test = "python3 old_app.py"
"#;

/// A repo on `main` with one migration, then a `pr` branch adding `second` as migration 0002.
fn sandbox_repo(config: &str, second: &str) -> Repo {
    sandbox_repo_with(config, OLD_APP, second)
}

/// Rows a production database would hold: a user, and a post when the table exists.
const SEED: &str = r#"import os, sqlite3
c = sqlite3.connect(os.environ["D1_PATH"])
c.execute("INSERT INTO users (id, email) VALUES (7, 's@x')")
if c.execute("SELECT 1 FROM sqlite_schema WHERE name = 'posts'").fetchone():
    c.execute("INSERT INTO posts (user_id) VALUES (7)")
c.commit()
"#;

const INIT: &str = "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT);";

fn sandbox_repo_with(config: &str, old_app: &str, second: &str) -> Repo {
    sandbox_repo_full(config, old_app, INIT, second)
}

fn sandbox_repo_full(config: &str, old_app: &str, first: &str, second: &str) -> Repo {
    let r = Repo::new();
    r.write("migate.toml", config);
    r.write("apply.py", APPLY);
    r.write("old_app.py", old_app);
    r.write("seed.py", SEED);
    r.write("migrations/0001_init.sql", first);
    r.commit("init");
    r.git(&["checkout", "-q", "-b", "pr"]);
    r.write("migrations/0002_change.sql", second);
    r.commit("change");
    r
}

fn sandbox_check(r: &Repo, env: &[(&str, &str)]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .envs(env.iter().copied())
        .args(["check"])
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn sandbox_catches_a_dropped_column_the_old_version_reads() {
    let r = sandbox_repo(SANDBOX_TOML, "ALTER TABLE users DROP COLUMN legacy;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG201") && out.contains("main.users.legacy"),
        "{out}"
    );
    assert!(
        out.contains("MG810") && out.contains("no column named legacy"),
        "{out}"
    );
}

#[test]
fn sandbox_passes_a_safe_addition_and_never_passes_a_connection_url_through() {
    let r = sandbox_repo(SANDBOX_TOML, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(
        &r,
        &[("DATABASE_URL", "postgres://production.example.invalid/db")],
    );
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("MG210") && !out.contains("MG810"), "{out}");
}

#[test]
fn sandbox_reports_a_failed_head_migration_and_what_it_left_behind() {
    let r = sandbox_repo(
        SANDBOX_TOML,
        "ALTER TABLE users ADD COLUMN nickname TEXT;\nALTER TABLE missing ADD COLUMN x TEXT;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG803") && out.contains("no such table: missing"),
        "{out}"
    );
    assert!(
        out.contains("MG210") && out.contains("main.users.nickname"),
        "left behind: {out}"
    );
}

#[test]
fn sandbox_refuses_to_pass_when_the_migrations_went_to_another_database() {
    // The project reads D1_PATH; this config sets another name, so the script writes to its default file instead.
    let misnamed = SANDBOX_TOML.replace("D1_PATH = ", "DB_PATH = ");
    let r = sandbox_repo(&misnamed, "ALTER TABLE users DROP COLUMN legacy;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(err.contains("MG804"), "{err}");
}

#[test]
fn sandbox_without_tests_says_only_the_diff_judged() {
    let no_test = SANDBOX_TOML.replace("test = \"python3 old_app.py\"\n", "");
    let r = sandbox_repo(&no_test, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("MG810") && out.contains("only the schema diff judged"),
        "{out}"
    );
}

#[test]
fn sandbox_refuses_a_database_with_no_migration_history() {
    // Writes the right file but records nothing, as a migrate command pointed at the wrong history table would.
    let config = SANDBOX_TOML.replace(
        "migrate = \"python3 apply.py\"",
        "migrate = \"python3 -c 'import os, sqlite3; sqlite3.connect(os.environ[\\\"D1_PATH\\\"]).execute(\\\"CREATE TABLE d1_migrations (name TEXT)\\\")'\"",
    );
    let r = sandbox_repo(&config, "ALTER TABLE users DROP COLUMN legacy;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("MG804") && err.contains("history is empty"),
        "{err}"
    );
}

#[test]
fn sandbox_leaves_no_worktree_behind() {
    let r = sandbox_repo(SANDBOX_TOML, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    let list = Command::new("git")
        .arg("-C")
        .arg(&r.dir)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .unwrap();
    let count = String::from_utf8_lossy(&list.stdout)
        .matches("worktree ")
        .count();
    assert_eq!(count, 1, "{}", String::from_utf8_lossy(&list.stdout));
}

#[test]
fn sandbox_catches_an_added_column_that_breaks_the_old_versions_own_sql() {
    // GitLab #5171: a nullable column added; the schema diff sees only a note, the old version's `SELECT *` breaks.
    let old_app = r#"import os, sqlite3
con = sqlite3.connect(os.environ["D1_PATH"])
print(con.execute("SELECT * FROM users UNION ALL SELECT id, email, legacy FROM users").fetchall())
"#;
    let r = sandbox_repo_with(
        SANDBOX_TOML,
        old_app,
        "ALTER TABLE users ADD COLUMN nickname TEXT;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("MG210") && out.contains("MG810"), "{out}");
    assert!(out.contains("same number of result columns"), "{out}");
}

#[test]
fn sandbox_works_from_inside_a_git_hook() {
    // Git sets these for hooks; migate's own git calls must not follow them to another repository.
    let r = sandbox_repo(SANDBOX_TOML, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let env = [
        ("GIT_INDEX_FILE", ".git/index"),
        ("GIT_DIR", "/nonexistent/.git"),
    ];
    let (code, out, err) = sandbox_check(&r, &env);
    assert_eq!(code, 0, "{out}{err}");
}

#[test]
fn sandbox_refuses_a_test_command_that_never_uses_the_database() {
    let r = sandbox_repo(
        &SANDBOX_TOML.replace("python3 old_app.py", "true"),
        "ALTER TABLE users DROP COLUMN legacy;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("MG813") && err.contains("never used the migrated database"),
        "{err}"
    );
}

#[test]
fn sandbox_refuses_a_test_command_that_cannot_start() {
    let r = sandbox_repo(
        &SANDBOX_TOML.replace("python3 old_app.py", "no-such-test-runner"),
        "SELECT 1;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("MG813") && err.contains("before any new migration"),
        "{err}"
    );
}

#[test]
fn sandbox_refuses_tests_that_rebuild_the_database() {
    // The tests empty the file and migrate it again from the base chain, as Django without --keepdb does.
    let test = r#"python3 -c 'import os; open(os.environ[\"D1_PATH\"], \"w\").close()' && python3 apply.py && python3 old_app.py"#;
    let r = sandbox_repo(
        &SANDBOX_TOML.replace("python3 old_app.py", test),
        "ALTER TABLE users ADD COLUMN nickname TEXT;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("MG813") && err.contains("rebuilt the database"),
        "{err}"
    );
}

#[test]
fn sandbox_catches_a_data_migration_that_deletes_rows() {
    // The seed leaves a user behind, as production would have one; the migration deletes it.
    let seeded = SANDBOX_TOML.replace("test = ", "seed = \"python3 seed.py\"\ntest = ");
    let r = sandbox_repo(&seeded, "DELETE FROM users;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG105")
            && out.contains("main.users")
            && out.contains("1 rows before the migrations, 0 after"),
        "{out}"
    );
}

#[test]
fn sandbox_catches_a_rebuild_that_cascade_deletes_children() {
    // drizzle-orm#4938: SQLite's copy-and-swap rebuild of a parent drops it, and ON DELETE CASCADE empties the child.
    let first = "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT);
CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users ON DELETE CASCADE);";
    let rebuild = "CREATE TABLE users_new (id INTEGER PRIMARY KEY, email TEXT, legacy TEXT);
INSERT INTO users_new SELECT id, email, legacy FROM users;
DROP TABLE users;
ALTER TABLE users_new RENAME TO users;";
    let config = SANDBOX_TOML.replace("test = ", "seed = \"python3 seed.py\"\ntest = ");
    let r = sandbox_repo_full(&config, OLD_APP, first, rebuild);
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG105")
            && out.contains("main.posts")
            && out.contains("1 rows before the migrations, 0 after"),
        "{out}"
    );
}

#[test]
fn migrations_come_from_git_not_from_ignored_folders() {
    let r = Repo::new();
    r.write(".gitignore", "env/\n");
    r.write("migrations/0001_init.sql", "CREATE TABLE a (id INTEGER);");
    // A virtualenv's packages ship their own migrations; a CI checkout never has them.
    r.write(
        "env/lib/site-packages/pkg/migrations/0002_pkg.sql",
        "CREATE TABLE b (id INTEGER);",
    );
    r.commit("init");
    // A new migration, not yet added to git, is the project's own and is listed.
    r.write("migrations/0003_new.sql", "CREATE TABLE c (id INTEGER);");
    let out = r.record("wrangler-d1", &["--all"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("0001_init.sql") && text.contains("0003_new.sql"),
        "{text}"
    );
    assert!(!text.contains("0002_pkg.sql"), "{text}");
}

#[test]
fn payload_seed_files_are_not_migrations() {
    let r = Repo::new();
    r.write(
        "src/migrations/20240101_000000_init.ts",
        "export async function up() {}\n",
    );
    r.write(
        "src/seed/20240101_000000_demo_data.ts",
        "export const demo = [];\n",
    );
    r.commit("init");
    let out = r.record("payload", &["--all"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("20240101_000000_init") && !text.contains("demo_data"),
        "{text}"
    );
}

/// A base version that no longer reads or writes `legacy` (Rails' ignored_columns, done in an earlier release).
const OLD_APP_WITHOUT_LEGACY: &str = r#"import os, sqlite3
con = sqlite3.connect(os.environ["D1_PATH"])
con.execute("INSERT INTO users (email) VALUES ('a@b.c')")
print(con.execute("SELECT id, email FROM users").fetchall())
"#;

const DROP_WITH_CONTRACT: &str = "-- migate: contract drop-column users.legacy -- unused since v42 (ignored_columns)\nALTER TABLE users DROP COLUMN legacy;";

#[test]
fn a_contract_accepts_an_intended_drop_the_old_version_no_longer_uses() {
    // A marker in a migration already merged does not count (it would be stale here, and fail).
    let first =
        format!("{INIT}\n-- migate: contract drop-column users.gone -- an old release's marker");
    let r = sandbox_repo_full(
        SANDBOX_TOML,
        OLD_APP_WITHOUT_LEGACY,
        &first,
        DROP_WITH_CONTRACT,
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("MG201")
            && out.contains("accepted by the contract at migrations/0002_change.sql:1"),
        "{out}"
    );
    assert!(
        out.contains("unused since v42") && !out.contains("MG901"),
        "{out}"
    );
}

#[test]
fn a_contract_cannot_cover_a_column_the_old_version_still_uses() {
    let r = sandbox_repo(SANDBOX_TOML, DROP_WITH_CONTRACT);
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("error   MG810"),
        "the tests' evidence stands: {out}"
    );
}

#[test]
fn a_contract_without_a_reason_or_matching_nothing_fails() {
    for second in [
        "-- migate: contract drop-column users.legacy\nALTER TABLE users DROP COLUMN legacy;",
        "-- migate: contract drop-column users.nickname -- stale\nALTER TABLE users DROP COLUMN legacy;",
    ] {
        let r = sandbox_repo_with(SANDBOX_TOML, OLD_APP_WITHOUT_LEGACY, second);
        let (code, out, err) = sandbox_check(&r, &[]);
        assert_eq!(code, 2, "{second}: {out}{err}");
        assert!(out.contains("error   MG901"), "{second}: {out}");
    }
}
