// The whole file is test code; clippy's allow-unwrap-in-tests covers #[test] functions but not their helpers.
#![allow(clippy::unwrap_used)]

//! The SQLite reader against real database files.

use std::collections::BTreeMap;
use std::fs;

use migate_core::schema::{Detail, IndexPart, Kind, Object, Snapshot, TypeFamily};
use migate_engine_sqlite::{Sandbox, read, snapshot, snapshot_files};
use rusqlite::Connection;

fn counts(s: &Snapshot) -> BTreeMap<Kind, usize> {
    let mut m = BTreeMap::new();
    for o in s.objects() {
        *m.entry(o.kind()).or_insert(0) += 1;
    }
    m
}

fn build(sql: &str) -> Sandbox {
    let sb = Sandbox::create().unwrap();
    Connection::open(sb.db_path())
        .unwrap()
        .execute_batch(sql)
        .unwrap();
    sb
}

fn get<'a>(s: &'a Snapshot, kind: Kind, parent: Option<&str>, name: &str) -> &'a Object {
    s.get(kind, "main", parent, name)
        .unwrap_or_else(|| panic!("missing {kind:?} {parent:?}.{name}"))
}

#[test]
fn the_100_table_benchmark_has_the_expected_counts() {
    // Expected values were counted independently with the sqlite3 CLI on the same file (pragma_table_xinfo,
    // pragma_foreign_key_list, pragma_index_list and sqlite_schema): 100 tables, 1291 table columns, 197 foreign keys,
    // 200 explicit indexes, 5 triggers, 5 views; every table has a primary key and one CHECK.
    let schema = include_str!("../../../docs/research/bench/stack/schema.sql");
    let s = build(schema).snapshot().unwrap();
    let c = counts(&s);
    let expected = [
        (Kind::Namespace, 1),
        (Kind::Table, 100),
        (Kind::Column, 1291),
        (Kind::PrimaryKey, 100),
        (Kind::Check, 100),
        (Kind::ForeignKey, 197),
        (Kind::Index, 200),
        (Kind::View, 5),
        (Kind::Trigger, 5),
    ];
    for (kind, n) in expected {
        assert_eq!(c.get(&kind).copied().unwrap_or(0), n, "{kind:?}");
    }
    assert_eq!(
        c.values().sum::<usize>(),
        expected.iter().map(|e| e.1).sum::<usize>(),
        "no unexpected kinds"
    );
}

const RICH: &str = r#"
CREATE TABLE users (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  email TEXT NOT NULL COLLATE NOCASE UNIQUE,
  email_lower TEXT GENERATED ALWAYS AS (lower(email)) VIRTUAL,
  score REAL AS (length(email) * 1.5) STORED,
  name VARCHAR(80),
  price NUMERIC(10,2),
  "odd, name" TEXT COLLATE BINARY,
  CONSTRAINT email_present CHECK (email <> ''),
  CHECK (length(email) < 320)
);
CREATE TABLE tags (name TEXT PRIMARY KEY, note TEXT) WITHOUT ROWID;
CREATE TABLE posts (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users ON DELETE CASCADE,
  body TEXT,
  deleted INTEGER NOT NULL DEFAULT 0,
  kind TEXT CHECK (kind IN ('a', 'b, c'))
) STRICT;
CREATE INDEX posts_live ON posts (user_id, id DESC) WHERE deleted = 0;
CREATE INDEX users_email_expr ON users (lower(email), length(email) DESC);
CREATE VIEW live_posts AS SELECT id, user_id FROM posts WHERE deleted = 0;
CREATE TRIGGER posts_touch AFTER UPDATE OF body ON posts BEGIN SELECT 1; END;
CREATE TRIGGER live_posts_insert INSTEAD OF INSERT ON live_posts BEGIN SELECT 1; END;
"#;

