# Postgres twin of gen_schema.py: 100 tables, 200 indexes, FKs, 5 triggers, 5 views.
import random
random.seed(42)
out = ["CREATE FUNCTION noop() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;"]
types = ["integer", "text", "double precision", "numeric", "bytea", "text NOT NULL DEFAULT ''", "integer NOT NULL DEFAULT 0"]
for t in range(100):
    cols = ["id integer PRIMARY KEY"]
    for c in range(random.randint(5, 12)):
        cols.append(f"c{c} {random.choice(types)}")
    fks = []
    if t > 0:
        for k in range(random.randint(1, 3)):
            ref = random.randint(0, t - 1)
            cols.append(f"t{ref}_id_{k} integer")
            fks.append(f"FOREIGN KEY (t{ref}_id_{k}) REFERENCES t{ref}(id) ON DELETE CASCADE")
    cols.append("created_at timestamptz NOT NULL DEFAULT now()")
    body = ",\n  ".join(cols + fks + ["CHECK (c0 IS NULL OR length(c0::text) < 1000)"])
    out.append(f"CREATE TABLE t{t} (\n  {body}\n);")
    out.append(f"CREATE INDEX t{t}_c1_idx ON t{t}(c1);")
    out.append(f"CREATE UNIQUE INDEX t{t}_c2_created_uq ON t{t}(c2, created_at);")
for i in range(5):
    out.append(f"CREATE TRIGGER trg{i} BEFORE INSERT ON t{i} FOR EACH ROW EXECUTE FUNCTION noop();")
for i in range(5):
    out.append(f"CREATE VIEW v{i} AS SELECT a.id, a.c1, b.id AS bid FROM t{i+10} a JOIN t{i+11} b ON b.id = a.id;")
print("\n".join(out))
