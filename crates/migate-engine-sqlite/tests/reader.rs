// The whole file is test code; clippy's allow-unwrap-in-tests covers #[test] functions but not their helpers.
#![allow(clippy::unwrap_used)]

//! The SQLite reader against real database files.

use std::collections::BTreeMap;
use std::fs;

use migate_core::schema::{Detail, IndexPart, Kind, Object, Snapshot, TypeFamily};
use migate_engine_sqlite::{Sandbox, snapshot};
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
    assert_eq!((name.family, name.length), (TypeFamily::Text, Some(80)));
    // A quoted column name with a comma, at the top level of the table body.
    assert_eq!(
        col("users", "odd, name").collation.as_deref(),
        Some("BINARY")
    );
    let price = col("users", "price").data_type;
    assert_eq!(
        (price.family, price.precision, price.scale),
        (TypeFamily::Decimal, Some(10), Some(2))
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