#[test]
fn columns_keep_types_generation_collation_and_identity() {
    let s = build(RICH).snapshot().unwrap();
    assert_eq!(s.engine_version(), Some(rusqlite::version()));
    let col = |table: &str, name: &str| match &get(&s, Kind::Column, Some(table), name).detail {
        Detail::Column(c) => c.clone(),
        other => panic!("{other:?}"),
    };
    let id = col("users", "id");
    assert!(id.identity && !id.nullable && id.data_type.family == TypeFamily::Integer);
    let email = col("users", "email");
    assert_eq!(email.collation.as_deref(), Some("NOCASE"));
    assert!(!email.nullable);
    let lower = col("users", "email_lower").generated.unwrap();
    assert_eq!(
        (lower.expression.as_str(), lower.stored),
        ("lower(email)", false)
    );
    let score = col("users", "score").generated.unwrap();
    assert_eq!(
        (score.expression.as_str(), score.stored),
        ("length(email) * 1.5", true)
    );
    let name = col("users", "name").data_type;
    // SQLite does not enforce declared sizes, so none is reported as a limit; the declaration stays in raw.
    assert_eq!(
        (name.family, name.length, name.raw.as_str()),
        (TypeFamily::Text, None, "VARCHAR(80)")
    );
    // A quoted column name with a comma, at the top level of the table body.
    assert_eq!(
        col("users", "odd, name").collation.as_deref(),
        Some("BINARY")
    );
    let price = col("users", "price").data_type;
    assert_eq!(
        (price.family, price.precision, price.scale),
        (TypeFamily::Decimal, None, None)
    );
    assert_eq!(col("posts", "deleted").default.as_deref(), Some("0"));
    // A WITHOUT ROWID primary key cannot be NULL; an ordinary column can.
    assert!(!col("tags", "name").nullable && col("tags", "note").nullable);
}

