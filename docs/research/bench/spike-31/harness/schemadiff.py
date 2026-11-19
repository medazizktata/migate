"""The schema-diff rules a release gate would apply, as a baseline for the mixed-version test.

    python schemadiff.py BEFORE AFTER      each a SQLite file path or a postgresql:// URL
Prints one finding per line and exits 1 when any rule fires, 0 when none does.
Rules: DROP_TABLE, DROP_COLUMN (a rename shows as a drop plus an add), ADD_NOT_NULL_NO_DEFAULT (new NOT NULL column with no
database default), SET_NOT_NULL (existing column becomes NOT NULL), TYPE_CHANGE.
"""
import sqlite3
import sys

IGNORED = {"django_migrations", "schema_migrations", "ar_internal_metadata", "payload_migrations", "d1_migrations"}


def read(target):
    """{table: {column: (type, notnull, default)}}"""
    out = {}
    if target.startswith("postgresql://"):
        import psycopg

        with psycopg.connect(target) as c:
            rows = c.execute(
                "SELECT table_name, column_name, data_type || coalesce('(' || character_maximum_length || ')', ''),"
                " is_nullable = 'NO', column_default FROM information_schema.columns WHERE table_schema = 'public'"
            ).fetchall()
        for t, col, typ, nn, d in rows:
            out.setdefault(t, {})[col] = (typ.lower(), bool(nn), d)
    else:
        c = sqlite3.connect(f"file:{target}?mode=ro", uri=True)
        tables = [r[0] for r in c.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")]
        for t in tables:
            for _cid, col, typ, nn, d, pk in c.execute(f'PRAGMA table_info("{t}")'):
                out.setdefault(t, {})[col] = (typ.lower(), bool(nn) or bool(pk), d)
        c.close()
    return {t: cols for t, cols in out.items() if t not in IGNORED}


def diff(before, after):
    found = []
    for t, cols in before.items():
        if t not in after:
            found.append(f"DROP_TABLE {t}")
            continue
        for col, (typ, nn, _d) in cols.items():
            if col not in after[t]:
                found.append(f"DROP_COLUMN {t}.{col}")
                continue
            typ2, nn2, _d2 = after[t][col]
            if typ2 != typ:
                found.append(f"TYPE_CHANGE {t}.{col} {typ} -> {typ2}")
            if nn2 and not nn:
                found.append(f"SET_NOT_NULL {t}.{col}")
    for t, cols in after.items():
        if t not in before:
            continue  # a new table cannot break the old code's queries on existing tables
        for col, (_typ, nn, d) in cols.items():
            if col not in before[t] and nn and d is None:
                found.append(f"ADD_NOT_NULL_NO_DEFAULT {t}.{col}")
    return found


def snapshot(src, dst):
    """Consistent copy of a SQLite database, WAL included (a plain file copy can miss everything still in the WAL)."""
    a, b = sqlite3.connect(f"file:{src}?mode=ro", uri=True), sqlite3.connect(dst)
    a.backup(b)
    a.close()
    b.close()


if __name__ == "__main__":
    before, after = read(sys.argv[1]), read(sys.argv[2])
    if not before or not after:  # an empty side compares as "nothing changed" and would hide every finding
        print(f"EMPTY_SCHEMA before={len(before)} tables after={len(after)} tables")
        sys.exit(2)
    findings = diff(before, after)
    for f in findings:
        print(f)
    sys.exit(1 if findings else 0)
