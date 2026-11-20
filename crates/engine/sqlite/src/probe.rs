//! Judge a trigger by the rows its table already holds: write each one again, unchanged and under its own id, and see
//! whether the trigger refuses it. It all happens on an in-memory copy, so the database itself is never written.

use std::path::Path;

use migate_core::engine::TriggerProbe;
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, ffi, params_from_iter};

use crate::{Error, check_file, copy_of, quoted, trigger_shape};

/// At most this many rows are written again; a larger table is judged by its first rows, and the report says so.
const MAX_ROWS: usize = 10_000;

pub fn probe_trigger(path: &Path, table: &str, trigger: &str) -> Result<TriggerProbe, Error> {
    check_file(path)?;
    let db = copy_of(&[("main", path)])?;
    let sql: Option<String> = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = ?1 AND tbl_name = ?2 COLLATE NOCASE",
            [trigger, table],
            |r| r.get(0),
        )
        .optional()?;
    let Some(sql) = sql else {
        return Ok(TriggerProbe::Unknown(format!(
            "no trigger {trigger} on {table}"
        )));
    };
    let (timing, events) = trigger_shape(&sql);
    if timing.eq_ignore_ascii_case("INSTEAD OF") {
        return Ok(TriggerProbe::Unknown("a trigger on a view".into()));
    }
    // On the copy only: no other trigger may refuse, and no foreign key may cascade or block the temporary delete.
    db.execute_batch("PRAGMA foreign_keys = OFF")?;
    let others: Vec<String> = db
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'trigger' AND name <> ?1")?
        .query_map([trigger], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for other in others {
        db.execute_batch(&format!("DROP TRIGGER {}", quoted(&other)))?;
    }

    // Generated columns are recomputed, so they are not written; a virtual table's rows live elsewhere.
    let columns: Vec<(String, i64, i64)> = db
        .prepare("SELECT name, hidden, pk FROM pragma_table_xinfo(?1)")?
        .query_map([table], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    if columns.iter().any(|c| c.1 == 1) || columns.is_empty() {
        return Ok(TriggerProbe::Unknown("a virtual table".into()));
    }
    let writable: Vec<&str> = columns
        .iter()
        .filter(|c| c.1 == 0)
        .map(|c| c.0.as_str())
        .collect();
    let without_rowid: bool = db.query_row(
        "SELECT wr FROM pragma_table_list WHERE schema = 'main' AND name = ?1 COLLATE NOCASE",
        [table],
        |r| r.get::<_, i64>(0),
    )? != 0;
    // How a row is found and written again under its own id: its rowid (named outright, since SELECT * hides it
    // when no column aliases it), or a WITHOUT ROWID table's primary key, which is among the written columns.
    let mut key_columns: Vec<(i64, &str)> = columns
        .iter()
        .filter(|c| c.2 > 0)
        .map(|c| (c.2, c.0.as_str()))
        .collect();
    key_columns.sort_unstable();
    let key: Vec<String> = if without_rowid {
        key_columns.iter().map(|(_, c)| quoted(c)).collect()
    } else {
        // A column of the table's own may be called rowid, oid or _rowid_; the row id answers to any name left free.
        let free = ["rowid", "oid", "_rowid_"]
            .into_iter()
            .find(|alias| !columns.iter().any(|c| c.0.eq_ignore_ascii_case(alias)));
        let Some(alias) = free else {
            return Ok(TriggerProbe::Unknown(
                "the table's columns take every name of the row id".into(),
            ));
        };
        vec![alias.into()]
    };
    let t = quoted(table);
    let list = |cols: &[String]| cols.join(", ");
    let written: Vec<String> = writable.iter().map(|c| quoted(c)).collect();
    let find = key
        .iter()
        .enumerate()
        .map(|(i, k)| format!("{k} = ?{}", i + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let held = u64::try_from(db.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| {
        r.get::<_, i64>(0)
    })?)
    .unwrap_or(0);
    if held == 0 {
        return Ok(TriggerProbe::NoRows);
    }
    let rows: Vec<Vec<Value>> = {
        let n = key.len() + written.len();
        let mut stmt = db.prepare(&format!(
            "SELECT {}, {} FROM {t} LIMIT {MAX_ROWS}",
            list(&key),
            list(&written)
        ))?;
        stmt.query_map([], |r| (0..n).map(|i| r.get(i)).collect())?
            .collect::<Result<_, _>>()?
    };

    let has = |e: &str| events.iter().any(|x| x.eq_ignore_ascii_case(e));
    let update = format!(
        "UPDATE {t} SET {} WHERE {find}",
        written
            .iter()
            .map(|c| format!("{c} = {c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let delete = format!("DELETE FROM {t} WHERE {find}");
    let count = format!("SELECT count(*) FROM {t} WHERE {find}");
    let (insert_columns, from) = if without_rowid {
        (written.clone(), key.len())
    } else {
        ([key.clone(), written.clone()].concat(), 0)
    };
    let insert = format!(
        "INSERT INTO {t} ({}) VALUES ({})",
        list(&insert_columns),
        (1..=insert_columns.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let (mut refused, mut first, mut unknown) = (0u64, None::<String>, None::<String>);
    // Each row in a savepoint of its own; outside a transaction, SAVEPOINT starts one, and RAISE(ROLLBACK) ends it.
    for row in &rows {
        let k = &row[..key.len()];
        db.execute_batch("SAVEPOINT probe")?;
        let outcome = (|| -> rusqlite::Result<Option<&'static str>> {
            if has("UPDATE") {
                db.execute(&update, params_from_iter(k))?;
            }
            if has("INSERT") || has("DELETE") {
                db.execute(&delete, params_from_iter(k))?;
                if db.query_row(&count, params_from_iter(k), |r| r.get::<_, i64>(0))? != 0 {
                    return Ok(Some(
                        "the row was not deleted (the trigger ignored or undid the delete)",
                    ));
                }
                db.execute(&insert, params_from_iter(&row[from..]))?;
                if db.query_row(&count, params_from_iter(k), |r| r.get::<_, i64>(0))? == 0 {
                    return Ok(Some(
                        "the row did not stay written (the trigger ignored or undid the write)",
                    ));
                }
            }
            Ok(None)
        })();
        match outcome {
            Ok(None) => {}
            Ok(Some(why)) => {
                refused += 1;
                first.get_or_insert_with(|| why.into());
            }
            // RAISE(ABORT|FAIL|ROLLBACK, message): the trigger's own refusal, told apart from any constraint by its code.
            Err(rusqlite::Error::SqliteFailure(e, message))
                if e.extended_code == ffi::SQLITE_CONSTRAINT_TRIGGER =>
            {
                refused += 1;
                first.get_or_insert_with(|| message.unwrap_or_default());
            }
            Err(e) => {
                unknown.get_or_insert_with(|| e.to_string());
            }
        }
        if !db.is_autocommit() {
            db.execute_batch("ROLLBACK TO probe; RELEASE probe")?;
        }
    }
    let checked = rows.len() as u64;
    Ok(match (refused, unknown) {
        (0, Some(why)) => TriggerProbe::Unknown(format!(
            "writing a row again failed outside the trigger: {why}"
        )),
        (0, None) => TriggerProbe::Accepted { checked, held },
        (refused, _) => TriggerProbe::Refused {
            refused,
            checked,
            held,
            message: first.unwrap_or_default(),
        },
    })
}