#[test]
fn tables_constraints_and_keys() {
    let s = build(RICH).snapshot().unwrap();
    assert!(matches!(
        get(&s, Kind::Table, None, "tags").detail,
        Detail::Table {
            without_rowid: true,
            ..
        }
    ));
    assert!(matches!(
        get(&s, Kind::Table, None, "posts").detail,
        Detail::Table { strict: true, .. }
    ));
    assert!(
        matches!(&get(&s, Kind::Unique, Some("users"), "users_email_key").detail,
        Detail::Unique { columns } if columns == &["email"])
    );
    assert!(
        matches!(&get(&s, Kind::Check, Some("users"), "email_present").detail,
        Detail::Check { expression, .. } if expression == "email <> ''")
    );
    let checks: Vec<String> = s
        .objects()
        .iter()
        .filter_map(|o| match &o.detail {
            Detail::Check { expression, .. } => Some(expression.clone()),
            _ => None,
        })
        .collect();
    assert!(
        checks.contains(&"length(email) < 320".to_string()),
        "{checks:?}"
    );
    assert!(
        checks.contains(&"kind IN ('a', 'b, c')".to_string()),
        "a comma inside a string: {checks:?}"
    );
    match &get(&s, Kind::ForeignKey, Some("posts"), "posts_user_id_fkey").detail {
        Detail::ForeignKey(fk) => {
            assert_eq!(fk.ref_table, "users");
            assert_eq!(
                fk.ref_columns,
                ["id"],
                "REFERENCES users without columns means its primary key"
            );
            assert_eq!(fk.on_delete, "CASCADE");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn indexes_triggers_and_views() {
    let s = build(RICH).snapshot().unwrap();
    match &get(&s, Kind::Index, Some("posts"), "posts_live").detail {
        Detail::Index(i) => {
            assert!(!i.unique);
            assert_eq!(i.predicate.as_deref(), Some("deleted = 0"));
            assert_eq!(
                i.parts,
                [
                    IndexPart::Column {
                        name: "user_id".into(),
                        descending: false
                    },
                    IndexPart::Column {
                        name: "id".into(),
                        descending: true
                    }
                ]
            );
        }
        other => panic!("{other:?}"),
    }
    match &get(&s, Kind::Index, Some("users"), "users_email_expr").detail {
        Detail::Index(i) => assert_eq!(
            i.parts,
            [
                IndexPart::Expression {
                    sql: "lower(email)".into(),
                    descending: false
                },
                IndexPart::Expression {
                    sql: "length(email)".into(),
                    descending: true
                }
            ]
        ),
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(&get(&s, Kind::Trigger, Some("posts"), "posts_touch").detail,
        Detail::Trigger { timing, events, .. } if timing == "AFTER" && events == &["UPDATE"])
    );
    assert!(
        matches!(&get(&s, Kind::Trigger, Some("live_posts"), "live_posts_insert").detail,
        Detail::Trigger { timing, events, .. } if timing == "INSTEAD OF" && events == &["INSERT"])
    );
    assert!(matches!(&get(&s, Kind::View, None, "live_posts").detail,
        Detail::View { definition, .. } if definition.contains("WHERE deleted = 0")));
}

#[test]
fn reading_is_deterministic() {
    let sb = build(RICH);
    assert_eq!(sb.snapshot().unwrap(), sb.snapshot().unwrap());
}

#[test]
fn uncheckpointed_wal_data_is_read_and_the_source_is_untouched() {
    let sb = Sandbox::create().unwrap();
    let writer = Connection::open(sb.db_path()).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0; CREATE TABLE only_in_wal (id INTEGER);")
        .unwrap();
    // The table exists only in the -wal file so far: a plain copy of the database file would not show it.
    let dir = sb.db_path().parent().unwrap().to_path_buf();
    let state = || {
        let mut v: Vec<(String, Vec<u8>)> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap())
            .filter(|e| !e.file_name().to_string_lossy().ends_with("-shm"))
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        v.sort();
        v
    };
    let before = state();
    assert!(
        before.iter().any(|(n, _)| n.ends_with("-wal")),
        "the test needs a -wal file"
    );
    let s = sb.snapshot().unwrap();
    assert!(
        s.get(Kind::Table, "main", None, "only_in_wal").is_some(),
        "WAL content was missed"
    );
    assert_eq!(before, state(), "reading changed the database or its WAL");
    drop(writer);
}

#[test]
fn a_missing_empty_or_foreign_file_is_an_error_not_an_empty_snapshot() {
    let sb = Sandbox::create().unwrap();
    assert!(snapshot(sb.db_path()).is_err(), "missing file");
    fs::write(sb.db_path(), b"").unwrap();
    let err = snapshot(sb.db_path()).unwrap_err();
    assert!(err.0.contains("empty file"), "{err}");
    fs::write(sb.db_path(), b"not a database at all, just text").unwrap();
    assert!(snapshot(sb.db_path()).is_err(), "a text file");
}

#[test]
fn a_sandbox_starts_empty_and_cleans_up() {
    let sb = Sandbox::create().unwrap();
    let dir = sb.db_path().parent().unwrap().to_path_buf();
    assert!(dir.is_dir() && !sb.db_path().exists());
    assert!(sb.url().starts_with("file:") && sb.url().ends_with("sandbox.db"));
    Connection::open(sb.db_path())
        .unwrap()
        .execute_batch("CREATE TABLE t (id INTEGER);")
        .unwrap();
    assert_eq!(counts(&sb.snapshot().unwrap()).get(&Kind::Table), Some(&1));
    drop(sb);
    assert!(!dir.exists(), "the sandbox directory was left behind");
}

#[test]
fn foreign_key_deferral_is_read_from_the_create_text() {
    let s = build(
        "CREATE TABLE p (id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, UNIQUE (a, b));
         CREATE TABLE c (
           id INTEGER PRIMARY KEY,
           col_deferred INTEGER REFERENCES p (id) DEFERRABLE INITIALLY DEFERRED,
           col_plain INTEGER REFERENCES p (id) DEFERRABLE,
           col_immediate INTEGER REFERENCES p (id) DEFERRABLE INITIALLY IMMEDIATE,
           col_not INTEGER REFERENCES p (id) NOT DEFERRABLE INITIALLY DEFERRED,
           col_none INTEGER REFERENCES p (id),
           x INTEGER, y INTEGER,
           CONSTRAINT pair FOREIGN KEY (x, \"Y\") REFERENCES p (a, b) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED
         );",
    )
    .snapshot()
    .unwrap();
    let fk = |name: &str| match &get(&s, Kind::ForeignKey, Some("c"), name).detail {
        Detail::ForeignKey(fk) => (fk.deferrable, fk.initially_deferred),
        other => panic!("{other:?}"),
    };
    assert_eq!(fk("c_col_deferred_fkey"), (true, true));
    assert_eq!(
        fk("c_col_plain_fkey"),
        (true, false),
        "DEFERRABLE alone is immediate in SQLite"
    );
    assert_eq!(fk("c_col_immediate_fkey"), (true, false));
    assert_eq!(
        fk("c_col_not_fkey"),
        (false, false),
        "NOT DEFERRABLE wins over INITIALLY DEFERRED"
    );
    assert_eq!(fk("c_col_none_fkey"), (false, false));
    assert_eq!(
        fk("c_x_y_fkey"), // SQLite names the child column as declared (y), not as written in the constraint ("Y")
        (true, true),
        "a table-level, two-column key with a quoted column"
    );
}

#[test]
fn several_files_become_several_namespaces() {
    let primary = build("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT);");
    let cache = build(
        "CREATE TABLE users (id INTEGER PRIMARY KEY);
         CREATE TABLE entries (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users);
         CREATE INDEX entries_user ON entries (user_id);",
    );
    let s = snapshot_files(&[("primary", primary.db_path()), ("cache", cache.db_path())]).unwrap();
    assert!(s.get(Kind::Namespace, "primary", None, "primary").is_some());
    assert!(s.get(Kind::Namespace, "cache", None, "cache").is_some());
    // The same table name in two files stays two objects.
    assert!(
        s.get(Kind::Column, "primary", Some("users"), "email")
            .is_some()
    );
    assert!(
        s.get(Kind::Column, "cache", Some("users"), "email")
            .is_none()
    );
    assert!(
        s.get(Kind::Index, "cache", Some("entries"), "entries_user")
            .is_some()
    );
    match &s
        .get(
            Kind::ForeignKey,
            "cache",
            Some("entries"),
            "entries_user_id_fkey",
        )
        .unwrap()
        .detail
    {
        Detail::ForeignKey(fk) => {
            assert_eq!(
                (fk.ref_namespace.as_str(), fk.ref_columns.as_slice()),
                ("cache", &["id".to_string()][..])
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(counts(&s).get(&Kind::Table), Some(&3));
}

#[test]
fn a_connection_with_an_attached_database_is_read_whole() {
    let other = build("CREATE TABLE jobs (id INTEGER PRIMARY KEY, run_at TEXT NOT NULL);");
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY);")
        .unwrap();
    conn.execute(
        &format!("ATTACH DATABASE '{}' AS queue", other.db_path().display()),
        [],
    )
    .unwrap();
    let s = read(&conn).unwrap();
    assert!(s.get(Kind::Table, "main", None, "users").is_some());
    assert!(
        s.get(Kind::Column, "queue", Some("jobs"), "run_at")
            .is_some()
    );
}

#[test]
fn bad_namespace_names_are_refused() {
    let a = build("CREATE TABLE t (id INTEGER);");
    for files in [
        vec![("temp", a.db_path())],
        vec![("", a.db_path())],
        vec![("x", a.db_path()), ("X", a.db_path())],
        vec![],
    ] {
        assert!(
            snapshot_files(&files).is_err(),
            "{:?}",
            files.iter().map(|f| f.0).collect::<Vec<_>>()
        );
    }
}

#[test]
fn two_real_databases_diff_into_the_expected_rules() {
    use migate_core::diff::{classify, diff};
    let base = build(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT, status TEXT DEFAULT 'a', bio TEXT);
         CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users, body TEXT CHECK (body <> ''));
         CREATE VIEW active AS SELECT id FROM users WHERE status = 'a';",
    )
    .snapshot()
    .unwrap();
    // What a typical migration does: SQLite's own ALTERs plus a new table, an index and a dropped view.
    let head = build(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, legacy TEXT, status TEXT DEFAULT 'a', bio TEXT);
         CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER REFERENCES users, body TEXT CHECK (body <> ''));
         CREATE VIEW active AS SELECT id FROM users WHERE status = 'a';
         ALTER TABLE users DROP COLUMN legacy;
         ALTER TABLE users ADD COLUMN nickname TEXT;
         CREATE UNIQUE INDEX users_email ON users (email);
         CREATE TABLE audit (id INTEGER PRIMARY KEY, at TEXT NOT NULL, user_id INTEGER NOT NULL REFERENCES users);
         DROP VIEW active;",
    )
    .snapshot()
    .unwrap();
    let mut found: Vec<(&str, String)> = classify(&diff(&base, &head))
        .into_iter()
        .map(|f| (f.rule, f.object))
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("MG201", "main.users.legacy".to_string()),
            ("MG210", "main.users.nickname".to_string()),
            ("MG302", "main.users.users_email".to_string()),
            ("MG501", "main.active".to_string()),
        ],
        "renumbered columns (bio moves up) and the new table's contents must not be findings"
    );
}

