// Probe: can a SUBPROCESS migrator reach PGlite through pglite-socket via a plain DATABASE_URL, and can we then read the
// schema (incl. triggers, enums, partial indexes, views) from pg_catalog? argv[2] = extra migration SQL (to show a diff fires).
import { PGlite } from '@electric-sql/pglite'
import { PGLiteSocketServer } from '@electric-sql/pglite-socket'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
// NB: the migrator must be spawned ASYNC: PGlite serves the socket from this process's event loop, so a sync spawn deadlocks.

const t0 = performance.now()
const db = await PGlite.create() // in-memory
const server = new PGLiteSocketServer({ db, port: 0, host: '127.0.0.1' })
await server.start()
const port = server.getServerConn().split(':').pop()
const url = `postgresql://postgres:postgres@127.0.0.1:${port}/postgres?sslmode=disable`
const tStart = performance.now() - t0

// The "user's migrate command": a separate Node process using the ordinary `pg` driver and DATABASE_URL only.
const migrator = `
const { Client } = require('pg'); const c = new Client({ connectionString: process.env.DATABASE_URL });
(async () => { await c.connect(); await c.query(process.env.MIGRATION); await c.end(); })().catch(e => { console.error(e.message); process.exit(1) })`
const base = `
CREATE TYPE mood AS ENUM ('ok','sad');
CREATE TABLE users(id serial PRIMARY KEY, email text NOT NULL UNIQUE, m mood, age int CHECK (age >= 0),
  full_name text GENERATED ALWAYS AS (email || '!') STORED);
CREATE TABLE posts(id bigserial PRIMARY KEY, user_id int REFERENCES users(id) ON DELETE CASCADE, deleted bool DEFAULT false);
CREATE INDEX posts_live ON posts(user_id) WHERE NOT deleted;
CREATE VIEW live_posts AS SELECT * FROM posts WHERE NOT deleted;
CREATE FUNCTION no_op() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
CREATE TRIGGER t_posts BEFORE INSERT ON posts FOR EACH ROW EXECUTE FUNCTION no_op();`
const run = (sql) => promisify(execFile)(process.execPath, ['-e', migrator], {
  cwd: process.cwd(), env: { ...process.env, DATABASE_URL: url, MIGRATION: sql }, timeout: 60000 })
const t1 = performance.now()
await run(base)
if (process.argv[2]) await run(process.argv[2])
const tMigrate = performance.now() - t1

const q = async (sql) => (await db.query(sql)).rows
const ns = `n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%'`
const snap = {
  columns: await q(`SELECT c.relname tbl, c.relkind kind, a.attname col, format_type(a.atttypid,a.atttypmod) type, a.attnotnull nn,
     pg_get_expr(d.adbin,d.adrelid) dflt, a.attgenerated gen FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid
     JOIN pg_namespace n ON n.oid=c.relnamespace LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
     WHERE ${ns} AND c.relkind IN ('r','v','m','p') AND a.attnum>0 AND NOT a.attisdropped ORDER BY 1,a.attnum`),
  constraints: await q(`SELECT conrelid::regclass::text tbl, conname, contype, pg_get_constraintdef(oid) def FROM pg_constraint
     WHERE connamespace='public'::regnamespace ORDER BY 1,2`),
  indexes: await q(`SELECT tablename, indexname, indexdef FROM pg_indexes WHERE schemaname='public' ORDER BY 1,2`),
  views: await q(`SELECT viewname, definition FROM pg_views WHERE schemaname='public' ORDER BY 1`),
  triggers: await q(`SELECT tgrelid::regclass::text tbl, tgname, pg_get_triggerdef(oid) def FROM pg_trigger WHERE NOT tgisinternal ORDER BY 1,2`),
  enums: await q(`SELECT t.typname, array_agg(e.enumlabel ORDER BY e.enumsortorder)::text labels FROM pg_enum e JOIN pg_type t ON t.oid=e.enumtypid GROUP BY 1 ORDER BY 1`),
  sequences: await q(`SELECT sequencename, data_type::text FROM pg_sequences WHERE schemaname='public' ORDER BY 1`),
  functions: await q(`SELECT p.proname, pg_get_function_identity_arguments(p.oid) args FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public' ORDER BY 1`),
}
await server.stop(); await db.close()
console.log(JSON.stringify({ ms: { start: Math.round(tStart), migrate: Math.round(tMigrate) }, url: url.replace(/:postgres@/, ':***@'), snap }, null, 1))
