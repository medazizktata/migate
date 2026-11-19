#!/bin/bash
# usage: snap_pg.sh <dbname>  -> normalised schema of non-system schemas (catalog queries, read-only)
set -euo pipefail
psql -h 127.0.0.1 -p 54377 -U postgres -d "$1" -v ON_ERROR_STOP=1 -At <<'SQL'
\o /dev/null
SET search_path = pg_catalog;
\o
WITH ns AS (SELECT oid, nspname FROM pg_namespace WHERE nspname NOT IN ('pg_catalog','information_schema','pg_toast') AND nspname NOT LIKE 'pg_temp%' AND nspname NOT LIKE 'pg_toast_temp%')
SELECT * FROM (
 SELECT 'rel '||ns.nspname||'.'||c.relname||' kind='||c.relkind::text FROM pg_class c JOIN ns ON ns.oid=c.relnamespace WHERE c.relkind IN ('r','v','m','S','p','f')
 UNION ALL
 SELECT 'col '||ns.nspname||'.'||c.relname||'.'||a.attname||' '||format_type(a.atttypid,a.atttypmod)||' notnull='||a.attnotnull||' dflt='||coalesce(pg_get_expr(d.adbin,d.adrelid),'NULL')||' ident='||a.attidentity::text||' gen='||a.attgenerated::text
   FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN ns ON ns.oid=c.relnamespace LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
   WHERE a.attnum>0 AND NOT a.attisdropped AND c.relkind IN ('r','v','m','p','f')
 UNION ALL
 SELECT 'con '||ns.nspname||'.'||c.relname||'.'||k.conname||' '||pg_get_constraintdef(k.oid) FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN ns ON ns.oid=c.relnamespace
 UNION ALL
 SELECT 'idx '||ns.nspname||'.'||i.relname||' '||pg_get_indexdef(i.oid) FROM pg_index x JOIN pg_class i ON i.oid=x.indexrelid JOIN ns ON ns.oid=i.relnamespace
 UNION ALL
 SELECT 'view '||ns.nspname||'.'||c.relname||' '||replace(pg_get_viewdef(c.oid),E'\n',' ') FROM pg_class c JOIN ns ON ns.oid=c.relnamespace WHERE c.relkind IN ('v','m')
 UNION ALL
 SELECT 'trg '||ns.nspname||'.'||c.relname||'.'||t.tgname||' '||pg_get_triggerdef(t.oid) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN ns ON ns.oid=c.relnamespace WHERE NOT t.tgisinternal
 UNION ALL
 SELECT 'fn '||ns.nspname||'.'||p.proname||'('||pg_get_function_identity_arguments(p.oid)||') md5='||md5(pg_get_functiondef(p.oid)) FROM pg_proc p JOIN ns ON ns.oid=p.pronamespace WHERE p.prokind IN ('f','p')
 UNION ALL
 SELECT 'type '||ns.nspname||'.'||t.typname||' '||t.typtype::text||' '||coalesce((SELECT string_agg(e.enumlabel,',' ORDER BY e.enumsortorder) FROM pg_enum e WHERE e.enumtypid=t.oid),'') FROM pg_type t JOIN ns ON ns.oid=t.typnamespace WHERE t.typtype IN ('e','d')
 UNION ALL
 SELECT 'ext '||extname||' '||extversion FROM pg_extension
) s(line) ORDER BY 1;
SQL