#[test]
fn a_shorter_declared_length_is_not_narrowing_on_sqlite() {
    use migate_core::diff::{classify, diff};
    let base = build("CREATE TABLE t (id INTEGER PRIMARY KEY, name VARCHAR(20));")
        .snapshot()
        .unwrap();
    let head = build("CREATE TABLE t (id INTEGER PRIMARY KEY, name VARCHAR(5));")
        .snapshot()
        .unwrap();
    // SQLite stores 'a much longer name' in VARCHAR(5) unchanged, so the old app keeps working: not MG205.
    let rules: Vec<&str> = classify(&diff(&base, &head))
        .into_iter()
        .map(|f| f.rule)
        .collect();
    assert_eq!(rules, ["MG206"]);
}

#[test]
fn a_virtual_tables_module_and_arguments_are_listed_outside_the_model() {
    // The model sees a virtual table's columns, not its module or arguments (an FTS5 tokenizer, say).
    let sb = build(
        "CREATE VIRTUAL TABLE docs USING fts5(title, body, tokenize = 'porter'); CREATE TABLE t (a);",
    );
    assert!(
        snapshot(sb.db_path()).is_ok(),
        "the bundled SQLite reads an FTS5 table"
    );
    let got = migate_engine_sqlite::unmodelled(sb.db_path()).unwrap();
    assert_eq!(got.keys().collect::<Vec<_>>(), ["virtual table main.docs"]);
    assert!(got["virtual table main.docs"].contains("porter"), "{got:?}");
}
