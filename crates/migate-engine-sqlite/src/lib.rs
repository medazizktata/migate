//! migate's SQLite engine (also Cloudflare D1's local files and libSQL): reads a database's schema into the canonical
//! [`Snapshot`], and makes throwaway databases.
//!
//! Each database is opened read-only and copied into memory with SQLite's backup API before anything is read: the
//! source is never written (not even a `-shm` file), and data still in the write-ahead log is included, which a plain
//! file copy would miss (`docs/research/spike-31.md`).
//!
//! A project can keep several SQLite files (Rails 8 keeps its cache, queue and cable databases apart from the primary
//! one): [`snapshot_files`] reads them into one snapshot, one namespace per file, the way SQLite itself names attached
//! databases.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use migate_core::engine;
use migate_core::schema::{
    Column, DataType, Detail, ForeignKey, Generated, Index, IndexPart, Kind, Object, Snapshot,
    TypeFamily,
};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use sha2::{Digest, Sha256};

mod features;
mod probe;
mod sql;
use sql::{Tok, first_group, split_top_level, squash, tokens};

/// The kinds this reader can see. SQLite has no routines, enums or sequences, so they are never "not checked".
pub const CAPABILITIES: [Kind; 10] = [
    Kind::Namespace,
    Kind::Table,
    Kind::Column,
    Kind::PrimaryKey,
    Kind::Unique,
    Kind::Check,
    Kind::ForeignKey,
    Kind::Index,
    Kind::View,
    Kind::Trigger,
];

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error(e.to_string())
    }
}

/// Read the schema of the SQLite database at `path`, as namespace `main`.
///
/// A missing or zero-byte file is an error, never an empty snapshot: an empty snapshot compares as "nothing changed",
/// which would turn a migration that never ran (or ran against another file) into a pass.
pub fn snapshot(path: &Path) -> Result<Snapshot, Error> {
    snapshot_files(&[("main", path)])
}

/// Read several SQLite files into one snapshot, each as the namespace given with it (for example `primary`, `cache`,
/// `queue`). Names must be unique and cannot be `temp`.
pub fn snapshot_files(files: &[(&str, &Path)]) -> Result<Snapshot, Error> {
    let mut seen = BTreeSet::new();
    for (ns, path) in files {
        if ns.is_empty() || ns.eq_ignore_ascii_case("temp") || !seen.insert(ns.to_ascii_lowercase())
        {
            return Err(Error(format!(
                "namespace `{ns}` is empty, reserved (temp) or given twice"
            )));
        }
        check_file(path)?;
    }
    if files.is_empty() {
        return Err(Error("no database files given".into()));
    }
    let copy = copy_of(files)?;
    let names: Vec<&str> = files.iter().map(|(ns, _)| *ns).collect();
    read_namespaces(&copy, &names)
}

/// How many rows `sql` returns on the database at `path`, read through the same read-only copy as [`snapshot`].
pub fn count_rows(path: &Path, sql: &str) -> Result<usize, Error> {
    check_file(path)?;
    let copy = copy_of(&[("main", path)])?;
    let mut stmt = copy.prepare(sql)?;
    let mut rows = stmt.query([])?;
    let mut n = 0;
    while rows.next()?.is_some() {
        n += 1;
    }
    Ok(n)
}

/// The row count of every table in the database at `path` (namespace `main`), read through the read-only copy.
pub fn row_counts(path: &Path) -> Result<BTreeMap<String, u64>, Error> {
    check_file(path)?;
    let copy = copy_of(&[("main", path)])?;
    let tables: Vec<String> = copy
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut counts = BTreeMap::new();
    for t in tables {
        let n: i64 = copy.query_row(&format!("SELECT count(*) FROM {}", quoted(&t)), [], |r| {
            r.get(0)
        })?;
        counts.insert(format!("main.{t}"), u64::try_from(n).unwrap_or(0));
    }
    Ok(counts)
}

