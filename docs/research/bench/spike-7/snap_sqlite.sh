#!/bin/bash
# usage: snap_sqlite.sh <db-file>  -> normalised schema text on stdout. Reads a COPY (never opens the subject).
# copies are left in spike7/copies (no deletes on this machine).
set -euo pipefail
src="$1"; [ -f "$src" ] || { echo "no such db: $src" >&2; exit 2; }
tmp=$(mktemp -d "$(dirname "$0")/../copies/c.XXXXXX")
cp "$src" "$tmp/db"; for x in -wal -shm; do if [ -f "$src$x" ]; then cp "$src$x" "$tmp/db$x"; fi; done
q() { sqlite3 -batch -noheader "$tmp/db" "$1"; }
echo "## objects"
q "SELECT type||' '||name||' ON '||tbl_name||' :: '||coalesce(replace(replace(sql,char(10),' '),'  ',' '),'(auto)') FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' AND name <> '_cf_KV' ORDER BY type,name;"
for t in $(q "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name <> '_cf_KV' ORDER BY name;"); do
  q "SELECT 'col $t.'||name||' '||type||' notnull='||\"notnull\"||' dflt='||coalesce(dflt_value,'NULL')||' pk='||pk||' hidden='||hidden FROM pragma_table_xinfo('$t') ORDER BY cid;"
  q "SELECT 'idx $t.'||name||' unique='||\"unique\"||' origin='||origin||' partial='||partial FROM pragma_index_list('$t') ORDER BY name;"
  q "SELECT 'fk $t.'||\"from\"||' -> '||\"table\"||'.'||coalesce(\"to\",'?')||' upd='||on_update||' del='||on_delete FROM pragma_foreign_key_list('$t') ORDER BY id,seq;"
done
