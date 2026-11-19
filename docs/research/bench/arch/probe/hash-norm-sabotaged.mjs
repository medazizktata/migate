// Proposed ledger hash: sha256 over (BOM stripped, CRLF/CR -> LF) bytes. Shows: line endings and BOM do not change it; a real edit does.
import { createHash } from 'node:crypto'
export const norm = (s) => s.replace(/^﻿/, '')
export const h = (s, n = true) => createHash('sha256').update(n ? norm(s) : s).digest('hex').slice(0, 12)
const lf = 'ALTER TABLE a ADD b int;\nUPDATE a SET b = 1;\n'
const crlf = '﻿' + lf.replace(/\n/g, '\r\n')
const edited = lf.replace('b = 1', 'b = 2')
const row = (k, ok) => console.log(k.padEnd(34), ok ? 'PASS' : 'FAIL')
row('raw hash differs LF vs CRLF+BOM', h(lf, false) !== h(crlf, false))
row('normalised hash equal LF vs CRLF', h(lf) === h(crlf))
row('normalised hash still sees an edit', h(lf) !== h(edited))