/// Whether the database at `path` was last written by a newer SQLite than the one compiled into migate, and which.
/// Bytes 96-99 of the file header hold the last writer's version number; SQLite writes them and never checks them.
pub fn newer_writer(path: &Path) -> Option<String> {
    let writer = writer_version(path)?;
    let reader = u32::try_from(rusqlite::version_number()).unwrap_or(u32::MAX);
    (writer > reader).then(|| {
        format!(
            "the migrations ran with SQLite {}, newer than the {} migate reads the database with",
            features::dotted(writer),
            features::dotted(reader)
        )
    })
}

/// The version of the SQLite that last wrote the database at `path`: bytes 96-99 of the file header.
fn writer_version(path: &Path) -> Option<u32> {
    use std::io::Read;
    let mut header = [0u8; 100];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    Some(u32::from_be_bytes([
        header[96], header[97], header[98], header[99],
    ]))
}

/// What the model does not cover in the database at `path`, as `category name` -> definition: a virtual table's module
/// and arguments (its columns are modelled).
pub fn unmodelled(path: &Path) -> Result<BTreeMap<String, String>, Error> {
    check_file(path)?;
    let copy = copy_of(&[("main", path)])?;
    let found = copy
        .prepare("SELECT name, sql FROM sqlite_schema WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL TABLE%'")?
        .query_map([], |r| Ok((format!("virtual table main.{}", r.get::<_, String>(0)?), squash(&r.get::<_, String>(1)?))))?
        .collect::<Result<_, _>>()?;
    Ok(found)
}

