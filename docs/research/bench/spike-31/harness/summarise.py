"""Aggregates results.jsonl: outcome per project x engine x fixture (with every distinct outcome, to show flakiness) and
timings (mean, min, max over the runs). Calibration runs (stub, no-keepdb, maintain-test-schema, migrate-before-boot,
skip-head) are listed separately."""
import json
import os
import statistics as st
import sys
from collections import defaultdict

S = os.environ["SPIKE_DIR"]  # the spike work folder
rows = [json.loads(line) for line in open(f"{S}/results.jsonl")]
# trial runs made before a harness fix (Rails: before the readback moved into the pre-booted process; payload-d1: before the
# WAL-safe snapshot) are left out of the tables
trial = lambda r: (r["project"] == "rails" and "readback_preboot" not in r) or (r["project"] == "payload-d1" and "schemadiff_valid" not in r)  # noqa: E731
print("trial runs left out:", sum(map(trial, rows)))
rows = [r for r in rows if not trial(r)]
calib = lambda r: r.get("stub") or r.get("keepdb") is False or r.get("maintain_test_schema") or r.get("migrate_before_boot") or r.get("skip_head")  # noqa: E731
OUT = {"django": ["mixed", "readback"], "rails": ["fresh", "preboot", "readback", "readback_preboot"],
       "payload-d1": ["sql", "admin_int", "api_int", "chain", "mixed"]}
TIMES = {"django": ["setup_s", "migrate_s", "test_s", "mixed_total_s", "schemadiff_s", "seed_s", "readback_s"],
         "rails": ["setup_s", "migrate_base_s", "migrate_head_s", "fresh_test_s", "fresh_total_s", "preboot_boot_s",
                   "preboot_test_s", "preboot_total_s", "schemadiff_s"],
         "payload-d1": ["install_s", "migrate_base_s", "setup_s", "migrate_s", "sql_s", "admin_int_s", "api_int_s", "test_s",
                 "mixed_total_s", "schemadiff_s", "chain_s"]}
groups = defaultdict(list)
for r in rows:
    if not calib(r) and r["fixture"] not in ("",):
        groups[(r["project"], r["engine"], r["fixture"])].append(r)
want = sys.argv[1] if len(sys.argv) > 1 else None
for (p, e, f), rs in sorted(groups.items()):
    if want and p != want:
        continue
    outs = {k: dict(sorted({str(o): [x.get(k) for x in rs].count(o) for o in set(x.get(k) for x in rs)}.items())) for k in OUT[p]}
    diffs = {tuple(x["schemadiff_findings"]) for x in rs}
    print(f"{p:6} {e:8} {f:22} n={len(rs)} " + " ".join(f"{k}={v}" for k, v in outs.items()) + f" diff={sorted(diffs)}")
    print("        " + "  ".join(f"{t[:-2]}={st.mean(x[t] for x in rs):.2f}[{min(x[t] for x in rs):.2f}-{max(x[t] for x in rs):.2f}]"
                                 for t in TIMES[p] if all(t in x for x in rs)))
print("\ncalibration runs:")
for r in rows:
    if calib(r) and (not want or r["project"] == want):
        flags = {k: r[k] for k in ("stub", "keepdb", "maintain_test_schema", "migrate_before_boot", "skip_head") if k in r and r[k] not in (None,)}
        print(" ", r["project"], r["engine"], r["fixture"], flags, {k: r.get(k) for k in OUT[r["project"]]}, r.get("db_still_head", ""))
