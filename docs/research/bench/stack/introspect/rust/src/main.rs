// Mirrors core.mjs: prints counts so an empty run is visible.
use rusqlite::{Connection, OpenFlags};
use std::time::Instant;

fn count(db: &Connection, q: &str, arg: &str) -> (usize, Vec<String>) {
    let mut st = db.prepare_cached(q).unwrap();
    let ncol = st.column_count();
    let name_idx = st.column_names().iter().position(|c| *c == "name");
    let mut rows = st.query([arg]).unwrap();
    let (mut n, mut names) = (0, vec![]);
    while let Some(r) = rows.next().unwrap() {
        n += 1;
        for i in 0..ncol { let _v: rusqlite::types::Value = r.get(i).unwrap(); }
        if let Some(i) = name_idx { if let Some(s) = r.get::<_, Option<String>>(i).unwrap() { names.push(s); } }
    }
    (n, names)
}

fn main() {
    let t0 = Instant::now();
    let path = std::env::args().nth(1).unwrap();
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut st = db.prepare("SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'").unwrap();
    let objs: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|x| x.unwrap()).collect();
    let (mut tables, mut views, mut triggers, mut columns, mut indexes, mut index_columns, mut fks) = (0, 0, 0, 0, 0, 0, 0);
    for (typ, name) in &objs {
        match typ.as_str() {
            "trigger" => triggers += 1,
            "view" => { views += 1; columns += count(&db, "SELECT * FROM pragma_table_xinfo(?)", name).0; }
            "table" => {
                tables += 1;
                columns += count(&db, "SELECT * FROM pragma_table_xinfo(?)", name).0;
                let (ni, ixs) = count(&db, "SELECT * FROM pragma_index_list(?)", name);
                indexes += ni;
                for ix in ixs { index_columns += count(&db, "SELECT * FROM pragma_index_xinfo(?)", &ix).0; }
                fks += count(&db, "SELECT * FROM pragma_foreign_key_list(?)", name).0;
            }
            _ => {}
        }
    }
    println!("{{\"objects\":{},\"tables\":{},\"views\":{},\"triggers\":{},\"columns\":{},\"indexes\":{},\"indexColumns\":{},\"fks\":{}}} introspect_ms={:.2}",
        objs.len(), tables, views, triggers, columns, indexes, index_columns, fks, t0.elapsed().as_secs_f64() * 1000.0);
}