/// Every column's count of non-NULL values in the database at `path`, by `main.table.column`.
pub fn value_counts(path: &Path) -> Result<BTreeMap<String, u64>, Error> {
    check_file(path)?;
    let copy = copy_of(&[("main", path)])?;
    let tables: Vec<String> = copy
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut counts = BTreeMap::new();
    for t in tables {
        let columns: Vec<String> = copy
            .prepare("SELECT name FROM pragma_table_info(?1)")?
            .query_map([&t], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if columns.is_empty() {
            continue;
        }
        let list: Vec<String> = columns
            .iter()
            .map(|c| format!("count({})", quoted(c)))
            .collect();
        let row: Vec<i64> = copy.query_row(
            &format!("SELECT {} FROM {}", list.join(", "), quoted(&t)),
            [],
            |r| (0..columns.len()).map(|i| r.get(i)).collect(),
        )?;
        for (c, n) in columns.iter().zip(row) {
            counts.insert(format!("main.{t}.{c}"), u64::try_from(n).unwrap_or(0));
        }
    }
    Ok(counts)
}

/// Drop every table and view in the database at `path` except those named in `keep` (a query, such as the migrator's
/// history query). Writes to `path`: only for a throwaway database.
pub fn drop_all_but(path: &Path, keep: &str) -> Result<(), Error> {
    let kept: BTreeSet<String> = tokens(keep)
        .iter()
        .filter_map(|t| t.ident().map(str::to_ascii_lowercase))
        .collect();
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let objects: Vec<(String, String)> = conn
        .prepare("SELECT type, name FROM sqlite_schema WHERE type IN ('view', 'table') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\' ORDER BY type DESC")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (kind, name) in objects {
        if !kept.contains(&name.to_ascii_lowercase()) {
            conn.execute_batch(&format!(
                // IF EXISTS: dropping a virtual table drops its shadow tables, still listed here.
                "DROP {} IF EXISTS {};",
                kind.to_ascii_uppercase(),
                quoted(&name)
            ))?;
        }
    }
    Ok(())
}

/// An in-memory copy of each file under its namespace, taken with SQLite's backup API from a read-only connection.
fn copy_of(files: &[(&str, &Path)]) -> Result<Connection, Error> {
    let mut copy = Connection::open_in_memory()?;
    for (ns, path) in files {
        let source = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        if !ns.eq_ignore_ascii_case("main") {
            copy.execute(&format!("ATTACH DATABASE ':memory:' AS {}", quoted(ns)), [])?;
        }
        Backup::new_with_names(&source, "main", &mut copy, *ns)?.run_to_completion(
            i32::MAX,
            Duration::ZERO,
            None,
        )?;
    }
    Ok(copy)
}

fn check_file(path: &Path) -> Result<(), Error> {
    let meta = std::fs::metadata(path).map_err(|e| Error(format!("{}: {e}", path.display())))?;
    if !meta.is_file() {
        return Err(Error(format!("{}: not a file", path.display())));
    }
    if meta.len() == 0 {
        return Err(Error(format!(
            "{}: empty file, not a database (did the migrations run against another file?)",
            path.display()
        )));
    }
    Ok(())
}

/// Read every database attached to an open connection (`main` and any `ATTACH`ed one; `temp` is skipped).
pub fn read(conn: &Connection) -> Result<Snapshot, Error> {
    let names: Vec<String> = conn
        .prepare("SELECT name FROM pragma_database_list WHERE name <> 'temp' ORDER BY seq")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    read_namespaces(conn, &names)
}

/// An SQL identifier, quoted.
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn read_namespaces(conn: &Connection, namespaces: &[&str]) -> Result<Snapshot, Error> {
    let version: String = conn.query_row("SELECT sqlite_version()", [], |r| r.get(0))?;
    let mut objects = Vec::new();
    for ns in namespaces {
        read_namespace(conn, ns, &mut objects)?;
    }
    Snapshot::new(
        "sqlite",
        Some(version),
        CAPABILITIES.into_iter().collect(),
        objects,
    )
    .map_err(|e| Error(e.0))
}

fn read_namespace(conn: &Connection, ns: &str, objects: &mut Vec<Object>) -> Result<(), Error> {
    objects.push(Object {
        namespace: ns.into(),
        name: ns.into(),
        parent: None,
        detail: Detail::Namespace {},
    });

    let mut stmt = conn.prepare(&format!(
        "SELECT type, name, tbl_name, coalesce(sql, '') FROM {}.sqlite_schema \
         WHERE name NOT LIKE 'sqlite\\_%' ESCAPE '\\' AND name <> '_cf_KV' ORDER BY type, name",
        quoted(ns)
    ))?;
    let rows: Vec<(String, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;

    let flags: BTreeMap<String, (bool, bool)> = conn
        .prepare(
            "SELECT name, strict, wr FROM pragma_table_list WHERE schema = ?1 AND type = 'table'",
        )?
        .query_map([ns], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, i64>(1)? != 0, r.get::<_, i64>(2)? != 0),
            ))
        })?
        .collect::<Result<_, _>>()?;

    for (kind, name, tbl, text) in &rows {
        match kind.as_str() {
            "table" => {
                let (strict, without_rowid) = flags.get(name).copied().unwrap_or((false, false));
                objects.push(Object {
                    namespace: ns.into(),
                    name: name.clone(),
                    parent: None,
                    detail: Detail::Table {
                        strict,
                        without_rowid,
                    },
                });
                read_table(conn, ns, name, text, without_rowid, objects)?;
            }
            "view" => objects.push(Object {
                namespace: ns.into(),
                name: name.clone(),
                parent: None,
                detail: Detail::View {
                    definition: squash(text),
                    materialized: false,
                },
            }),
            "trigger" => {
                let (timing, events) = trigger_shape(text);
                objects.push(Object {
                    namespace: ns.into(),
                    name: name.clone(),
                    parent: Some(tbl.clone()),
                    detail: Detail::Trigger {
                        timing,
                        events,
                        definition: squash(text),
                    },
                });
            }
            _ => {} // indexes are read per table, so their origin (explicit or a UNIQUE constraint) is known
        }
    }
    Ok(())
}

/// A `pragma_table_xinfo` row: cid, name, declared type, not null, default, pk position, hidden.
type XinfoRow = (i64, String, String, bool, Option<String>, i64, i64);

