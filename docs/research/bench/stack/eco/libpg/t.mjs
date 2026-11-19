import { parse } from 'libpg-query';
const t0 = performance.now();
const r = await parse('ALTER TABLE t1 DROP COLUMN c2; CREATE INDEX CONCURRENTLY i ON t1(c1);');
console.log(r.stmts.length, Object.keys(r.stmts[0].stmt)[0], (performance.now()-t0).toFixed(1)+'ms');
