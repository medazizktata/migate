// One throwaway PGlite cycle: start (in memory), optionally apply the 100-table schema, read counts, close.
import { PGlite } from '@electric-sql/pglite';
import { readFileSync } from 'node:fs';
const full = process.argv[2] === 'full';
const t0 = performance.now();
const db = await PGlite.create();
await db.query('select 1');
const t1 = performance.now();
let counts = '';
if (full) {
  await db.exec(readFileSync(new URL('../schema_pg.sql', import.meta.url), 'utf8'));
  counts = JSON.stringify((await db.query(readFileSync(new URL('./counts.sql', import.meta.url), 'utf8'))).rows[0]);
}
const t2 = performance.now();
await db.close();
console.log(counts, `start_ms=${(t1 - t0).toFixed(0)} schema+counts_ms=${(t2 - t1).toFixed(0)}`);
