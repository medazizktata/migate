# In-process introspection time (excludes runtime start): 30 runs each, median/mean/stdev/min.
import subprocess, statistics, re, sys
D = sys.argv[1]
cmds = {"rust-rusqlite": ["bin/rusqlite", D], "go-mattn": ["bin/mattn", D], "go-modernc": ["bin/modernc", D],
        "node:sqlite": ["node", "node_sqlite.mjs", D], "bun:sqlite": ["bun", "bun_sqlite.mjs", D],
        "deno node:sqlite": ["deno", "run", "-A", "node_sqlite.mjs", D]}
for name, cmd in cmds.items():
    xs = []
    for _ in range(30):
        out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
        assert '"triggers":5' in out and '"views":5' in out and '"fks":197' in out, out  # not an empty run
        xs.append(float(re.search(r"introspect_ms=([\d.]+)", out).group(1)))
    print(f"{name:18} runs=30 median={statistics.median(xs):6.2f} mean={statistics.mean(xs):6.2f} sd={statistics.stdev(xs):5.2f} min={min(xs):6.2f} ms")
