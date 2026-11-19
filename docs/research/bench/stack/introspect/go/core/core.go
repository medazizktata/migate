package core

import (
	"database/sql"
	"fmt"
)

func count(db *sql.DB, q string, arg string) (int, []string) {
	rows, err := db.Query(q, arg)
	if err != nil {
		panic(err)
	}
	defer rows.Close()
	cols, _ := rows.Columns()
	vals := make([]any, len(cols))
	ptrs := make([]any, len(cols))
	for i := range vals {
		ptrs[i] = &vals[i]
	}
	n := 0
	var names []string
	for rows.Next() {
		if err := rows.Scan(ptrs...); err != nil {
			panic(err)
		}
		n++
		for i, c := range cols {
			if c == "name" {
				names = append(names, fmt.Sprint(vals[i]))
			}
		}
	}
	return n, names
}

// Introspect mirrors core.mjs and returns counts so an empty run is visible.
func Introspect(db *sql.DB) string {
	type obj struct{ typ, name string }
	rows, err := db.Query("SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
	if err != nil {
		panic(err)
	}
	var objs []obj
	for rows.Next() {
		var t, n, tbl string
		var s sql.NullString
		if err := rows.Scan(&t, &n, &tbl, &s); err != nil {
			panic(err)
		}
		objs = append(objs, obj{t, n})
	}
	rows.Close()
	var tables, views, triggers, columns, indexes, indexColumns, fks int
	for _, o := range objs {
		switch o.typ {
		case "trigger":
			triggers++
		case "view":
			views++
			n, _ := count(db, "SELECT * FROM pragma_table_xinfo(?)", o.name)
			columns += n
		case "table":
			tables++
			n, _ := count(db, "SELECT * FROM pragma_table_xinfo(?)", o.name)
			columns += n
			ni, ixs := count(db, "SELECT * FROM pragma_index_list(?)", o.name)
			indexes += ni
			for _, ix := range ixs {
				n, _ := count(db, "SELECT * FROM pragma_index_xinfo(?)", ix)
				indexColumns += n
			}
			n, _ = count(db, "SELECT * FROM pragma_foreign_key_list(?)", o.name)
			fks += n
		}
	}
	return fmt.Sprintf(`{"objects":%d,"tables":%d,"views":%d,"triggers":%d,"columns":%d,"indexes":%d,"indexColumns":%d,"fks":%d}`,
		len(objs), tables, views, triggers, columns, indexes, indexColumns, fks)
}
