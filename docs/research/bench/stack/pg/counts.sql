-- One row of counts so a fast run cannot be an empty run.
SELECT
 (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind='r') AS tables,
 (SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind='v') AS views,
 (SELECT count(*) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND NOT t.tgisinternal) AS triggers,
 (SELECT count(*) FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','v') AND a.attnum>0 AND NOT a.attisdropped) AS columns,
 (SELECT count(*) FROM pg_index i JOIN pg_class c ON c.oid=i.indrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public') AS indexes,
 (SELECT count(*) FROM pg_constraint k JOIN pg_namespace n ON n.oid=k.connamespace WHERE n.nspname='public' AND k.contype='f') AS fks,
 (SELECT count(*) FROM pg_constraint k JOIN pg_namespace n ON n.oid=k.connamespace WHERE n.nspname='public' AND k.contype='c') AS checks;
