// What a Node guard would do: PGlite in memory + pglite-socket on a free port, run the user's migrate
// command (migrate_stub.sh) with DATABASE_URL as a subprocess, then stop. $2 = start | full.
import { PGlite } from '@electric-sql/pglite';
import { PGLiteSocketServer } from '@electric-sql/pglite-socket';
const t0 = performance.now();
const db = await PGlite.create();
const port = 20000 + Math.floor(Math.random() * 20000);
const server = new PGLiteSocketServer({ db, port, host: '127.0.0.1' });
await server.start();
const t1 = performance.now();
const { spawn } = await import('node:child_process');
const code = await new Promise((res) => {
  const p = spawn(new URL('./migrate_stub.sh', import.meta.url).pathname, [process.argv[2] ?? 'start'],
    { stdio: 'inherit', env: { ...process.env, DATABASE_URL: `postgresql://postgres:postgres@127.0.0.1:${port}/postgres` } });
  p.on('close', res);
});
const t2 = performance.now();
await server.stop(); await db.close();
console.error(`start_ms=${(t1 - t0).toFixed(0)} migrate_ms=${(t2 - t1).toFixed(0)} exit=${code}`);
process.exit(code);
