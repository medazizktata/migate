// Probe: can one SELECT per object kind read a full SQLite schema through table-valued pragma functions?
// Usage: node sqlite-introspect.mjs [extra-sql]   (extra-sql is applied after the base schema, to show a diff fires)
import { DatabaseSync } from 'node:sqlite'

const db = new DatabaseSync(':memory:')
db.exec(`
CREATE TABLE users(id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE, name TEXT DEFAULT 'x',
  age INT CHECK (age >= 0), full TEXT GENERATED ALWAYS AS (email || name) VIRTUAL);
CREATE TABLE posts(id INTEGER PRIMARY KEY, user_id INT REFERENCES users(id) ON DELETE CASCADE, body TEXT, deleted INT);
CREATE INDEX posts_live ON posts(user_id) WHERE deleted = 0;
CREATE INDEX users_lower ON users(lower(email));
CREATE VIEW live_posts AS SELECT * FROM posts WHERE deleted = 0;
CREATE TRIGGER no_empty BEFORE INSERT ON posts WHEN NEW.body = '' BEGIN SELECT RAISE(ABORT, 'empty'); END;
`)
if (process.argv[2]) db.exec(process.argv[2])

const q = (sql) => db.prepare(sql).all().map((r) => ({ ...r }))
const snapshot = {
  objects: q(`SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name`),
  tables: q(`SELECT name, type, ncol, wr, strict FROM pragma_table_list WHERE schema='main' AND name NOT LIKE 'sqlite_%' ORDER BY name`),
  columns: q(`SELECT m.name AS tbl, c.* FROM sqlite_schema m JOIN pragma_table_xinfo(m.name) c
              WHERE m.type IN ('table','view') AND m.name NOT LIKE 'sqlite_%' ORDER BY tbl, c.cid`),
  indexes: q(`SELECT m.name AS tbl, il.* FROM sqlite_schema m JOIN pragma_index_list(m.name) il
              WHERE m.type='table' ORDER BY tbl, il.name`),
  indexCols: q(`SELECT il.name AS idx, ix.* FROM sqlite_schema m JOIN pragma_index_list(m.name) il
                JOIN pragma_index_xinfo(il.name) ix WHERE m.type='table' ORDER BY idx, ix.seqno`),
  fks: q(`SELECT m.name AS tbl, fk.* FROM sqlite_schema m JOIN pragma_foreign_key_list(m.name) fk
          WHERE m.type='table' ORDER BY tbl, fk.id, fk.seq`),
}
console.log(JSON.stringify(snapshot, null, 1))
