"""Mixed-version test for the Django sample: migrate a throwaway database with the HEAD checkout, run the BASE checkout's test
suite against that same database (`manage.py test --keepdb`, TEST.NAME = the migrated database), then the rollback probe
(head's `seed`, base's readback). Times each phase.

    python run_django.py ENGINE FIXTURE RUNS [--no-keepdb] [--stub-tests]
ENGINE sqlite|pg; FIXTURE a branch name or `base` (the control: head == base). Appends one JSON line per run to results.jsonl.
--no-keepdb and --stub-tests exist only to calibrate the harness (see the report).
"""
import json
import os
import re
import sqlite3
import subprocess
import sys
import time
import uuid

S = os.environ["SPIKE_DIR"]  # the spike work folder
REPO, PY, H = f"{S}/django-sample", f"{S}/.venv/bin/python", f"{S}/harness"
PG = "postgresql://postgres@127.0.0.1:54331/"
EXPECTED_TESTS = 4


def sh(cmd, cwd=None, env=None):
    t = time.perf_counter()
    p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    return p.returncode, p.stdout + p.stderr, time.perf_counter() - t


def last_migration(engine, db):
    try:
        return _last_migration(engine, db)
    except Exception:  # noqa: BLE001 - a missing database means the suite did not keep it
        return None


def _last_migration(engine, db):
    q = "SELECT name FROM django_migrations WHERE app='shop' ORDER BY id DESC LIMIT 1"
    if engine == "pg":
        import psycopg

        with psycopg.connect(PG + db) as c:
            return c.execute(q).fetchone()[0]
    c = sqlite3.connect(f"file:{db}.sqlite3?mode=ro", uri=True)
    try:
        return c.execute(q).fetchone()[0]
    finally:
        c.close()


def one_run(engine, fixture, keepdb=True, stub=False):
    rid = uuid.uuid4().hex[:8]
    wt = f"{S}/wt/{rid}"
    db = f"mv_{rid}" if engine == "pg" else f"{S}/dbs/mv_{rid}"
    env = dict(os.environ, MV_ENGINE=engine, MV_DB=db)
    r = {"project": "django", "engine": engine, "fixture": fixture, "keepdb": keepdb, "stub": stub, "run": rid, "guard_v2": True}

    # setup: two clean checkouts and an empty database
    t0 = time.perf_counter()
    for name, rev in (("base", "base"), ("head", fixture)):
        code, out, _ = sh(["git", "worktree", "add", "--detach", f"{wt}/{name}", rev], cwd=REPO)
        assert code == 0, out
    if engine == "pg":
        code, out, _ = sh(["createdb", "-h", "127.0.0.1", "-p", "54331", "-U", "postgres", db])
        assert code == 0, out
    r["setup_s"] = time.perf_counter() - t0

    # migrate: the HEAD version's migrations
    code, out, r["migrate_s"] = sh([PY, "manage.py", "migrate", "--noinput"], cwd=f"{wt}/head", env=env)
    assert code == 0, out
    head_last = last_migration(engine, db)

    # schema diff baseline needs the BASE schema too: a second database migrated with base (timed separately)
    before = f"{db}_before"
    if engine == "pg":
        sh(["createdb", "-h", "127.0.0.1", "-p", "54331", "-U", "postgres", before])
    env_b = dict(env, MV_DB=before)
    code, out, t_base_migrate = sh([PY, "manage.py", "migrate", "--noinput"], cwd=f"{wt}/base", env=env_b)
    assert code == 0, out
    target = (lambda d: PG + d) if engine == "pg" else (lambda d: d + ".sqlite3")
    code, out, t_diff = sh([PY, f"{H}/schemadiff.py", target(before), target(db)])
    r["schemadiff_valid"] = code in (0, 1)
    r["schemadiff_flagged"], r["schemadiff_findings"] = code == 1, out.split("\n")[:-1]
    r["schemadiff_s"] = t_base_migrate + t_diff

    # test: the BASE version's suite against the head-migrated database
    cmd = ["/bin/echo"] if stub else [PY, "manage.py", "test", "--noinput", "-v", "2"] + (["--keepdb"] if keepdb else [])
    code, out, r["test_s"] = sh(cmd, cwd=f"{wt}/base", env=env)
    ran = re.search(r"^Ran (\d+) tests?", out, re.M)
    r["tests_ran"] = int(ran.group(1)) if ran else 0
    r["failed_tests"] = sorted(set(re.findall(r"^(?:ERROR|FAIL): (\w+)", out, re.M)))
    r["errors"] = sorted(set(re.findall(r"^(\w+(?:\.\w+)*(?:Error|Exception|Violation)): (.+)$", out, re.M)))[:4]
    # guard: the suite must have run, and against the HEAD-migrated database (its last migration is still head's)
    r["db_still_head"] = last_migration(engine, db) == head_last
    if r["tests_ran"] != EXPECTED_TESTS or not r["db_still_head"]:
        r["mixed"] = "INVALID"
    else:
        r["mixed"] = "PASS" if code == 0 else "FAIL"

    # rollback compatibility: rows written by HEAD's code, read back through BASE's models
    code, out, r["seed_s"] = sh([PY, "manage.py", "seed"], cwd=f"{wt}/head", env=env)
    r["seed_ok"] = code == 0
    code, out, r["readback_s"] = sh([PY, f"{H}/readback_django.py", "shop"], cwd=f"{wt}/base", env=env)
    r["readback"] = {0: "PASS", 1: "FAIL", 2: "INVALID"}.get(code, "ERROR")
    r["readback_findings"] = [line for line in out.split("\n") if line.startswith("READBACK")][:3]

    t0 = time.perf_counter()
    for name in ("base", "head"):
        sh(["git", "worktree", "remove", "--force", f"{wt}/{name}"], cwd=REPO)
    if engine == "pg":
        for d in (db, before):
            sh(["dropdb", "-h", "127.0.0.1", "-p", "54331", "-U", "postgres", d])
    r["teardown_s"] = time.perf_counter() - t0
    r["mixed_total_s"] = r["setup_s"] + r["migrate_s"] + r["test_s"]
    return r


if __name__ == "__main__":
    engine, fixture, runs = sys.argv[1], sys.argv[2], int(sys.argv[3])
    os.makedirs(f"{S}/dbs", exist_ok=True)
    with open(f"{S}/results.jsonl", "a") as f:
        for _ in range(runs):
            r = one_run(engine, fixture, keepdb="--no-keepdb" not in sys.argv, stub="--stub-tests" in sys.argv)
            f.write(json.dumps(r) + "\n")
            f.flush()
            print(r["fixture"], r["engine"], "mixed", r["mixed"], r["failed_tests"], "| diff", r["schemadiff_findings"],
                  "| readback", r["readback"], r["readback_findings"][:1], f"| {r['mixed_total_s']:.2f}s")
