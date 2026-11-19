// Shared introspection: q(sql, ...params) -> array of row objects. Returns counts so an empty run is visible.
export function introspect(q) {
  const objs = q("SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'");
  const c = { objects: objs.length, tables: 0, views: 0, triggers: 0, columns: 0, indexes: 0, indexColumns: 0, fks: 0 };
  for (const o of objs) {
    if (o.type === 'trigger') c.triggers++;
    if (o.type === 'view') { c.views++; c.columns += q('SELECT * FROM pragma_table_xinfo(?)', o.name).length; }
    if (o.type !== 'table') continue;
    c.tables++;
    c.columns += q('SELECT * FROM pragma_table_xinfo(?)', o.name).length;
    for (const ix of q('SELECT * FROM pragma_index_list(?)', o.name)) {
      c.indexes++;
      c.indexColumns += q('SELECT * FROM pragma_index_xinfo(?)', ix.name).length;
    }
    c.fks += q('SELECT * FROM pragma_foreign_key_list(?)', o.name).length;
  }
  return c;
}
