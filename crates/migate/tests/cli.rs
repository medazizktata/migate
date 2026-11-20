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
        // A pre-commit hook exports these; they would point the test's git at the hook's repository.
        let status = Command::new("git")
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
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
        ORDERS,
        "class CreateOrders < ActiveRecord::Migration[8.1]; end\n",
    );
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
    // A flag overrides the file: the wrong migrator finds no Rails migrations, so nothing was checked.
    let out = r.migate(&["check", "--migrator", "payload"]);
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("no payload migrations") && err.contains("files here match: rails"),
        "{err}"
    );
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
for leak in ("DATABASE_URL", "BUNDLE_GEMS__EXAMPLE__COM"):
    if leak in os.environ:
        sys.exit(leak + " leaked into the sandbox: " + os.environ[leak])
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
        &[
            ("DATABASE_URL", "postgres://production.example.invalid/db"),
            ("BUNDLE_GEMS__EXAMPLE__COM", "user:secret"),
        ],
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
        out.contains("MG810")
            && out.contains("only the schema diff judged")
            && out.contains(", sandbox: sqlite 3."),
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
        !out.contains("MG106"),
        "lost rows are reported once, not again per column: {out}"
    );
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

fn worktrees(r: &Repo) -> usize {
    let list = Command::new("git")
        .arg("-C")
        .arg(&r.dir)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&list.stdout)
        .matches("worktree ")
        .count()
}

