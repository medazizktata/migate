import { DatabaseSync } from 'node:sqlite';
import { introspect } from './core.mjs';
const t0 = performance.now();
const db = new DatabaseSync(process.argv[2], { readOnly: true });
const c = introspect((s, ...p) => db.prepare(s).all(...p));
console.log(JSON.stringify(c), 'introspect_ms=' + (performance.now() - t0).toFixed(2));