/// One column pair of a foreign key: parent table, from, to (absent means the parent's primary key), on update, on delete.
type FkPart = (String, String, Option<String>, String, String);

/// What only the `CREATE TABLE` text says about one column definition.
#[derive(Debug, Default)]
struct ColumnText {
    generated: Option<String>,
    collation: Option<String>,
    autoincrement: bool,
}

/// What only the `CREATE TABLE` text says about a table.
#[derive(Debug, Default)]
struct TableText {
    /// Lower-cased column name -> what its definition says.
    columns: BTreeMap<String, ColumnText>,
    /// (name, expression) of every CHECK constraint, table- or column-level.
    checks: Vec<(String, String)>,
    /// Lower-cased, comma-joined child columns of a foreign key -> (deferrable, initially deferred).
    deferral: BTreeMap<String, (bool, bool)>,
}

fn read_table(
    conn: &Connection,
    ns: &str,
    table: &str,
    text: &str,
    without_rowid: bool,
    out: &mut Vec<Object>,
) -> Result<(), Error> {
    let parent = Some(table.to_string());
    let tt = parse_create_table(table, text);

    // Columns: hidden 0 normal, 1 hidden (virtual tables, skipped), 2 virtual generated, 3 stored generated.
    let mut stmt = conn.prepare(
        "SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden FROM pragma_table_xinfo(?1, ?2) ORDER BY cid",
    )?;
    let cols: Vec<XinfoRow> = stmt
        .query_map([table, ns], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, i64>(3)? != 0,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let pk: Vec<(i64, String, String)> = {
        let mut v: Vec<(i64, String, String)> = cols
            .iter()
            .filter(|c| c.5 > 0)
            .map(|c| (c.5, c.1.clone(), c.2.clone()))
            .collect();
        v.sort();
        v
    };
    // A single INTEGER PRIMARY KEY is an alias of the rowid: an identity column.
    let rowid_alias = pk.len() == 1 && pk[0].2.eq_ignore_ascii_case("INTEGER");

    let mut ordinal = 0u32;
    for (_, name, declared, notnull, default, pk_pos, hidden) in &cols {
        if *hidden == 1 {
            continue;
        }
        let from_text = tt.columns.get(&name.to_ascii_lowercase());
        let generated = match hidden {
            2 | 3 => Some(Generated {
                expression: from_text
                    .and_then(|c| c.generated.clone())
                    .unwrap_or_default(),
                stored: *hidden == 3,
            }),
            _ => None,
        };
        out.push(Object {
            namespace: ns.into(),
            name: name.clone(),
            parent: parent.clone(),
            detail: Detail::Column(Column {
                ordinal,
                data_type: data_type(declared),
                // SQLite lets a PRIMARY KEY column hold NULL unless it is the rowid alias or the table has no rowid.
                nullable: !notnull && !(*pk_pos > 0 && (rowid_alias || without_rowid)),
                default: default.clone(),
                generated,
                identity: (*pk_pos > 0 && rowid_alias)
                    || from_text.is_some_and(|c| c.autoincrement),
                collation: from_text.and_then(|c| c.collation.clone()),
            }),
        });
        ordinal += 1;
    }

    if !pk.is_empty() {
        out.push(Object {
            namespace: ns.into(),
            name: format!("{table}_pkey"),
            parent: parent.clone(),
            detail: Detail::PrimaryKey {
                columns: pk.iter().map(|p| p.1.clone()).collect(),
            },
        });
    }

    for (name, expression) in &tt.checks {
        out.push(Object {
            namespace: ns.into(),
            name: name.clone(),
            parent: parent.clone(),
            detail: Detail::Check {
                expression: expression.clone(),
                validated: true,
            },
        });
    }

    // Foreign keys: SQLite keeps no constraint names, so the name is derived from the columns (stable across rebuilds).
    let mut stmt = conn.prepare(
        "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete FROM pragma_foreign_key_list(?1, ?2) ORDER BY id, seq",
    )?;
    let rows: Vec<(i64, String, String, Option<String>, String, String)> = stmt
        .query_map([table, ns], |r| {
            Ok((
                r.get(0)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut fks: BTreeMap<i64, Vec<FkPart>> = BTreeMap::new();
    for (id, ref_table, from, to, on_update, on_delete) in rows {
        fks.entry(id)
            .or_default()
            .push((ref_table, from, to, on_update, on_delete));
    }
    for parts in fks.values() {
        let ref_table = parts[0].0.clone();
        let columns: Vec<String> = parts.iter().map(|p| p.1.clone()).collect();
        let mut ref_columns: Vec<String> = parts.iter().filter_map(|p| p.2.clone()).collect();
        if ref_columns.len() != columns.len() {
            // `REFERENCES parent` without columns means the parent's primary key.
            ref_columns = primary_key_of(conn, ns, &ref_table)?;
        }
        let key = columns.join(",").to_ascii_lowercase();
        let (deferrable, initially_deferred) = tt.deferral.get(&key).copied().unwrap_or_default();
        out.push(Object {
            namespace: ns.into(),
            name: format!("{table}_{}_fkey", columns.join("_")),
            parent: parent.clone(),
            detail: Detail::ForeignKey(ForeignKey {
                columns,
                // SQLite foreign keys cannot cross databases.
                ref_namespace: ns.into(),
                ref_table,
                ref_columns,
                on_delete: parts[0].4.clone(),
                on_update: parts[0].3.clone(),
                deferrable,
                initially_deferred,
                validated: true,
            }),
        });
    }

    // Indexes: origin c = CREATE INDEX, u = a UNIQUE constraint, pk = the primary key (already read).
    let mut stmt =
        conn.prepare("SELECT name, \"unique\", origin, partial FROM pragma_index_list(?1, ?2)")?;
    let indexes: Vec<(String, bool, String, bool)> = stmt
        .query_map([table, ns], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, i64>(1)? != 0,
                r.get(2)?,
                r.get::<_, i64>(3)? != 0,
            ))
        })?
        .collect::<Result<_, _>>()?;
    for (index, unique, origin, partial) in indexes {
        let mut stmt = conn.prepare(
            "SELECT cid, coalesce(name, ''), \"desc\" FROM pragma_index_xinfo(?1, ?2) WHERE key = 1 ORDER BY seqno",
        )?;
        let keys: Vec<(i64, String, bool)> = stmt
            .query_map([index.as_str(), ns], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0))
            })?
            .collect::<Result<_, _>>()?;
        match origin.as_str() {
            "u" => {
                let columns: Vec<String> = keys.into_iter().map(|k| k.1).collect();
                out.push(Object {
                    namespace: ns.into(),
                    name: format!("{table}_{}_key", columns.join("_")),
                    parent: parent.clone(),
                    detail: Detail::Unique { columns },
                });
            }
            "c" => {
                let text: String = conn.query_row(
                    &format!(
                        "SELECT coalesce(sql, '') FROM {}.sqlite_schema WHERE type = 'index' AND name = ?1",
                        quoted(ns)
                    ),
                    [&index],
                    |r| r.get(0),
                )?;
                let (expressions, predicate) = index_text(&text);
                let parts = keys
                    .into_iter()
                    .enumerate()
                    .map(|(i, (cid, name, descending))| {
                        if cid == -2 {
                            IndexPart::Expression {
                                sql: expressions.get(i).cloned().unwrap_or_default(),
                                descending,
                            }
                        } else {
                            IndexPart::Column { name, descending }
                        }
                    })
                    .collect();
                out.push(Object {
                    namespace: ns.into(),
                    name: index,
                    parent: parent.clone(),
                    detail: Detail::Index(Index {
                        unique,
                        parts,
                        predicate: if partial { predicate } else { None },
                        method: None,
                    }),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn primary_key_of(conn: &Connection, ns: &str, table: &str) -> Result<Vec<String>, Error> {
    let mut stmt =
        conn.prepare("SELECT name FROM pragma_table_info(?1, ?2) WHERE pk > 0 ORDER BY pk")?;
    let v: Vec<String> = stmt
        .query_map([table, ns], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(v)
}

/// Column details, CHECK constraints and foreign-key deferral that exist only in the `CREATE TABLE` text.
fn parse_create_table(table: &str, text: &str) -> TableText {
    let mut tt = TableText::default();
    let Some((body, _)) = first_group(text) else {
        return tt;
    };
    for item in split_top_level(&body) {
        let toks = tokens(&item);
        let Some(first) = toks.first() else { continue };
        let is_constraint = ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"]
            .iter()
            .any(|k| first.is_kw(k));
        let column = if is_constraint {
            None
        } else {
            first.ident().map(str::to_ascii_lowercase)
        };
        let mut info = ColumnText::default();
        let mut pending_name: Option<String> = None;
        // A table-level FOREIGN KEY (cols): its child columns, for matching the deferral to pragma_foreign_key_list.
        let mut fk_columns: Option<String> = column.clone();
        let mut i = usize::from(column.is_some());
        while i < toks.len() {
            let t = &toks[i];
            if t.is_kw("CONSTRAINT") {
                pending_name = toks.get(i + 1).and_then(Tok::ident).map(str::to_string);
                i += 2;
                continue;
            }
            let group = match toks.get(i + 1) {
                Some(Tok::Group(expr)) => Some(expr),
                _ => None,
            };
            if t.is_kw("CHECK")
                && let Some(expr) = group
            {
                let expression = squash(expr);
                // Unnamed checks get a name from their expression, stable when other checks are added or reordered.
                let name = pending_name.take().unwrap_or_else(|| {
                    let digest = Sha256::digest(expression.as_bytes());
                    let short: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
                    format!("{table}_check_{short}")
                });
                tt.checks.push((name, expression));
                i += 2;
                continue;
            }
            if t.is_kw("KEY")
                && column.is_none()
                && let Some(cols) = group
            {
                fk_columns = Some(child_columns(cols));
            }
            if t.is_kw("REFERENCES") {
                let deferral = deferral(&toks[i..]);
                if let (Some(cols), Some(d)) = (&fk_columns, deferral) {
                    tt.deferral.insert(cols.clone(), d);
                }
            }
            if t.is_kw("AS")
                && let Some(expr) = group
            {
                info.generated = Some(squash(expr));
            }
            if t.is_kw("COLLATE") {
                info.collation = toks.get(i + 1).and_then(Tok::ident).map(str::to_string);
            }
            if t.is_kw("AUTOINCREMENT") {
                info.autoincrement = true;
            }
            i += 1;
        }
        if let Some(name) = column {
            tt.columns.insert(name, info);
        }
    }
    tt
}

/// `a, "B"` -> `a,b`: child columns of a table-level foreign key, as the deferral map keys them.
fn child_columns(list: &str) -> String {
    split_top_level(list)
        .iter()
        .filter_map(|c| {
            tokens(c)
                .first()
                .and_then(|t| t.ident().map(str::to_ascii_lowercase))
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// After `REFERENCES …`: `(deferrable, initially deferred)` if a deferral clause is written, else `None`.
/// `NOT DEFERRABLE` is not deferrable; SQLite defers only with `INITIALLY DEFERRED`.
fn deferral(after_references: &[Tok]) -> Option<(bool, bool)> {
    let pos = after_references
        .iter()
        .position(|t| t.is_kw("DEFERRABLE"))?;
    let negated = pos > 0 && after_references[pos - 1].is_kw("NOT");
    let initially_deferred = after_references[pos + 1..]
        .windows(2)
        .next()
        .is_some_and(|w| w[0].is_kw("INITIALLY") && w[1].is_kw("DEFERRED"));
    Some((!negated, !negated && initially_deferred))
}

/// From `CREATE [UNIQUE] INDEX … ON t (part, …) [WHERE predicate]`: each part's text and the predicate.
fn index_text(text: &str) -> (Vec<String>, Option<String>) {
    let Some((list, rest)) = first_group(text) else {
        return (Vec::new(), None);
    };
    let parts = split_top_level(&list)
        .into_iter()
        .map(|p| {
            // A trailing ASC/DESC is recorded as `descending`, not as part of the expression.
            let p = p.trim();
            let upper = p.to_ascii_uppercase();
            let cut = [" DESC", " ASC"]
                .iter()
                .find_map(|s| upper.ends_with(s).then(|| p.len() - s.len()));
            squash(&p[..cut.unwrap_or(p.len())])
        })
        .collect();
    let rest = rest.trim();
    let predicate = rest
        .get(..5)
        .filter(|w| w.eq_ignore_ascii_case("WHERE"))
        .map(|_| squash(rest[5..].trim_end_matches(';')));
    (parts, predicate)
}

/// Timing and events from `CREATE TRIGGER name [BEFORE|AFTER|INSTEAD OF] INSERT|DELETE|UPDATE [OF …] ON …`.
fn trigger_shape(text: &str) -> (String, Vec<String>) {
    let toks = tokens(text);
    let mut timing = "BEFORE".to_string(); // SQLite's default when none is written
    let mut events = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if t.is_kw("ON") {
            break;
        }
        if t.is_kw("BEFORE") || t.is_kw("AFTER") {
            timing = t.ident().unwrap_or_default().to_ascii_uppercase();
        } else if t.is_kw("INSTEAD") && toks.get(i + 1).is_some_and(|n| n.is_kw("OF")) {
            timing = "INSTEAD OF".into();
        } else if ["INSERT", "UPDATE", "DELETE"].iter().any(|k| t.is_kw(k)) {
            events.push(t.ident().unwrap_or_default().to_ascii_uppercase());
        }
    }
    (timing, events)
}

/// A declared SQLite type: its text, and SQLite's type affinity refined into a family.
fn data_type(declared: &str) -> DataType {
    let up = declared.to_ascii_uppercase();
    let has = |s: &str| up.contains(s);
    // SQLite's affinity rules, in order (sqlite.org/datatype3.html §3.1), refined where the declared name says more.
    let family = if has("INT") {
        TypeFamily::Integer
    } else if has("CHAR") || has("CLOB") || has("TEXT") {
        TypeFamily::Text
    } else if has("BLOB") {
        TypeFamily::Binary
    } else if up.trim().is_empty() {
        TypeFamily::Other
    } else if has("REAL") || has("FLOA") || has("DOUB") {
        TypeFamily::Float
    } else if has("BOOL") {
        TypeFamily::Boolean
    } else if has("TIMESTAMP") || has("DATETIME") {
        TypeFamily::Timestamp
    } else if has("DATE") {
        TypeFamily::Date
    } else if has("TIME") {
        TypeFamily::Time
    } else if has("JSON") {
        TypeFamily::Json
    } else if has("UUID") {
        TypeFamily::Uuid
    } else {
        TypeFamily::Decimal
    };
    DataType {
        raw: declared.to_string(),
        family,
        // SQLite enforces no declared size (sqlite.org/datatype3.html §3), so none is a limit here.
        length: None,
        precision: None,
        scale: None,
    }
}

/// The SQLite engine behind the engine port: a database file the project's migrate command writes, at `db` unless a
/// pattern names where the tool writes it.
#[derive(Debug)]
pub struct Throwaway {
    db: PathBuf,
    pattern: Option<String>,
}

impl Throwaway {
    pub fn new(db: PathBuf, pattern: Option<String>) -> Throwaway {
        Throwaway { db, pattern }
    }
}

impl engine::Sandbox for Throwaway {
    fn placeholders(&self) -> Vec<(&'static str, String)> {
        vec![("db", self.db.to_string_lossy().into_owned())]
    }

    fn open(&self, history: &str) -> Result<Box<dyn engine::Database + '_>, String> {
        let path = locate(self.pattern.as_deref(), &self.db, history)?
            .ok_or_else(|| format!("no database at {}", self.db.display()))?;
        Ok(Box::new(File(path)))
    }

    fn newer_syntax(&self, sql: &str, target: &str) -> Result<Vec<(u32, String, String)>, String> {
        let target = target_number(target)?;
        Ok(features::newer_than(sql, target)
            .into_iter()
            .map(|(line, feature, since)| (line, feature.to_string(), features::dotted(since)))
            .collect())
    }
}

fn target_number(target: &str) -> Result<u32, String> {
    features::version_number(target).ok_or_else(|| {
        format!("target_version `{target}` is not a SQLite version such as 3.37 or 3.37.2")
    })
}

/// The database file the migrate command wrote.
#[derive(Debug)]
struct File(PathBuf);

impl engine::Database for File {
    fn unmodelled(&self) -> Result<BTreeMap<String, String>, String> {
        unmodelled(&self.0).map_err(|e| e.0)
    }
    fn newer_writer(&self) -> Option<String> {
        newer_writer(&self.0)
    }
    fn probe_trigger(&self, table: &str, trigger: &str) -> Result<engine::TriggerProbe, String> {
        probe::probe_trigger(&self.0, table, trigger).map_err(|e| e.0)
    }
    fn target_gap(&self, target: &str) -> Result<Option<migate_core::Finding>, String> {
        let target_n = target_number(target)?;
        Ok(writer_version(&self.0).filter(|&w| w > target_n).map(|w| {
            migate_core::Finding::new(
                "MG806",
                migate_core::Severity::Note,
                "sandbox",
                format!(
                    "the migrations ran on SQLite {}, production runs {target}: syntax newer than production's is \
                     checked (MG701), but behaviour that differs between the two versions was not run",
                    features::dotted(w)
                ),
            )
        }))
    }
    fn snapshot(&self) -> Result<Snapshot, String> {
        snapshot(&self.0).map_err(|e| e.0)
    }
    fn count_rows(&self, sql: &str) -> Result<usize, String> {
        count_rows(&self.0, sql).map_err(|e| e.0)
    }
    fn row_counts(&self) -> Result<BTreeMap<String, u64>, String> {
        row_counts(&self.0).map_err(|e| e.0)
    }
    fn value_counts(&self) -> Result<BTreeMap<String, u64>, String> {
        value_counts(&self.0).map_err(|e| e.0)
    }
    fn drop_all_but(&self, keep: &str) -> Result<(), String> {
        drop_all_but(&self.0, keep).map_err(|e| e.0)
    }
}

/// The database file: `db`, or the file `pattern` names (one `*` in the file name may match several; the one holding
/// the migration history is the database).
fn locate(pattern: Option<&str>, db: &Path, history: &str) -> Result<Option<PathBuf>, String> {
    let Some(pattern) = pattern else {
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
        hits.retain(|p| count_rows(p, history).is_ok());
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

/// A throwaway SQLite database: a fresh directory holding a database path the project's migrate command writes to.
/// The directory is removed when the sandbox is dropped.
#[derive(Debug)]
pub struct Sandbox {
    dir: PathBuf,
    db: PathBuf,
}

impl Sandbox {
    pub fn create() -> std::io::Result<Sandbox> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "migate-sqlite-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir)?;
        let db = dir.join("sandbox.db");
        Ok(Sandbox { dir, db })
    }

    /// Where the database lives (not created until something writes to it).
    pub fn db_path(&self) -> &Path {
        &self.db
    }

    /// A connection URL for migrate commands that take one (`file:` form).
    pub fn url(&self) -> String {
        format!("file:{}", self.db.display())
    }

    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        snapshot(&self.db)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
