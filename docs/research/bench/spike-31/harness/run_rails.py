"""Mixed-version test for the Rails sample, in the order a deploy happens:
  database at BASE -> a base process boots and warms up (pre-booted) -> migrate to HEAD -> the pre-booted process runs the
  base suite -> a FRESH base process runs the base suite -> head seeds rows -> base reads them back.

    python run_rails.py ENGINE FIXTURE RUNS [--maintain-test-schema] [--stub-tests]
Appends one JSON line per run to results.jsonl. The two flags exist only to calibrate the harness.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import time
import uuid

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from schemadiff import snapshot  # noqa: E402

S = os.environ["SPIKE_DIR"]  # the spike work folder
REPO, PY, H = f"{S}/rails-sample", f"{S}/.venv/bin/python", f"{S}/harness"
PGURL = "postgresql://postgres@127.0.0.1:54331/"
RUBY = "/opt/homebrew/opt/ruby/bin"
EXPECTED_TESTS = 4
PGARGS = ["-h", "127.0.0.1", "-p", "54331", "-U", "postgres"]


def sh(cmd, cwd=None, env=None, timeout=300):
    t = time.perf_counter()
    p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    return p.returncode, p.stdout + p.stderr, time.perf_counter() - t


def parse(code, out):
    m = re.search(r"^(\d+) runs, \d+ assertions, (\d+) failures, (\d+) errors", out, re.M)
    ran = int(m.group(1)) if m else 0
    failed = sorted(set(re.findall(r"^OrderTest#(test_\w+)", out, re.M)))
    errs = sorted(set(re.findall(r"^\s*((?:ActiveRecord|ActiveModel|PG|SQLite3|NoMethodError|KeyError)[\w:]*): (.{0,140})", out, re.M)))[:3]
    verdict = "INVALID" if ran != EXPECTED_TESTS else ("PASS" if code == 0 else "FAIL")
    return verdict, failed, errs


def one_run(engine, fixture, maintain=False, stub=False):
    rid = uuid.uuid4().hex[:8]
    wt = f"{S}/wt/r{rid}"
    if engine == "pg":
        db, before = f"rmv_{rid}", f"rmv_{rid}_before"
        url, url_before, diff_args = PGURL + db, PGURL + before, (PGURL + before, PGURL + db)
    else:
        db, before = f"{S}/dbs/rmv_{rid}.sqlite3", f"{S}/dbs/rmv_{rid}_before.sqlite3"
        url, url_before, diff_args = f"sqlite3:{db}", None, (before, db)
    env = dict(os.environ, PATH=f"{S}/gems/bin:{RUBY}:{os.environ['PATH']}", GEM_HOME=f"{S}/gems", GEM_PATH=f"{S}/gems",
               RAILS_ENV="test", DATABASE_URL=url, PARALLEL_WORKERS="1",
               MV_MAINTAIN_TEST_SCHEMA="1" if maintain else "0")
    r = {"project": "rails", "engine": engine, "fixture": fixture, "maintain_test_schema": maintain, "stub": stub, "run": rid}

    t0 = time.perf_counter()
    for name, rev in (("base", "base"), ("head", fixture)):
        code, out, _ = sh(["git", "worktree", "add", "--detach", f"{wt}/{name}", rev], cwd=REPO)
        assert code == 0, out
    if engine == "pg":
        assert sh(["createdb", *PGARGS, db])[0] == 0
    r["setup_s"] = time.perf_counter() - t0

    # the database as production has it: migrated by BASE
    code, out, r["migrate_base_s"] = sh(["bin/rails", "db:migrate"], cwd=f"{wt}/base", env=env)
    assert code == 0, out
    # snapshot for the schema diff (timed with the diff)
    t0 = time.perf_counter()
    if engine == "pg":
        assert sh(["createdb", *PGARGS, "-T", db, before])[0] == 0
    else:
        snapshot(db, before)
    snap_s = time.perf_counter() - t0

    # a live BASE process boots and warms up before the migration
    flags = f"{wt}/flags"
    os.makedirs(flags)
    f = lambda n: f"{flags}/{n}"  # noqa: E731
    def wait(n):
        while not os.path.exists(f(n)):
            assert pre.poll() is None or os.path.exists(f(n)), pre.stdout.read()
            time.sleep(0.02)
    late = "--migrate-before-boot" in sys.argv  # calibration only: the process boots AFTER the migration
    if late:
        assert sh(["bin/rails", "db:migrate"], cwd=f"{wt}/head", env=env)[0] == 0
    t0 = time.perf_counter()
    pre = subprocess.Popen(["ruby", f"{H}/preboot_test.rb", flags], cwd=f"{wt}/base", env=env,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    wait("ready")
    r["preboot_boot_s"] = time.perf_counter() - t0

    # migrate to HEAD under the live process, then let it run the base suite
    code, out, r["migrate_head_s"] = sh(["bin/rails", "db:migrate"], cwd=f"{wt}/head", env=env)
    assert code == 0, out
    r["migrate_before_boot"] = late
    t0 = time.perf_counter()
    open(f("go"), "w").close()
    wait("tests_done")
    r["preboot_test_s"] = time.perf_counter() - t0

    # a FRESH base process runs the same suite against the head-migrated database
    cmd = ["/bin/echo"] if stub else ["bin/rails", "test"]
    code, out, r["fresh_test_s"] = sh(cmd, cwd=f"{wt}/base", env=env)
    r["fresh"], r["fresh_failed"], r["fresh_errors"] = parse(code, out)
    # guard: the suite must have run against the head schema, not one Rails rebuilt from base's schema.rb
    code, out, _ = sh(["bin/rails", "runner", "p ActiveRecord::Base.connection_pool.migration_context.current_version"],
                      cwd=f"{wt}/head", env=env)
    r["db_still_head"] = out.strip().endswith("20260201000001") if fixture != "base" else True
    if not r["db_still_head"]:
        r["fresh"] = "INVALID"

    code, out, r["schemadiff_s"] = sh([PY, f"{H}/schemadiff.py", *diff_args])
    r["schemadiff_s"] += snap_s
    r["schemadiff_valid"] = code in (0, 1)
    r["schemadiff_flagged"], r["schemadiff_findings"] = code == 1, out.split("\n")[:-1]

    # rollback compatibility: head writes rows; the pre-booted base process and a fresh base process read them back
    code, out, r["seed_s"] = sh(["bin/rails", "seed_mv"], cwd=f"{wt}/head", env=env)
    r["seed_ok"] = code == 0
    open(f("seeded"), "w").close()
    out = pre.communicate(timeout=300)[0]
    r["preboot"], r["preboot_failed"], r["preboot_errors"] = parse(int(open(f("tests_rc")).read()), out)
    rb = {"0": "PASS", "1": "FAIL", "2": "INVALID"}
    r["readback_preboot"] = rb.get(open(f("readback_rc")).read().strip(), "ERROR") if os.path.exists(f("readback_rc")) else "ERROR"
    r["readback_preboot_findings"] = [line for line in out.split("\n") if line.startswith("READBACK")][:3]
    code, out, r["readback_s"] = sh(["bin/rails", "runner", f"{H}/readback_rails.rb"], cwd=f"{wt}/base", env=env)
    r["readback"] = rb.get(str(code), "ERROR")
    r["readback_findings"] = [line for line in out.split("\n") if line.startswith("READBACK")][:3]

    for name in ("base", "head"):
        sh(["git", "worktree", "remove", "--force", f"{wt}/{name}"], cwd=REPO)
    if engine == "pg":
        for d in (db, before):
            sh(["dropdb", *PGARGS, d])
    r["fresh_total_s"] = r["setup_s"] + r["migrate_base_s"] + r["migrate_head_s"] + r["fresh_test_s"]
    r["preboot_total_s"] = r["setup_s"] + r["migrate_base_s"] + r["preboot_boot_s"] + r["migrate_head_s"] + r["preboot_test_s"]
    return r


if __name__ == "__main__":
    engine, fixture, runs = sys.argv[1], sys.argv[2], int(sys.argv[3])
    with open(f"{S}/results.jsonl", "a") as f:
        for _ in range(runs):
            r = one_run(engine, fixture, maintain="--maintain-test-schema" in sys.argv, stub="--stub-tests" in sys.argv)
            f.write(json.dumps(r) + "\n")
            f.flush()
            print(fixture, engine, "| fresh", r["fresh"], r["fresh_failed"], "| preboot", r["preboot"], r["preboot_failed"],
                  r["preboot_errors"][:1], "| diff", r["schemadiff_findings"], "| readback fresh", r["readback"],
                  r["readback_findings"][:1], "preboot", r["readback_preboot"], r["readback_preboot_findings"][:1], f"| fresh {r['fresh_total_s']:.1f}s preboot {r['preboot_total_s']:.1f}s")