fn run_folders(pid: u32) -> usize {
    let prefix = format!("migate-run-{pid}-");
    fs::read_dir(std::env::temp_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .count()
}

#[test]
fn an_interrupted_sandbox_run_stops_its_command_and_removes_everything() {
    // The setup command is slow; it would leave a file behind if it outlived the interrupt.
    // `{dir}` is the run's database folder, removed with the run, so the file lands in the repository instead.
    let r = sandbox_repo(SANDBOX_TOML, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (started, late) = (r.dir.join("started"), r.dir.join("late"));
    r.write(
        "migate.toml",
        &SANDBOX_TOML.replace(
            "test = ",
            &format!(
                "setup = \"touch {} && sleep 2 && touch {}\"\ntest = ",
                started.display(),
                late.display()
            ),
        ),
    );
    r.commit("slow setup");
    let child = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .arg("check")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // Interrupt only once the slow command runs, or there is nothing for the handler to stop.
    for _ in 0..200 {
        if started.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(started.exists(), "the setup command never started");
    assert_eq!(
        worktrees(&r),
        3,
        "both checkouts exist before the interrupt"
    );
    let pid = child.id();
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("interrupted"));
    assert_eq!(worktrees(&r), 1);
    assert_eq!(run_folders(pid), 0);
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(!late.exists(), "the setup command outlived the interrupt");
}

#[test]
fn a_run_cleans_up_what_a_killed_run_left_behind() {
    let r = sandbox_repo(SANDBOX_TOML, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    // What `kill -9` leaves: a run folder of a process that no longer exists, holding a registered worktree.
    let dead = 4_294_967; // above any pid_max
    let left = std::env::temp_dir().join(format!("migate-run-{dead}-{}", std::process::id()));
    r.git(&[
        "worktree",
        "add",
        "--quiet",
        "--detach",
        &left.join("base").to_string_lossy(),
        "main",
    ]);
    assert_eq!(worktrees(&r), 2);
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(!left.exists(), "the dead run's folder is still there");
    assert_eq!(worktrees(&r), 1);
}

#[test]
fn sandbox_finds_the_database_among_files_the_tool_writes() {
    // Like Wrangler's --persist-to folder: the database under a name migate cannot know, and a metadata.sqlite beside it.
    let config = SANDBOX_TOML
        .replace("migrate = \"python3 apply.py\"", "migrate = \"python3 apply.py && python3 -c 'import os, sqlite3; sqlite3.connect(os.environ[\\\"META\\\"]).execute(\\\"CREATE TABLE IF NOT EXISTS mf (k)\\\")'\"\ndatabase = \"{dir}/*.sqlite\"")
        .replace("env = { D1_PATH = \"{db}\" }", "env = { D1_PATH = \"{dir}/3f2a9c.sqlite\", META = \"{dir}/metadata.sqlite\" }");
    let r = sandbox_repo(&config, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("MG210"), "{out}");
}

#[test]
fn schema_findings_point_at_the_migration_that_made_them() {
    let r = sandbox_repo_with(
        SANDBOX_TOML,
        OLD_APP_WITHOUT_LEGACY,
        "ALTER TABLE users ADD COLUMN nickname TEXT;",
    );
    r.write(
        "migrations/0003_drop.sql",
        "-- legacy_notes stays; this drops the other one\nALTER TABLE users DROP COLUMN legacy;\n",
    );
    r.commit("drop");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("main.users.legacy (migrations/0003_drop.sql:2): column dropped"),
        "{out}"
    );
    assert!(
        out.contains("main.users.nickname (migrations/0002_change.sql:1): column added"),
        "{out}"
    );
    let json = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .args(["check", "--format", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let dropped = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule"] == "MG201")
        .unwrap()
        .clone();
    assert_eq!(
        (dropped["file"].as_str(), dropped["line"].as_u64()),
        (Some("migrations/0003_drop.sql"), Some(2))
    );
}

#[test]
fn a_run_that_could_not_check_still_reports_to_machines() {
    let r = sandbox_repo(
        &SANDBOX_TOML.replace("python3 old_app.py", "true"),
        "SELECT 1;",
    );
    let run = |format: &str| {
        Command::new(env!("CARGO_BIN_EXE_migate"))
            .current_dir(&r.dir)
            .args(["check", "--format", format])
            .output()
            .unwrap()
    };
    let json = run("json");
    assert_eq!(json.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["outcome"], "could-not-check");
    assert!(v["error"].as_str().unwrap().contains("MG813"), "{v}");
    let sarif = run("sarif");
    assert_eq!(sarif.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_slice(&sarif.stdout).unwrap();
    let invocation = &v["runs"][0]["invocations"][0];
    assert_eq!(invocation["executionSuccessful"], false);
    assert!(
        invocation["toolExecutionNotifications"][0]["message"]["text"]
            .as_str()
            .unwrap()
            .contains("MG813")
    );
}

#[test]
fn a_migrator_that_finds_no_migrations_has_checked_nothing() {
    // Wrangler-D1 files, checked as Rails: nothing matches, which must never read as a pass.
    let r = Repo::new();
    r.write("migrations/0001_init.sql", "CREATE TABLE a (id INTEGER);");
    r.commit("init");
    let (code, out, err) = r.check("rails");
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("no rails migrations") && err.contains("wrangler-d1"),
        "{err}"
    );
    let out = r.record("rails", &["--all"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !r.dir.join("migate-ledger.json").exists(),
        "an empty ledger was written"
    );
}

#[test]
fn an_old_branch_migration_is_flagged_before_any_ledger_exists() {
    let r = Repo::new();
    r.write("migrations/0001_init.sql", "CREATE TABLE a (id INTEGER);");
    r.write("migrations/0003_c.sql", "CREATE TABLE c (id INTEGER);");
    r.commit("base");
    r.git(&["checkout", "-q", "-b", "pr"]);
    r.write("migrations/0002_b.sql", "CREATE TABLE b (id INTEGER);");
    r.commit("late branch");
    let (code, out, err) = r.check("wrangler-d1");
    assert_eq!(code, 0, "Wrangler warns: {out}{err}");
    assert!(
        out.contains("warning MG004") && out.contains("0002_b.sql"),
        "{out}"
    );
    r.git(&["checkout", "-q", "main"]);
    r.git(&["checkout", "-q", "-b", "pr2"]);
    r.write("migrations/0004_d.sql", "CREATE TABLE d (id INTEGER);");
    r.commit("in order");
    let (_, out, _) = r.check("wrangler-d1");
    assert!(!out.contains("MG004"), "{out}");
}

#[test]
fn temp_files_the_project_makes_go_with_the_run() {
    // Test runners and Wrangler write temp folders; they must not outlive the run.
    let name = format!("migate-leftover-probe-{}", std::process::id());
    let probe = format!(
        "test = '''python3 -c 'import os, tempfile; open(os.path.join(tempfile.gettempdir(), \"{name}\"), \"w\").write(\"x\")' && python3 old_app.py'''"
    );
    let r = sandbox_repo(
        &SANDBOX_TOML.replace("test = \"python3 old_app.py\"", &probe),
        "ALTER TABLE users ADD COLUMN nickname TEXT;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        !std::env::temp_dir().join(&name).exists(),
        "a temp file the test wrote outlived the run"
    );
}

#[test]
fn a_database_the_project_keeps_inside_its_checkout_is_shared_by_base_and_head() {
    // Like a local D1 under apps/web/.wrangler: the runner takes no setting and always writes inside the checkout.
    let fixed = SANDBOX_TOML
        .replace("migrate = \"python3 apply.py\"", "migrate = \"mkdir -p state && D1_PATH=state/db.sqlite python3 apply.py\"\nshared = \"state\"\ndatabase = \"{dir}/shared/db.sqlite\"")
        .replace("env = { D1_PATH = \"{db}\" }", "env = { D1_PATH = \"{dir}/shared/db.sqlite\" }");
    let r = sandbox_repo(&fixed, "ALTER TABLE users DROP COLUMN legacy;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG201") && out.contains("main.users.legacy"),
        "{out}"
    );
    let escaping = fixed.replace("shared = \"state\"", "shared = \"../escape\"");
    let r = sandbox_repo(&escaping, "SELECT 1;");
    let (code, _, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("must be a folder inside the project"), "{err}");
}

#[test]
fn a_project_whose_runner_refuses_late_migrations_can_say_so() {
    let r = Repo::new();
    r.write(
        "migate.toml",
        "migrator = \"wrangler-d1\"\nbase = \"main\"\nout_of_order = \"block\"\n",
    );
    r.write("migrations/0001_init.sql", "CREATE TABLE a (id INTEGER);");
    r.write("migrations/0003_c.sql", "CREATE TABLE c (id INTEGER);");
    r.commit("base");
    r.git(&["checkout", "-q", "-b", "pr"]);
    r.write("migrations/0002_b.sql", "CREATE TABLE b (id INTEGER);");
    r.commit("late branch");
    let out = r.migate(&["check"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(2), "{text}");
    assert!(
        text.contains("error   MG004") && text.contains("does not allow out-of-order"),
        "{text}"
    );
    r.write(
        "migate.toml",
        "migrator = \"wrangler-d1\"\nbase = \"main\"\nout_of_order = \"maybe\"\n",
    );
    let out = r.migate(&["check"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("out_of_order"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// The sandbox run on the PostgreSQL engine: a local server per run, migrated by a small psql script.
const PG_APPLY: &str = r#"set -e
q() { psql -X -q -v ON_ERROR_STOP=1 -d "$DATABASE_URL" "$@"; }
q -c "create table if not exists d1_migrations (id serial primary key, name text unique)"
for f in migrations/*.sql; do
  n=$(basename "$f")
  [ -n "$(psql -X -A -t -d "$DATABASE_URL" -c "select 1 from d1_migrations where name = '$n'")" ] && continue
  q -f "$f"
  q -c "insert into d1_migrations (name) values ('$n')"
done
"#;

const PG_OLD_APP: &str = r#"psql -X -q -v ON_ERROR_STOP=1 -d "$DATABASE_URL" -c "insert into users (email, legacy) values ('a@b.c', 'x')" -c "select id, email, legacy from users"
"#;

const PG_TOML: &str = r#"migrator = "wrangler-d1"
base = "main"
[sandbox]
engine = "postgres"
migrate = "sh apply.sh"
env = { DATABASE_URL = "{url}" }
test = "sh old_app.sh"
"#;

fn pg_repo(second: &str) -> Repo {
    let r = Repo::new();
    r.write("migate.toml", PG_TOML);
    r.write("apply.sh", PG_APPLY);
    r.write("old_app.sh", PG_OLD_APP);
    r.write(
        "migrations/0001_init.sql",
        "create table users (id serial primary key, email text not null, legacy text);",
    );
    r.commit("init");
    r.git(&["checkout", "-q", "-b", "pr"]);
    r.write("migrations/0002_change.sql", second);
    r.commit("change");
    r
}

fn postgres_servers_under(dir: &std::path::Path) -> usize {
    postgres_server_lines(dir).len()
}

/// The `ps` lines of PostgreSQL servers whose data folder is under `dir`, with pid, parent and state.
fn postgres_server_lines(dir: &std::path::Path) -> Vec<String> {
    let ps = Command::new("ps")
        .args(["-Ao", "pid,ppid,stat,command"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&ps.stdout)
        .lines()
        .filter(|l| l.contains("postgres -D") && l.contains(&*dir.to_string_lossy()))
        .map(str::to_string)
        .collect()
}

#[test]
fn postgres_sandbox_catches_a_dropped_column_the_old_version_writes() {
    let r = pg_repo("alter table users drop column legacy;");
    // A temp folder of its own, so servers of other tests running in parallel are not counted.
    let own_tmp = std::env::temp_dir().join(format!(
        "migate-pg-tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&own_tmp).unwrap();
    let (code, out, err) = sandbox_check(&r, &[("TMPDIR", &*own_tmp.to_string_lossy())]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG201") && out.contains("public.users.legacy"),
        "{out}"
    );
    assert!(out.contains("MG810") && out.contains("legacy"), "{out}");
    assert_eq!(
        postgres_servers_under(&own_tmp),
        0,
        "a postgres server outlived the run"
    );
    let _ = fs::remove_dir_all(&own_tmp);
}

#[test]
fn postgres_sandbox_passes_a_safe_addition() {
    let r = pg_repo("alter table users add column nickname text;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("MG210")
            && out.contains("public.users.nickname")
            && !out.contains("MG810")
            && out.contains(", sandbox: postgres 1"),
        "{out}"
    );
}

#[test]
fn an_unknown_engine_is_refused_where_the_config_is_read() {
    let r = pg_repo("select 1;");
    r.write(
        "migate.toml",
        &PG_TOML.replace("\"postgres\"", "\"postgress\""),
    );
    let (code, _, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("unknown variant `postgress`"), "{err}");
}

#[test]
fn an_interrupted_postgres_run_stops_its_server() {
    let r = pg_repo("alter table users add column nickname text;");
    let started = r.dir.join("started");
    r.write(
        "migate.toml",
        &PG_TOML.replace(
            "test = \"sh old_app.sh\"",
            &format!(
                "test = \"touch {} && sleep 5 && sh old_app.sh\"",
                started.display()
            ),
        ),
    );
    r.commit("slow tests");
    let child = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .arg("check")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    for _ in 0..400 {
        if started.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(started.exists(), "the test command never started");
    let run = std::env::temp_dir().join(format!("migate-run-{}-0", child.id()));
    assert_eq!(
        postgres_servers_under(&run),
        1,
        "the run's server is up before the interrupt"
    );
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let left = postgres_server_lines(&run);
    assert!(
        left.is_empty(),
        "the server outlived the interrupt: {left:#?}"
    );
}

#[test]
fn the_next_run_stops_the_server_a_killed_run_left() {
    let r = pg_repo("alter table users add column nickname text;");
    let own_tmp = std::env::temp_dir().join(format!("migate-pg-kill-{}", std::process::id()));
    fs::create_dir_all(&own_tmp).unwrap();
    r.write(
        "slow.toml",
        &PG_TOML.replace("test = \"sh old_app.sh\"", "test = \"sleep 30\""),
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .args(["check", "--config", "slow.toml"])
        .env("TMPDIR", &own_tmp)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..400 {
        if postgres_servers_under(&own_tmp) == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    child.kill().unwrap(); // SIGKILL: no handler runs
    child.wait().unwrap();
    assert_eq!(
        postgres_servers_under(&own_tmp),
        1,
        "a killed run's server keeps running (the precondition)"
    );
    let (code, out, err) = sandbox_check(&r, &[("TMPDIR", &*own_tmp.to_string_lossy())]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(
        postgres_servers_under(&own_tmp),
        0,
        "the next run left the killed run's server running"
    );
    let _ = fs::remove_dir_all(&own_tmp);
}

#[test]
fn drizzle_on_postgres_keeps_its_history_in_its_own_schema() {
    // Like drizzle-kit on Postgres: applied migrations are recorded in drizzle.__drizzle_migrations.
    let apply = r#"set -e
q() { psql -X -q -v ON_ERROR_STOP=1 -d "$DATABASE_URL" "$@"; }
q -c "create schema if not exists drizzle" -c "create table if not exists drizzle.__drizzle_migrations (id serial primary key, hash text unique, created_at bigint)"
i=0
for f in drizzle/*.sql; do
  i=$((i + 1)); n=$(basename "$f")
  [ -n "$(psql -X -A -t -d "$DATABASE_URL" -c "select 1 from drizzle.__drizzle_migrations where hash = '$n'")" ] && continue
  q -f "$f"
  q -c "insert into drizzle.__drizzle_migrations (hash, created_at) values ('$n', 1700000000000 + $i)"
done
"#;
    let journal = |entries: &str| {
        format!(r#"{{"version":"7","dialect":"postgresql","entries":[{entries}]}}"#)
    };
    let first =
        r#"{"idx":0,"version":"7","when":1700000000000,"tag":"0000_init","breakpoints":true}"#;
    let second =
        r#"{"idx":1,"version":"7","when":1700000500000,"tag":"0001_drop","breakpoints":true}"#;
    let r = Repo::new();
    r.write("migate.toml", &PG_TOML.replace("wrangler-d1", "drizzle"));
    r.write("apply.sh", apply);
    r.write("old_app.sh", PG_OLD_APP);
    r.write("drizzle/meta/_journal.json", &journal(first));
    r.write(
        "drizzle/0000_init.sql",
        "create table users (id serial primary key, email text not null, legacy text);",
    );
    r.commit("init");
    r.git(&["checkout", "-q", "-b", "pr"]);
    r.write(
        "drizzle/meta/_journal.json",
        &journal(&format!("{first},{second}")),
    );
    r.write(
        "drizzle/0001_drop.sql",
        "alter table users drop column legacy;",
    );
    r.commit("drop");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("MG201") && out.contains("MG810"), "{out}");
}

/// Base 0001 and 0003; the change adds 0002, which sorts before the last one: MG004.
fn late_migration_repo(config: &str) -> Repo {
    let r = Repo::new();
    r.write("migate.toml", config);
    r.write("apply.py", APPLY);
    r.write("old_app.py", OLD_APP);
    r.write("seed.py", SEED);
    r.write("migrations/0001_init.sql", INIT);
    r.write("migrations/0003_c.sql", "CREATE TABLE c (id INTEGER);");
    r.commit("base");
    r.git(&["checkout", "-q", "-b", "pr"]);
    r.write("migrations/0002_b.sql", "CREATE TABLE b (id INTEGER);");
    r.commit("late");
    r
}

#[test]
fn findings_the_sandbox_did_not_need_survive_when_it_cannot_check() {
    // The sandbox proves nothing (a stub test command: MG813), but the late migration is known without it.
    let stub = SANDBOX_TOML.replace("python3 old_app.py", "true");
    let blocking = stub.replace(
        "base = \"main\"",
        "base = \"main\"\nout_of_order = \"block\"",
    );
    let r = late_migration_repo(&blocking);
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "an error was found, so it is exit 2: {out}{err}");
    assert!(
        out.contains("error   MG004") && out.contains("0002_b.sql"),
        "{out}"
    );
    assert!(
        err.contains("MG813"),
        "the sandbox part still says why it could not check: {err}"
    );

    // Only a warning without the sandbox: exit 3, since the part that could find errors did not run.
    let r = late_migration_repo(&stub);
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        out.contains("warning MG004") && out.contains("could not check, so this is not a pass"),
        "{out}"
    );
    let json = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .args(["check", "--format", "json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["outcome"], "could-not-check");
    assert!(v["error"].as_str().unwrap().contains("MG813"), "{v}");
    assert_eq!(v["findings"][0]["rule"], "MG004", "{v}");
}

#[test]
fn sandbox_catches_values_a_migration_blanks_without_losing_rows() {
    let seeded = SANDBOX_TOML.replace("test = ", "seed = \"python3 seed.py\"\ntest = ");
    // The base migration leaves a row with a legacy value; the change blanks it but deletes nothing.
    let r = sandbox_repo_full(
        &seeded,
        OLD_APP,
        &format!("{INIT}\nINSERT INTO users (id, email, legacy) VALUES (1, 'a@b', 'kept');"),
        "UPDATE users SET legacy = NULL;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG106") && out.contains("main.users.legacy"),
        "{out}"
    );
}

#[test]
fn sandbox_catches_a_rebuild_that_nulls_children_through_set_null() {
    // The dogfood's case: a faithful parent rebuild, but the child's ON DELETE SET NULL blanks its reference.
    let first = "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT);
CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users ON DELETE SET NULL);";
    let rebuild = "CREATE TABLE users_new (id INTEGER PRIMARY KEY, email TEXT, legacy TEXT);
INSERT INTO users_new SELECT id, email, legacy FROM users;
DROP TABLE users;
ALTER TABLE users_new RENAME TO users;";
    let seeded = SANDBOX_TOML.replace("test = ", "seed = \"python3 seed.py\"\ntest = ");
    let r = sandbox_repo_full(&seeded, OLD_APP, first, rebuild);
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG106") && out.contains("main.posts.user_id"),
        "{out}"
    );
    assert!(!out.contains("MG105"), "no row was lost: {out}");
}

#[test]
fn postgres_labels_lost_values_with_their_schema() {
    let r = Repo::new();
    r.write("migate.toml", PG_TOML);
    r.write("apply.sh", PG_APPLY);
    r.write("old_app.sh", PG_OLD_APP);
    r.write(
        "migrations/0001_init.sql",
        "create table users (id serial primary key, email text not null, legacy text);
insert into users (email, legacy) values ('a@b', 'kept');",
    );
    r.commit("init");
    r.git(&["checkout", "-q", "-b", "pr"]);
    r.write(
        "migrations/0002_change.sql",
        "update users set legacy = null;",
    );
    r.commit("blank");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG106")
            && out.contains("public.users.legacy")
            && !out.contains("main.public"),
        "{out}"
    );
}

#[test]
fn postgres_settings_in_the_callers_environment_do_not_reach_the_server() {
    // A developer's shell or CI may set these for their real database; the throwaway server must not inherit them.
    let r = pg_repo("alter table users add column nickname text;");
    let (code, out, err) = sandbox_check(
        &r,
        &[
            ("PGSSLMODE", "require"),
            ("PGDATABASE", "production"),
            ("PGUSER", "admin"),
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
}

/// A bin folder with the real initdb and psql and a stand-in `postgres` script, for start-up failures on demand.
fn fake_pg_bin(dir: &std::path::Path, postgres: &str) -> std::path::PathBuf {
    let real = [
        "/opt/homebrew/opt/postgresql@18/bin",
        "/usr/local/opt/postgresql@18/bin",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .chain(
        fs::read_dir("/usr/lib/postgresql")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path().join("bin")),
    )
    .find(|d| d.join("initdb").is_file())
    .expect("PostgreSQL server programs");
    let bin = dir.join("fake-pg-bin");
    fs::create_dir_all(&bin).unwrap();
    for tool in ["initdb", "psql"] {
        std::os::unix::fs::symlink(real.join(tool), bin.join(tool)).unwrap();
    }
    fs::write(bin.join("postgres"), format!("#!/bin/sh\n{postgres}\n")).unwrap();
    Command::new("chmod")
        .args(["+x", &bin.join("postgres").to_string_lossy()])
        .status()
        .unwrap();
    bin
}

#[test]
fn a_postgres_that_dies_at_start_fails_the_run_at_once_with_its_own_error() {
    let r = pg_repo("select 1;");
    let bin = fake_pg_bin(
        &r.dir,
        "echo 'FATAL: the stand-in server refused to start' >&2; exit 1",
    );
    let started = std::time::Instant::now();
    let (code, _, err) = sandbox_check(&r, &[("MIGATE_PG_BIN", &*bin.to_string_lossy())]);
    assert_eq!(code, 3, "{err}");
    assert!(
        err.contains("the stand-in server refused to start"),
        "{err}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn an_interrupt_while_the_server_starts_stops_it() {
    let r = pg_repo("select 1;");
    let pid_file = r.dir.join("server.pid");
    let bin = fake_pg_bin(
        &r.dir,
        &format!("echo $$ > {}\nexec sleep 60", pid_file.display()),
    );
    let child = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .arg("check")
        .env("MIGATE_PG_BIN", &bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    for _ in 0..400 {
        if fs::read_to_string(&pid_file).is_ok_and(|p| !p.trim().is_empty()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let server = fs::read_to_string(&pid_file).unwrap().trim().to_string();
    std::thread::sleep(std::time::Duration::from_millis(300)); // migate is now waiting for it to accept connections
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::thread::sleep(std::time::Duration::from_millis(500));
    let alive = Command::new("kill")
        .args(["-0", &server])
        .status()
        .unwrap()
        .success();
    if alive {
        let _ = Command::new("kill").args(["-KILL", &server]).status();
    }
    assert!(!alive, "the starting server outlived the interrupt");
}

const AUDIT_TRIGGER: &str = "CREATE TABLE audit (at TEXT);
CREATE TRIGGER users_audit AFTER INSERT ON users BEGIN INSERT INTO audit VALUES (datetime('now')); END;";

#[test]
fn without_tests_the_schema_rule_still_decides() {
    let no_test = SANDBOX_TOML.replace("test = \"python3 old_app.py\"\n", "");
    let r = sandbox_repo(&no_test, AUDIT_TRIGGER);
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("error   MG503"), "{out}");
}

#[test]
fn a_base_suite_that_flakes_once_is_retried_rather_than_refused() {
    // Fails the first time only, as a busy database at start-up does, then behaves like the old app.
    let flaky = SANDBOX_TOML.replace(
        "test = \"python3 old_app.py\"",
        "test = 'if [ ! -e .flaked ]; then touch .flaked; echo \"SQLITE_BUSY: database is locked\" >&2; exit 1; fi; python3 old_app.py'",
    );
    let r = sandbox_repo(&flaky, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        err.contains("SQLITE_BUSY") && err.contains("again"),
        "the flake is reported, not hidden: {err}"
    );
}

#[test]
fn a_base_suite_that_always_fails_is_still_refused_after_the_retry() {
    let failing = SANDBOX_TOML.replace(
        "test = \"python3 old_app.py\"",
        "test = 'echo broken >&2; exit 1'",
    );
    let r = sandbox_repo(&failing, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("MG813") && err.contains("fail on the database at"),
        "{err}"
    );
}

#[test]
fn postgres_says_what_it_did_not_check() {
    // A row-level security policy is outside migate's schema model: the change is named as not checked, never silent.
    let r = pg_repo(
        "alter table users enable row level security;\ncreate policy none_for_you on users using (false);",
    );
    let (_, out, err) = sandbox_check(&r, &[]);
    assert!(
        out.contains("MG801") && out.contains("policy public.users.none_for_you"),
        "{out}{err}"
    );
}

#[test]
fn postgres_counts_values_in_a_table_wider_than_fifty_columns() {
    // Postgres functions take at most 100 arguments; a count per column must not become one call's arguments.
    let cols: Vec<String> = (0..62).map(|i| format!("c{i} text")).collect();
    let r = pg_repo(&format!("create table wide ({});", cols.join(", ")));
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
}

#[test]
fn a_constraint_on_a_new_column_the_migration_fills_stays_an_error() {
    // The column is new, but the migration backfills it: production rows can collide under the UNIQUE index.
    let seeded = SANDBOX_TOML
        .replace("test = \"python3 old_app.py\"\n", "")
        .replace("[sandbox]\n", "[sandbox]\nseed = \"python3 seed.py\"\n");
    let r = sandbox_repo(
        &seeded,
        "ALTER TABLE users ADD COLUMN slug TEXT;\nUPDATE users SET slug = lower(email);\nCREATE UNIQUE INDEX users_slug ON users (slug);",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("error   MG302"), "{out}");
}

#[test]
fn a_suite_that_only_reads_does_not_vouch_for_a_trigger_that_rejects_writes() {
    // The old app writes users in production; its tests only read them. Passing tests cannot clear this trigger.
    const READS_ONLY: &str = r#"import os, sqlite3
con = sqlite3.connect(os.environ["D1_PATH"])
print(con.execute("SELECT id, email, legacy FROM users").fetchall())
"#;
    let r = sandbox_repo_with(
        SANDBOX_TOML,
        READS_ONLY,
        "CREATE TRIGGER no_new_users BEFORE INSERT ON users BEGIN SELECT RAISE(ABORT, 'closed'); END;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(out.contains("error   MG503"), "{out}");
}

#[test]
fn a_migration_that_fails_on_the_rows_present_says_which_rows() {
    // Rows production holds already collide: the new UNIQUE index fails on them, not on its own SQL, and the message
    // must say so.
    let r = sandbox_repo_full(
        SANDBOX_TOML,
        OLD_APP,
        &format!("{INIT}\nINSERT INTO users (email) VALUES ('dup@b.c'), ('dup@b.c');"),
        "CREATE UNIQUE INDEX users_email ON users (email);",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("MG803") && out.contains("main.users ("),
        "{out}"
    );
}

#[test]
fn an_interrupt_as_initdb_finishes_leaves_no_server() {
    // The handler's clean-up takes a while (the new cluster's files); a server must not start meanwhile and then
    // outlive the run. The window is narrow, so three attempts: the old code left a server in 5 of 6 single attempts.
    for attempt in 0..3 {
        assert!(
            !interrupt_as_initdb_finishes(attempt),
            "attempt {attempt}: a server outlived the run"
        );
    }
}

/// Interrupt a run the moment initdb finishes; whether a server it started is still running afterwards.
fn interrupt_as_initdb_finishes(attempt: u32) -> bool {
    let r = pg_repo("select 1;");
    let done = r.dir.join(format!("initdb.done.{attempt}"));
    let pid_file = r.dir.join(format!("server.pid.{attempt}"));
    let bin = fake_pg_bin(
        &r.dir,
        &format!("echo $$ > {}\nexec sleep 60", pid_file.display()),
    );
    let real_initdb = fs::read_link(bin.join("initdb")).unwrap();
    fs::remove_file(bin.join("initdb")).unwrap();
    fs::write(
        bin.join("initdb"),
        format!(
            "#!/bin/sh\n{} \"$@\"; s=$?; touch {}; sleep 0.02; exit $s\n",
            real_initdb.display(),
            done.display()
        ),
    )
    .unwrap();
    Command::new("chmod")
        .args(["+x", &bin.join("initdb").to_string_lossy()])
        .status()
        .unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_migate"))
        .current_dir(&r.dir)
        .arg("check")
        .env("MIGATE_PG_BIN", &bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    while !done.exists() {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert_eq!(child.wait_with_output().unwrap().status.code(), Some(3));
    std::thread::sleep(std::time::Duration::from_millis(300));
    let Ok(server) = fs::read_to_string(&pid_file) else {
        return false; // never started
    };
    let alive = Command::new("kill")
        .args(["-0", server.trim()])
        .status()
        .unwrap()
        .success();
    if alive {
        let _ = Command::new("kill").args(["-KILL", server.trim()]).status();
    }
    alive
}

#[test]
fn sqlite_names_a_changed_virtual_table_as_not_checked() {
    let r = sandbox_repo_full(
        SANDBOX_TOML,
        OLD_APP,
        &format!("{INIT}\nCREATE VIRTUAL TABLE docs USING fts5(title, body, tokenize = 'porter');"),
        "DROP TABLE docs;\nCREATE VIRTUAL TABLE docs USING fts5(title, body, tokenize = 'unicode61');",
    );
    let (_, out, err) = sandbox_check(&r, &[]);
    assert!(
        out.contains("MG801") && out.contains("virtual table main.docs"),
        "{out}{err}"
    );
}

/// The applier, then the header bytes a newer SQLite writes: bytes 96-99 hold the version of the last writer, and
/// SQLite itself never reads them, so this is what a file from SQLite 3.99.0 looks like to a reader.
const APPLY_AS_NEWER_SQLITE: &str = r#"python3 apply.py && python3 -c "import os; f = open(os.environ['D1_PATH'], 'r+b'); f.seek(96); f.write((3099000).to_bytes(4, 'big'))" "#;

#[test]
fn a_database_written_by_a_newer_sqlite_is_flagged() {
    let newer = SANDBOX_TOML.replace(
        "migrate = \"python3 apply.py\"",
        &format!("migrate = \"\"\"{APPLY_AS_NEWER_SQLITE}\"\"\""),
    );
    let r = sandbox_repo(&newer, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "a warning, not an error: {out}{err}");
    assert!(
        out.contains("warning MG805") && out.contains("SQLite 3.99.0"),
        "{out}"
    );
}

#[test]
fn a_database_written_by_an_older_or_the_same_sqlite_is_not_flagged() {
    let r = sandbox_repo(SANDBOX_TOML, "ALTER TABLE users ADD COLUMN nickname TEXT;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(!out.contains("MG805"), "{out}");
}

fn with_target(config: &str, target: &str) -> String {
    config.replacen(
        "[sandbox]\n",
        &format!("[sandbox]\ntarget_version = \"{target}\"\n"),
        1,
    )
}

#[test]
fn sqlite_syntax_newer_than_production_is_an_error() {
    let r = sandbox_repo(
        &with_target(SANDBOX_TOML, "3.31"),
        "CREATE TABLE plain (id INTEGER);\nCREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT) STRICT;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("error   MG701  STRICT table (migrations/0002_change.sql:2)")
            && out.contains("3.37.0"),
        "{out}"
    );
    assert!(
        out.contains("note    MG806"),
        "the sandbox's SQLite is newer than 3.31: {out}"
    );
}

#[test]
fn sqlite_syntax_production_can_run_passes() {
    let r = sandbox_repo(
        &with_target(SANDBOX_TOML, "3.99"),
        "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT) STRICT;",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(!out.contains("MG701") && !out.contains("MG806"), "{out}");
}

#[test]
fn a_target_version_that_names_no_release_cannot_check() {
    let r = sandbox_repo(&with_target(SANDBOX_TOML, "latest"), "SELECT 1;");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 3, "{out}{err}");
    assert!(
        err.contains("target_version") && err.contains("latest"),
        "{err}"
    );
}

#[test]
fn postgres_without_productions_version_installed_says_so() {
    let r = pg_repo("alter table users add column nickname text;");
    std::fs::write(r.dir.join("migate.toml"), with_target(PG_TOML, "9")).unwrap();
    r.commit("target 9");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "a warning, not an error: {out}{err}");
    assert!(
        out.contains("warning MG806") && out.contains("PostgreSQL 9"),
        "{out}"
    );
}

#[test]
fn postgres_runs_on_productions_major_when_it_is_installed() {
    if std::env::var_os("MIGATE_PG_BIN").is_some() {
        eprintln!("skipped: MIGATE_PG_BIN chooses the server explicitly");
        return;
    }
    let mut majors: Vec<u32> = ["/opt/homebrew/opt", "/usr/local/opt", "/usr/lib/postgresql"]
        .iter()
        .flat_map(|root| fs::read_dir(root).into_iter().flatten().flatten())
        .filter(|e| e.path().join("bin/postgres").is_file())
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .trim_start_matches("postgresql@")
                .parse()
                .ok()
        })
        .collect();
    majors.sort_unstable();
    majors.dedup();
    let [oldest, .., _] = majors[..] else {
        eprintln!("skipped: needs two PostgreSQL majors installed, found {majors:?}");
        return;
    };
    let r = pg_repo("alter table users add column nickname text;");
    std::fs::write(
        r.dir.join("migate.toml"),
        with_target(PG_TOML, &oldest.to_string()),
    )
    .unwrap();
    r.commit("target oldest");
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains(&format!("sandbox: postgres {oldest}.")) && !out.contains("MG806"),
        "{out}"
    );
}

/// SANDBOX_TOML without the base tests: the rows a trigger is judged by come from the migrations alone.
fn without_tests() -> String {
    SANDBOX_TOML.replace("test = \"python3 old_app.py\"\n", "")
}

/// Two users from the base migration: one with a legacy value, one without.
const TWO_USERS: &str =
    "INSERT INTO users (id, email, legacy) VALUES (1, 'a@x', 'old'), (2, 'b@x', NULL);";

fn trigger_run(first: &str, second: &str) -> (i32, String) {
    let r = sandbox_repo_full(&without_tests(), OLD_APP, first, second);
    let (code, out, err) = sandbox_check(&r, &[]);
    (code, format!("{out}{err}"))
}

#[test]
fn a_trigger_that_refuses_a_row_the_database_holds_is_an_error_with_the_count() {
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}"),
        "CREATE TRIGGER needs_legacy BEFORE INSERT ON users WHEN new.legacy IS NULL BEGIN SELECT RAISE(ABORT, 'legacy required'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.users.needs_legacy")
            && out.contains("1 of the 2 rows the database holds would already be refused")
            && out.contains("legacy required"),
        "{out}"
    );
}

#[test]
fn a_trigger_no_stored_row_breaks_stays_an_error() {
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}"),
        "CREATE TRIGGER no_zzz BEFORE INSERT ON users WHEN new.legacy = 'zzz' BEGIN SELECT RAISE(ABORT, 'no zzz'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.users.no_zzz")
            && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn a_trigger_on_an_empty_table_stays_an_error() {
    let (code, out) = trigger_run(
        INIT,
        "CREATE TRIGGER no_zzz BEFORE INSERT ON users WHEN new.legacy = 'zzz' BEGIN SELECT RAISE(ABORT, 'no zzz'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.users.no_zzz")
            && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn a_row_written_back_is_not_refused_as_its_own_duplicate() {
    // Uniqueness enforced by a trigger: a copy of the row would be refused; the row itself must not be.
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}"),
        "CREATE TRIGGER one_email BEFORE INSERT ON users WHEN EXISTS (SELECT 1 FROM users u WHERE u.email = new.email)
         BEGIN SELECT RAISE(ABORT, 'email taken'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.users.one_email")
            && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn only_the_trigger_being_judged_can_refuse() {
    // The base already has a trigger refusing every insert and one refusing every delete; neither is the new one's.
    let first = format!(
        "{INIT}\n{TWO_USERS}
CREATE TRIGGER no_inserts BEFORE INSERT ON users BEGIN SELECT RAISE(ABORT, 'closed'); END;
CREATE TRIGGER no_deletes BEFORE DELETE ON users BEGIN SELECT RAISE(ABORT, 'never delete'); END;"
    );
    let (code, out) = trigger_run(
        &first,
        "CREATE TRIGGER no_zzz BEFORE INSERT ON users WHEN new.legacy = 'zzz' BEGIN SELECT RAISE(ABORT, 'no zzz'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.users.no_zzz")
            && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn each_kind_of_trigger_event_is_judged() {
    let first = format!("{INIT}\n{TWO_USERS}");
    for (name, body, refusal) in [
        (
            "on_update",
            "BEFORE UPDATE OF legacy ON users WHEN new.legacy = 'old' BEGIN SELECT RAISE(ABORT, 'frozen'); END",
            "frozen",
        ),
        (
            "on_delete",
            "BEFORE DELETE ON users WHEN old.id = 2 BEGIN SELECT RAISE(ABORT, 'keep two'); END",
            "keep two",
        ),
        (
            "after_insert",
            "AFTER INSERT ON users WHEN new.legacy IS NULL BEGIN SELECT RAISE(ABORT, 'set legacy'); END",
            "set legacy",
        ),
        (
            "ignored",
            "BEFORE INSERT ON users WHEN new.legacy IS NULL BEGIN SELECT RAISE(IGNORE); END",
            "did not stay written",
        ),
        (
            "rolled_back",
            "BEFORE INSERT ON users WHEN new.legacy = 'old' BEGIN SELECT RAISE(ROLLBACK, 'undo'); END",
            "undo",
        ),
    ] {
        let (code, out) = trigger_run(&first, &format!("CREATE TRIGGER {name} {body};"));
        assert_eq!(code, 2, "{name}: {out}");
        assert!(
            out.contains(&format!("error   MG503  main.users.{name}"))
                && out.contains("1 of the 2 rows the database holds would already be refused")
                && out.contains(refusal),
            "{name}: {out}"
        );
    }
}

#[test]
fn each_kind_of_table_keeps_its_rows_and_ids() {
    // A hidden rowid, AUTOINCREMENT (with a UNIQUE column), WITHOUT ROWID and a generated column: one row of two is refused
    // in each, so a row written back under a new id, or a generated value not recomputed, would show in the count.
    let first = "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT);
CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('keep'), ('bad');
CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE); INSERT INTO tags (name) VALUES ('ok'), ('bad');
CREATE TABLE codes (code TEXT PRIMARY KEY, v TEXT) WITHOUT ROWID; INSERT INTO codes VALUES ('k1', 'ok'), ('k2', 'bad');
CREATE TABLE prices (id INTEGER PRIMARY KEY, net INTEGER, gross INTEGER GENERATED ALWAYS AS (net * 2) STORED);
INSERT INTO prices (id, net) VALUES (1, 10), (2, 600);";
    let second = "CREATE TRIGGER notes_ok BEFORE INSERT ON notes WHEN new.body = 'bad' BEGIN SELECT RAISE(ABORT, 'bad note'); END;
CREATE TRIGGER tags_ok BEFORE INSERT ON tags WHEN new.name = 'bad' BEGIN SELECT RAISE(ABORT, 'bad tag'); END;
CREATE TRIGGER codes_ok BEFORE INSERT ON codes WHEN new.v = 'bad' BEGIN SELECT RAISE(ABORT, 'bad code'); END;
CREATE TRIGGER prices_ok BEFORE INSERT ON prices WHEN new.gross > 1000 BEGIN SELECT RAISE(ABORT, 'too dear'); END;";
    let (code, out) = trigger_run(first, second);
    assert_eq!(code, 2, "{out}");
    for (table, refusal) in [
        ("notes", "bad note"),
        ("tags", "bad tag"),
        ("codes", "bad code"),
        ("prices", "too dear"),
    ] {
        let line = out
            .lines()
            .find(|l| l.contains(&format!("main.{table}.{table}_ok")))
            .unwrap_or_else(|| panic!("{table}: {out}"));
        assert!(
            line.starts_with("error   MG503")
                && line.contains("1 of the 2 rows the database holds would already be refused")
                && line.contains(refusal),
            "{line}"
        );
    }
}

#[test]
fn a_changed_trigger_is_judged_and_a_dropped_one_is_not() {
    let first = format!(
        "{INIT}\n{TWO_USERS}
CREATE TRIGGER guard BEFORE INSERT ON users WHEN new.legacy = 'zzz' BEGIN SELECT RAISE(ABORT, 'no zzz'); END;
CREATE TRIGGER gone BEFORE INSERT ON users WHEN new.legacy = 'zzz' BEGIN SELECT RAISE(ABORT, 'no zzz'); END;"
    );
    let (code, out) = trigger_run(
        &first,
        "DROP TRIGGER guard;
CREATE TRIGGER guard BEFORE INSERT ON users WHEN new.legacy = 'old' BEGIN SELECT RAISE(ABORT, 'no old'); END;
DROP TRIGGER gone;",
    );
    assert_eq!(code, 2, "{out}");
    let line = |name: &str| {
        out.lines()
            .find(|l| l.contains(&format!("main.users.{name}")))
            .unwrap_or_else(|| panic!("{name}: {out}"))
            .to_string()
    };
    assert!(
        line("guard").starts_with("error   MG504")
            && line("guard")
                .contains("1 of the 2 rows the database holds would already be refused"),
        "{out}"
    );
    assert!(
        !line("gone").contains("rows"),
        "a dropped trigger refuses nothing new: {out}"
    );
}

#[test]
fn postgres_says_it_cannot_judge_a_trigger_by_its_rows_yet() {
    let r = pg_repo(
        "create function no_zzz() returns trigger language plpgsql as $$ begin raise exception 'no zzz'; end $$;
create trigger no_zzz before insert on users for each row when (new.email = 'zzz') execute function no_zzz();",
    );
    let (code, out, err) = sandbox_check(&r, &[]);
    assert_eq!(code, 2, "{out}{err}");
    assert!(
        out.contains("error   MG503") && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn a_row_that_fails_for_another_reason_cannot_judge_the_trigger() {
    // A row stored while CHECK constraints were ignored: writing it again fails on the CHECK, not on the trigger.
    let first = "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT);
CREATE TABLE stock (id INTEGER PRIMARY KEY, n INTEGER CHECK (n >= 0));
PRAGMA ignore_check_constraints = ON; INSERT INTO stock VALUES (1, -5); PRAGMA ignore_check_constraints = OFF;";
    let (code, out) = trigger_run(
        first,
        "CREATE TRIGGER cap BEFORE INSERT ON stock WHEN new.n > 100 BEGIN SELECT RAISE(ABORT, 'too many'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.stock.cap") && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn a_trigger_on_a_view_stays_an_error() {
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}\nCREATE VIEW people AS SELECT id, email FROM users;"),
        "CREATE TRIGGER people_in INSTEAD OF INSERT ON people BEGIN INSERT INTO users (email) VALUES (new.email); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.people.people_in")
            && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn a_trigger_naming_its_table_in_another_case_is_judged_not_fatal() {
    // SQLite names are case-insensitive; the trigger keeps the table's name as written in its own SQL.
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}"),
        "CREATE TRIGGER g BEFORE INSERT ON USERS WHEN new.legacy IS NULL BEGIN SELECT RAISE(ABORT, 'legacy required'); END;
",
    );
    assert_ne!(code, 3, "the run must not be lost: {out}");
    assert!(
        out.contains("error   MG503") && out.contains("legacy required"),
        "{out}"
    );
}

#[test]
fn rows_that_satisfy_a_trigger_do_not_clear_it_when_the_old_app_writes_differently() {
    // The migration fills a new column and requires it: every stored row passes, every insert from the old app (which
    // does not know the column) is refused. Rows the database holds cannot vouch for the old app's writes.
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}"),
        "ALTER TABLE users ADD COLUMN tenant TEXT; UPDATE users SET tenant = 't1';
CREATE TRIGGER need_tenant BEFORE INSERT ON users WHEN new.tenant IS NULL BEGIN SELECT RAISE(ABORT, 'tenant required'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.users.need_tenant"),
        "{out}"
    );
}

#[test]
fn a_column_named_rowid_does_not_make_up_refusals() {
    // The table's own `rowid` column hides SQLite's row id under that name; the probe must find rows by another alias.
    let (code, out) = trigger_run(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT);
CREATE TABLE t (rowid TEXT, v TEXT); INSERT INTO t VALUES (NULL, 'ok'), (NULL, 'ok2');",
        "CREATE TRIGGER never BEFORE INSERT ON t WHEN new.v = 'never' BEGIN SELECT RAISE(ABORT, 'never'); END;",
    );
    assert_eq!(code, 2, "{out}");
    assert!(
        out.contains("error   MG503  main.t.never") && !out.contains("would already be refused"),
        "{out}"
    );
}

#[test]
fn a_trigger_that_undoes_a_write_is_described_as_such() {
    // No RAISE in the body: the row is written, then deleted by the trigger itself.
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}"),
        "CREATE TRIGGER undo AFTER INSERT ON users WHEN new.legacy IS NULL BEGIN DELETE FROM users WHERE id = new.id; END;",
    );
    assert_eq!(code, 2, "{out}");
    let line = out
        .lines()
        .find(|l| l.contains("main.users.undo"))
        .unwrap_or_default();
    assert!(
        line.contains("1 of the 2 rows")
            && line.contains("did not stay written")
            && !line.contains("RAISE(IGNORE)"),
        "{line}"
    );
}

#[test]
fn a_trigger_error_prints_the_marker_that_accepts_it() {
    let trigger = "CREATE TRIGGER no_zzz BEFORE INSERT ON users WHEN new.legacy = 'zzz' BEGIN SELECT RAISE(ABORT, 'no zzz'); END;";
    let (code, out) = trigger_run(&format!("{INIT}\n{TWO_USERS}"), trigger);
    assert_eq!(code, 2, "{out}");
    let line = out
        .lines()
        .find(|l| l.contains("main.users.no_zzz"))
        .unwrap_or_default();
    let start = line
        .find("`migate: contract")
        .unwrap_or_else(|| panic!("no marker offered: {line}"))
        + 1;
    let marker = &line[start..start + line[start..].find('`').unwrap()];
    assert_eq!(
        marker,
        "migate: contract add-trigger users.no_zzz -- <why the old app never writes a row it refuses>"
    );
    // Pasted into the migration with a reason, it accepts the finding.
    let pasted = marker.replace(
        "<why the old app never writes a row it refuses>",
        "the old app never sets legacy",
    );
    let (code, out) = trigger_run(
        &format!("{INIT}\n{TWO_USERS}"),
        &format!("-- {pasted}\n{trigger}"),
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("note    MG503  main.users.no_zzz")
            && out.contains("accepted by the contract"),
        "{out}"
    );
}
