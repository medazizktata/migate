import { parse } from 'libpg-query';
import { readFileSync } from 'node:fs';
const sql = readFileSync(process.argv[2], 'utf8');
const t0 = performance.now();
const r = await parse(sql);
console.log(`stmts=${r.stmts.length} parse_ms=${(performance.now() - t0).toFixed(1)}`);
