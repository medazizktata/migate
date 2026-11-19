import { Database } from 'bun:sqlite';
import { introspect } from './core.mjs';
const t0 = performance.now();
const db = new Database(process.argv[2], { readonly: true });
const c = introspect((s, ...p) => db.query(s).all(...p));
console.log(JSON.stringify(c), 'introspect_ms=' + (performance.now() - t0).toFixed(2));
