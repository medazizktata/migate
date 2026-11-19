CREATE FUNCTION noop() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
CREATE TABLE t0 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 text NOT NULL DEFAULT '',
  c2 double precision,
  c3 text,
  c4 text,
  c5 text,
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t0_c1_idx ON t0(c1);
CREATE UNIQUE INDEX t0_c2_created_uq ON t0(c2, created_at);
CREATE TABLE t1 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 text NOT NULL DEFAULT '',
  c2 bytea,
  c3 integer,
  c4 bytea,
  c5 numeric,
  t0_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t0_id_0) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t1_c1_idx ON t1(c1);
CREATE UNIQUE INDEX t1_c2_created_uq ON t1(c2, created_at);
CREATE TABLE t2 (
  id integer PRIMARY KEY,
  c0 text,
  c1 text,
  c2 bytea,
  c3 bytea,
  c4 integer,
  c5 bytea,
  t1_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t1_id_0) REFERENCES t1(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t2_c1_idx ON t2(c1);
CREATE UNIQUE INDEX t2_c2_created_uq ON t2(c2, created_at);
CREATE TABLE t3 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 bytea,
  c2 double precision,
  c3 integer NOT NULL DEFAULT 0,
  c4 integer NOT NULL DEFAULT 0,
  c5 integer,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer NOT NULL DEFAULT 0,
  t2_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t2_id_0) REFERENCES t2(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t3_c1_idx ON t3(c1);
CREATE UNIQUE INDEX t3_c2_created_uq ON t3(c2, created_at);
CREATE TABLE t4 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 double precision,
  c2 text,
  c3 text,
  c4 integer NOT NULL DEFAULT 0,
  c5 double precision,
  c6 integer,
  c7 integer,
  c8 numeric,
  c9 integer,
  c10 double precision,
  t2_id_0 integer,
  t0_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t2_id_0) REFERENCES t2(id) ON DELETE CASCADE,
  FOREIGN KEY (t0_id_1) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t4_c1_idx ON t4(c1);
CREATE UNIQUE INDEX t4_c2_created_uq ON t4(c2, created_at);
CREATE TABLE t5 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 integer,
  c2 numeric,
  c3 integer,
  c4 bytea,
  c5 double precision,
  c6 integer NOT NULL DEFAULT 0,
  c7 text NOT NULL DEFAULT '',
  c8 bytea,
  c9 integer NOT NULL DEFAULT 0,
  c10 double precision,
  c11 bytea,
  t0_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t0_id_0) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t5_c1_idx ON t5(c1);
CREATE UNIQUE INDEX t5_c2_created_uq ON t5(c2, created_at);
CREATE TABLE t6 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 text,
  c2 integer NOT NULL DEFAULT 0,
  c3 double precision,
  c4 integer,
  t0_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t0_id_0) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t6_c1_idx ON t6(c1);
CREATE UNIQUE INDEX t6_c2_created_uq ON t6(c2, created_at);
CREATE TABLE t7 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 numeric,
  c2 text NOT NULL DEFAULT '',
  c3 integer NOT NULL DEFAULT 0,
  c4 double precision,
  c5 text,
  c6 double precision,
  c7 double precision,
  c8 text,
  c9 text NOT NULL DEFAULT '',
  c10 double precision,
  t5_id_0 integer,
  t5_id_1 integer,
  t0_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t5_id_0) REFERENCES t5(id) ON DELETE CASCADE,
  FOREIGN KEY (t5_id_1) REFERENCES t5(id) ON DELETE CASCADE,
  FOREIGN KEY (t0_id_2) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t7_c1_idx ON t7(c1);
CREATE UNIQUE INDEX t7_c2_created_uq ON t7(c2, created_at);
CREATE TABLE t8 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text NOT NULL DEFAULT '',
  c2 text,
  c3 text,
  c4 numeric,
  c5 numeric,
  c6 double precision,
  t3_id_0 integer,
  t5_id_1 integer,
  t0_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t3_id_0) REFERENCES t3(id) ON DELETE CASCADE,
  FOREIGN KEY (t5_id_1) REFERENCES t5(id) ON DELETE CASCADE,
  FOREIGN KEY (t0_id_2) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t8_c1_idx ON t8(c1);
CREATE UNIQUE INDEX t8_c2_created_uq ON t8(c2, created_at);
CREATE TABLE t9 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 integer,
  c2 integer NOT NULL DEFAULT 0,
  c3 double precision,
  c4 numeric,
  c5 double precision,
  c6 integer,
  c7 text,
  t5_id_0 integer,
  t3_id_1 integer,
  t7_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t5_id_0) REFERENCES t5(id) ON DELETE CASCADE,
  FOREIGN KEY (t3_id_1) REFERENCES t3(id) ON DELETE CASCADE,
  FOREIGN KEY (t7_id_2) REFERENCES t7(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t9_c1_idx ON t9(c1);
CREATE UNIQUE INDEX t9_c2_created_uq ON t9(c2, created_at);
CREATE TABLE t10 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 numeric,
  c2 text,
  c3 double precision,
  c4 text,
  c5 text,
  c6 text NOT NULL DEFAULT '',
  c7 bytea,
  c8 bytea,
  c9 double precision,
  c10 text NOT NULL DEFAULT '',
  t6_id_0 integer,
  t9_id_1 integer,
  t6_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t6_id_0) REFERENCES t6(id) ON DELETE CASCADE,
  FOREIGN KEY (t9_id_1) REFERENCES t9(id) ON DELETE CASCADE,
  FOREIGN KEY (t6_id_2) REFERENCES t6(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t10_c1_idx ON t10(c1);
CREATE UNIQUE INDEX t10_c2_created_uq ON t10(c2, created_at);
CREATE TABLE t11 (
  id integer PRIMARY KEY,
  c0 text,
  c1 text,
  c2 bytea,
  c3 numeric,
  c4 integer,
  c5 integer NOT NULL DEFAULT 0,
  c6 integer,
  c7 integer NOT NULL DEFAULT 0,
  c8 integer,
  c9 text,
  t2_id_0 integer,
  t10_id_1 integer,
  t6_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t2_id_0) REFERENCES t2(id) ON DELETE CASCADE,
  FOREIGN KEY (t10_id_1) REFERENCES t10(id) ON DELETE CASCADE,
  FOREIGN KEY (t6_id_2) REFERENCES t6(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t11_c1_idx ON t11(c1);
CREATE UNIQUE INDEX t11_c2_created_uq ON t11(c2, created_at);
CREATE TABLE t12 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 numeric,
  c2 bytea,
  c3 numeric,
  c4 bytea,
  c5 double precision,
  t0_id_0 integer,
  t10_id_1 integer,
  t11_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t0_id_0) REFERENCES t0(id) ON DELETE CASCADE,
  FOREIGN KEY (t10_id_1) REFERENCES t10(id) ON DELETE CASCADE,
  FOREIGN KEY (t11_id_2) REFERENCES t11(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t12_c1_idx ON t12(c1);
CREATE UNIQUE INDEX t12_c2_created_uq ON t12(c2, created_at);
CREATE TABLE t13 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 bytea,
  c2 integer NOT NULL DEFAULT 0,
  c3 double precision,
  c4 integer NOT NULL DEFAULT 0,
  c5 text NOT NULL DEFAULT '',
  t1_id_0 integer,
  t4_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t1_id_0) REFERENCES t1(id) ON DELETE CASCADE,
  FOREIGN KEY (t4_id_1) REFERENCES t4(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t13_c1_idx ON t13(c1);
CREATE UNIQUE INDEX t13_c2_created_uq ON t13(c2, created_at);
CREATE TABLE t14 (
  id integer PRIMARY KEY,
  c0 text,
  c1 numeric,
  c2 integer,
  c3 text NOT NULL DEFAULT '',
  c4 text NOT NULL DEFAULT '',
  c5 double precision,
  c6 bytea,
  c7 integer NOT NULL DEFAULT 0,
  c8 text,
  c9 bytea,
  c10 integer,
  t4_id_0 integer,
  t13_id_1 integer,
  t10_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t4_id_0) REFERENCES t4(id) ON DELETE CASCADE,
  FOREIGN KEY (t13_id_1) REFERENCES t13(id) ON DELETE CASCADE,
  FOREIGN KEY (t10_id_2) REFERENCES t10(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t14_c1_idx ON t14(c1);
CREATE UNIQUE INDEX t14_c2_created_uq ON t14(c2, created_at);
CREATE TABLE t15 (
  id integer PRIMARY KEY,
  c0 text,
  c1 double precision,
  c2 integer NOT NULL DEFAULT 0,
  c3 text,
  c4 bytea,
  c5 integer NOT NULL DEFAULT 0,
  c6 bytea,
  c7 integer,
  t5_id_0 integer,
  t7_id_1 integer,
  t0_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t5_id_0) REFERENCES t5(id) ON DELETE CASCADE,
  FOREIGN KEY (t7_id_1) REFERENCES t7(id) ON DELETE CASCADE,
  FOREIGN KEY (t0_id_2) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t15_c1_idx ON t15(c1);
CREATE UNIQUE INDEX t15_c2_created_uq ON t15(c2, created_at);
CREATE TABLE t16 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 integer NOT NULL DEFAULT 0,
  c2 integer NOT NULL DEFAULT 0,
  c3 double precision,
  c4 text,
  c5 integer,
  t2_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t2_id_0) REFERENCES t2(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t16_c1_idx ON t16(c1);
CREATE UNIQUE INDEX t16_c2_created_uq ON t16(c2, created_at);
CREATE TABLE t17 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 numeric,
  c2 integer NOT NULL DEFAULT 0,
  c3 integer,
  c4 integer NOT NULL DEFAULT 0,
  c5 bytea,
  t4_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t4_id_0) REFERENCES t4(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t17_c1_idx ON t17(c1);
CREATE UNIQUE INDEX t17_c2_created_uq ON t17(c2, created_at);
CREATE TABLE t18 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 double precision,
  c3 bytea,
  c4 integer NOT NULL DEFAULT 0,
  c5 bytea,
  c6 numeric,
  c7 text,
  c8 bytea,
  c9 integer NOT NULL DEFAULT 0,
  c10 text NOT NULL DEFAULT '',
  c11 text NOT NULL DEFAULT '',
  t9_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t9_id_0) REFERENCES t9(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t18_c1_idx ON t18(c1);
CREATE UNIQUE INDEX t18_c2_created_uq ON t18(c2, created_at);
CREATE TABLE t19 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 text NOT NULL DEFAULT '',
  c2 double precision,
  c3 numeric,
  c4 bytea,
  c5 numeric,
  c6 integer,
  c7 text,
  c8 text,
  c9 integer,
  c10 double precision,
  t18_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t18_id_0) REFERENCES t18(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t19_c1_idx ON t19(c1);
CREATE UNIQUE INDEX t19_c2_created_uq ON t19(c2, created_at);
CREATE TABLE t20 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 integer,
  c3 integer,
  c4 text NOT NULL DEFAULT '',
  c5 text NOT NULL DEFAULT '',
  c6 integer,
  c7 text,
  t1_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t1_id_0) REFERENCES t1(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t20_c1_idx ON t20(c1);
CREATE UNIQUE INDEX t20_c2_created_uq ON t20(c2, created_at);
CREATE TABLE t21 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 bytea,
  c2 text,
  c3 double precision,
  c4 text NOT NULL DEFAULT '',
  c5 numeric,
  c6 text,
  c7 bytea,
  c8 text,
  c9 text NOT NULL DEFAULT '',
  t18_id_0 integer,
  t15_id_1 integer,
  t7_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t18_id_0) REFERENCES t18(id) ON DELETE CASCADE,
  FOREIGN KEY (t15_id_1) REFERENCES t15(id) ON DELETE CASCADE,
  FOREIGN KEY (t7_id_2) REFERENCES t7(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t21_c1_idx ON t21(c1);
CREATE UNIQUE INDEX t21_c2_created_uq ON t21(c2, created_at);
CREATE TABLE t22 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 numeric,
  c2 text,
  c3 integer,
  c4 integer,
  c5 text NOT NULL DEFAULT '',
  c6 numeric,
  c7 double precision,
  c8 numeric,
  c9 numeric,
  c10 numeric,
  c11 integer NOT NULL DEFAULT 0,
  t1_id_0 integer,
  t21_id_1 integer,
  t20_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t1_id_0) REFERENCES t1(id) ON DELETE CASCADE,
  FOREIGN KEY (t21_id_1) REFERENCES t21(id) ON DELETE CASCADE,
  FOREIGN KEY (t20_id_2) REFERENCES t20(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t22_c1_idx ON t22(c1);
CREATE UNIQUE INDEX t22_c2_created_uq ON t22(c2, created_at);
CREATE TABLE t23 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 numeric,
  c2 text NOT NULL DEFAULT '',
  c3 double precision,
  c4 integer NOT NULL DEFAULT 0,
  c5 integer NOT NULL DEFAULT 0,
  t7_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t7_id_0) REFERENCES t7(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t23_c1_idx ON t23(c1);
CREATE UNIQUE INDEX t23_c2_created_uq ON t23(c2, created_at);
CREATE TABLE t24 (
  id integer PRIMARY KEY,
  c0 text,
  c1 bytea,
  c2 numeric,
  c3 text,
  c4 numeric,
  c5 text,
  c6 double precision,
  c7 numeric,
  t2_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t2_id_0) REFERENCES t2(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t24_c1_idx ON t24(c1);
CREATE UNIQUE INDEX t24_c2_created_uq ON t24(c2, created_at);
CREATE TABLE t25 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 integer NOT NULL DEFAULT 0,
  c2 integer NOT NULL DEFAULT 0,
  c3 bytea,
  c4 integer,
  c5 integer,
  c6 text NOT NULL DEFAULT '',
  c7 bytea,
  c8 integer NOT NULL DEFAULT 0,
  c9 integer,
  c10 integer,
  c11 integer NOT NULL DEFAULT 0,
  t5_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t5_id_0) REFERENCES t5(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t25_c1_idx ON t25(c1);
CREATE UNIQUE INDEX t25_c2_created_uq ON t25(c2, created_at);
CREATE TABLE t26 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 numeric,
  c2 text,
  c3 integer NOT NULL DEFAULT 0,
  c4 numeric,
  c5 integer,
  c6 text,
  c7 numeric,
  c8 integer,
  c9 numeric,
  c10 double precision,
  t9_id_0 integer,
  t13_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t9_id_0) REFERENCES t9(id) ON DELETE CASCADE,
  FOREIGN KEY (t13_id_1) REFERENCES t13(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t26_c1_idx ON t26(c1);
CREATE UNIQUE INDEX t26_c2_created_uq ON t26(c2, created_at);
CREATE TABLE t27 (
  id integer PRIMARY KEY,
  c0 text,
  c1 text,
  c2 double precision,
  c3 text,
  c4 integer,
  c5 bytea,
  c6 text NOT NULL DEFAULT '',
  c7 bytea,
  c8 integer,
  c9 text NOT NULL DEFAULT '',
  c10 double precision,
  c11 integer,
  t18_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t18_id_0) REFERENCES t18(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t27_c1_idx ON t27(c1);
CREATE UNIQUE INDEX t27_c2_created_uq ON t27(c2, created_at);
CREATE TABLE t28 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 integer NOT NULL DEFAULT 0,
  c2 bytea,
  c3 text,
  c4 integer,
  c5 bytea,
  c6 integer,
  c7 integer NOT NULL DEFAULT 0,
  c8 text,
  c9 integer,
  c10 bytea,
  c11 integer,
  t27_id_0 integer,
  t7_id_1 integer,
  t12_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t27_id_0) REFERENCES t27(id) ON DELETE CASCADE,
  FOREIGN KEY (t7_id_1) REFERENCES t7(id) ON DELETE CASCADE,
  FOREIGN KEY (t12_id_2) REFERENCES t12(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t28_c1_idx ON t28(c1);
CREATE UNIQUE INDEX t28_c2_created_uq ON t28(c2, created_at);
CREATE TABLE t29 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 bytea,
  c3 bytea,
  c4 integer,
  c5 bytea,
  t13_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t13_id_0) REFERENCES t13(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t29_c1_idx ON t29(c1);
CREATE UNIQUE INDEX t29_c2_created_uq ON t29(c2, created_at);
CREATE TABLE t30 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 text,
  c2 text NOT NULL DEFAULT '',
  c3 text NOT NULL DEFAULT '',
  c4 double precision,
  c5 text,
  c6 double precision,
  c7 numeric,
  c8 text,
  c9 text NOT NULL DEFAULT '',
  t9_id_0 integer,
  t14_id_1 integer,
  t10_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t9_id_0) REFERENCES t9(id) ON DELETE CASCADE,
  FOREIGN KEY (t14_id_1) REFERENCES t14(id) ON DELETE CASCADE,
  FOREIGN KEY (t10_id_2) REFERENCES t10(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t30_c1_idx ON t30(c1);
CREATE UNIQUE INDEX t30_c2_created_uq ON t30(c2, created_at);
CREATE TABLE t31 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 numeric,
  c2 bytea,
  c3 bytea,
  c4 integer,
  c5 integer,
  t6_id_0 integer,
  t16_id_1 integer,
  t8_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t6_id_0) REFERENCES t6(id) ON DELETE CASCADE,
  FOREIGN KEY (t16_id_1) REFERENCES t16(id) ON DELETE CASCADE,
  FOREIGN KEY (t8_id_2) REFERENCES t8(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t31_c1_idx ON t31(c1);
CREATE UNIQUE INDEX t31_c2_created_uq ON t31(c2, created_at);
CREATE TABLE t32 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 integer,
  c2 text,
  c3 double precision,
  c4 double precision,
  c5 text,
  c6 numeric,
  t19_id_0 integer,
  t0_id_1 integer,
  t19_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t19_id_0) REFERENCES t19(id) ON DELETE CASCADE,
  FOREIGN KEY (t0_id_1) REFERENCES t0(id) ON DELETE CASCADE,
  FOREIGN KEY (t19_id_2) REFERENCES t19(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t32_c1_idx ON t32(c1);
CREATE UNIQUE INDEX t32_c2_created_uq ON t32(c2, created_at);
CREATE TABLE t33 (
  id integer PRIMARY KEY,
  c0 text,
  c1 double precision,
  c2 integer,
  c3 integer,
  c4 text NOT NULL DEFAULT '',
  c5 bytea,
  t17_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t17_id_0) REFERENCES t17(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t33_c1_idx ON t33(c1);
CREATE UNIQUE INDEX t33_c2_created_uq ON t33(c2, created_at);
CREATE TABLE t34 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 text NOT NULL DEFAULT '',
  c3 double precision,
  c4 text,
  c5 text NOT NULL DEFAULT '',
  c6 text NOT NULL DEFAULT '',
  c7 integer NOT NULL DEFAULT 0,
  c8 double precision,
  t31_id_0 integer,
  t16_id_1 integer,
  t3_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t31_id_0) REFERENCES t31(id) ON DELETE CASCADE,
  FOREIGN KEY (t16_id_1) REFERENCES t16(id) ON DELETE CASCADE,
  FOREIGN KEY (t3_id_2) REFERENCES t3(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t34_c1_idx ON t34(c1);
CREATE UNIQUE INDEX t34_c2_created_uq ON t34(c2, created_at);
CREATE TABLE t35 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 numeric,
  c2 integer NOT NULL DEFAULT 0,
  c3 double precision,
  c4 integer,
  c5 integer,
  t8_id_0 integer,
  t16_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t8_id_0) REFERENCES t8(id) ON DELETE CASCADE,
  FOREIGN KEY (t16_id_1) REFERENCES t16(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t35_c1_idx ON t35(c1);
CREATE UNIQUE INDEX t35_c2_created_uq ON t35(c2, created_at);
CREATE TABLE t36 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 numeric,
  c2 bytea,
  c3 text NOT NULL DEFAULT '',
  c4 numeric,
  c5 bytea,
  c6 integer,
  t4_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t4_id_0) REFERENCES t4(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t36_c1_idx ON t36(c1);
CREATE UNIQUE INDEX t36_c2_created_uq ON t36(c2, created_at);
CREATE TABLE t37 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 integer,
  c2 integer NOT NULL DEFAULT 0,
  c3 double precision,
  c4 bytea,
  c5 bytea,
  c6 text,
  t8_id_0 integer,
  t2_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t8_id_0) REFERENCES t8(id) ON DELETE CASCADE,
  FOREIGN KEY (t2_id_1) REFERENCES t2(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t37_c1_idx ON t37(c1);
CREATE UNIQUE INDEX t37_c2_created_uq ON t37(c2, created_at);
CREATE TABLE t38 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 integer NOT NULL DEFAULT 0,
  c2 integer NOT NULL DEFAULT 0,
  c3 integer,
  c4 double precision,
  c5 text,
  c6 text NOT NULL DEFAULT '',
  c7 text,
  c8 text NOT NULL DEFAULT '',
  t22_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t22_id_0) REFERENCES t22(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t38_c1_idx ON t38(c1);
CREATE UNIQUE INDEX t38_c2_created_uq ON t38(c2, created_at);
CREATE TABLE t39 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text NOT NULL DEFAULT '',
  c2 text,
  c3 text,
  c4 integer NOT NULL DEFAULT 0,
  c5 text,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer NOT NULL DEFAULT 0,
  c8 text,
  c9 numeric,
  c10 integer,
  t21_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t21_id_0) REFERENCES t21(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t39_c1_idx ON t39(c1);
CREATE UNIQUE INDEX t39_c2_created_uq ON t39(c2, created_at);
CREATE TABLE t40 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 text NOT NULL DEFAULT '',
  c2 integer NOT NULL DEFAULT 0,
  c3 text NOT NULL DEFAULT '',
  c4 integer NOT NULL DEFAULT 0,
  c5 text,
  c6 double precision,
  c7 text,
  c8 integer NOT NULL DEFAULT 0,
  c9 text NOT NULL DEFAULT '',
  c10 integer,
  t2_id_0 integer,
  t30_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t2_id_0) REFERENCES t2(id) ON DELETE CASCADE,
  FOREIGN KEY (t30_id_1) REFERENCES t30(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t40_c1_idx ON t40(c1);
CREATE UNIQUE INDEX t40_c2_created_uq ON t40(c2, created_at);
CREATE TABLE t41 (
  id integer PRIMARY KEY,
  c0 text,
  c1 integer NOT NULL DEFAULT 0,
  c2 numeric,
  c3 double precision,
  c4 double precision,
  c5 integer NOT NULL DEFAULT 0,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer NOT NULL DEFAULT 0,
  t14_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t14_id_0) REFERENCES t14(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t41_c1_idx ON t41(c1);
CREATE UNIQUE INDEX t41_c2_created_uq ON t41(c2, created_at);
CREATE TABLE t42 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 text,
  c2 numeric,
  c3 double precision,
  c4 double precision,
  t17_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t17_id_0) REFERENCES t17(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t42_c1_idx ON t42(c1);
CREATE UNIQUE INDEX t42_c2_created_uq ON t42(c2, created_at);
CREATE TABLE t43 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 bytea,
  c2 numeric,
  c3 text NOT NULL DEFAULT '',
  c4 integer NOT NULL DEFAULT 0,
  c5 bytea,
  c6 double precision,
  c7 integer,
  c8 integer,
  c9 double precision,
  t37_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t37_id_0) REFERENCES t37(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t43_c1_idx ON t43(c1);
CREATE UNIQUE INDEX t43_c2_created_uq ON t43(c2, created_at);
CREATE TABLE t44 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 integer,
  c2 bytea,
  c3 numeric,
  c4 double precision,
  c5 text NOT NULL DEFAULT '',
  c6 integer NOT NULL DEFAULT 0,
  c7 double precision,
  c8 numeric,
  t32_id_0 integer,
  t7_id_1 integer,
  t24_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t32_id_0) REFERENCES t32(id) ON DELETE CASCADE,
  FOREIGN KEY (t7_id_1) REFERENCES t7(id) ON DELETE CASCADE,
  FOREIGN KEY (t24_id_2) REFERENCES t24(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t44_c1_idx ON t44(c1);
CREATE UNIQUE INDEX t44_c2_created_uq ON t44(c2, created_at);
CREATE TABLE t45 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 integer,
  c2 text NOT NULL DEFAULT '',
  c3 numeric,
  c4 integer,
  c5 bytea,
  c6 integer NOT NULL DEFAULT 0,
  c7 bytea,
  t42_id_0 integer,
  t12_id_1 integer,
  t23_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t42_id_0) REFERENCES t42(id) ON DELETE CASCADE,
  FOREIGN KEY (t12_id_1) REFERENCES t12(id) ON DELETE CASCADE,
  FOREIGN KEY (t23_id_2) REFERENCES t23(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t45_c1_idx ON t45(c1);
CREATE UNIQUE INDEX t45_c2_created_uq ON t45(c2, created_at);
CREATE TABLE t46 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 text NOT NULL DEFAULT '',
  c2 double precision,
  c3 bytea,
  c4 double precision,
  c5 text NOT NULL DEFAULT '',
  c6 integer NOT NULL DEFAULT 0,
  c7 integer,
  c8 text NOT NULL DEFAULT '',
  c9 double precision,
  c10 bytea,
  t42_id_0 integer,
  t26_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t42_id_0) REFERENCES t42(id) ON DELETE CASCADE,
  FOREIGN KEY (t26_id_1) REFERENCES t26(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t46_c1_idx ON t46(c1);
CREATE UNIQUE INDEX t46_c2_created_uq ON t46(c2, created_at);
CREATE TABLE t47 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 text NOT NULL DEFAULT '',
  c2 double precision,
  c3 bytea,
  c4 text,
  c5 text,
  c6 numeric,
  c7 text NOT NULL DEFAULT '',
  c8 numeric,
  c9 text NOT NULL DEFAULT '',
  t11_id_0 integer,
  t39_id_1 integer,
  t36_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t11_id_0) REFERENCES t11(id) ON DELETE CASCADE,
  FOREIGN KEY (t39_id_1) REFERENCES t39(id) ON DELETE CASCADE,
  FOREIGN KEY (t36_id_2) REFERENCES t36(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t47_c1_idx ON t47(c1);
CREATE UNIQUE INDEX t47_c2_created_uq ON t47(c2, created_at);
CREATE TABLE t48 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 bytea,
  c2 integer NOT NULL DEFAULT 0,
  c3 integer,
  c4 double precision,
  c5 double precision,
  c6 text,
  c7 numeric,
  c8 integer NOT NULL DEFAULT 0,
  t38_id_0 integer,
  t41_id_1 integer,
  t20_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t38_id_0) REFERENCES t38(id) ON DELETE CASCADE,
  FOREIGN KEY (t41_id_1) REFERENCES t41(id) ON DELETE CASCADE,
  FOREIGN KEY (t20_id_2) REFERENCES t20(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t48_c1_idx ON t48(c1);
CREATE UNIQUE INDEX t48_c2_created_uq ON t48(c2, created_at);
CREATE TABLE t49 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 numeric,
  c2 text NOT NULL DEFAULT '',
  c3 text,
  c4 bytea,
  c5 numeric,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer NOT NULL DEFAULT 0,
  c8 text NOT NULL DEFAULT '',
  c9 text,
  c10 text NOT NULL DEFAULT '',
  c11 integer,
  t32_id_0 integer,
  t42_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t32_id_0) REFERENCES t32(id) ON DELETE CASCADE,
  FOREIGN KEY (t42_id_1) REFERENCES t42(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t49_c1_idx ON t49(c1);
CREATE UNIQUE INDEX t49_c2_created_uq ON t49(c2, created_at);
CREATE TABLE t50 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 integer NOT NULL DEFAULT 0,
  c2 integer NOT NULL DEFAULT 0,
  c3 text,
  c4 text NOT NULL DEFAULT '',
  c5 double precision,
  c6 text,
  c7 integer NOT NULL DEFAULT 0,
  c8 text,
  c9 text,
  t2_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t2_id_0) REFERENCES t2(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t50_c1_idx ON t50(c1);
CREATE UNIQUE INDEX t50_c2_created_uq ON t50(c2, created_at);
CREATE TABLE t51 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 bytea,
  c2 integer NOT NULL DEFAULT 0,
  c3 integer NOT NULL DEFAULT 0,
  c4 integer,
  c5 numeric,
  c6 numeric,
  c7 text NOT NULL DEFAULT '',
  t12_id_0 integer,
  t45_id_1 integer,
  t44_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t12_id_0) REFERENCES t12(id) ON DELETE CASCADE,
  FOREIGN KEY (t45_id_1) REFERENCES t45(id) ON DELETE CASCADE,
  FOREIGN KEY (t44_id_2) REFERENCES t44(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t51_c1_idx ON t51(c1);
CREATE UNIQUE INDEX t51_c2_created_uq ON t51(c2, created_at);
CREATE TABLE t52 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 numeric,
  c2 text,
  c3 text,
  c4 text NOT NULL DEFAULT '',
  c5 text NOT NULL DEFAULT '',
  c6 integer,
  c7 integer NOT NULL DEFAULT 0,
  c8 integer NOT NULL DEFAULT 0,
  c9 integer NOT NULL DEFAULT 0,
  c10 integer,
  t14_id_0 integer,
  t11_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t14_id_0) REFERENCES t14(id) ON DELETE CASCADE,
  FOREIGN KEY (t11_id_1) REFERENCES t11(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t52_c1_idx ON t52(c1);
CREATE UNIQUE INDEX t52_c2_created_uq ON t52(c2, created_at);
CREATE TABLE t53 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 bytea,
  c2 text,
  c3 integer NOT NULL DEFAULT 0,
  c4 integer,
  c5 numeric,
  c6 text,
  c7 integer NOT NULL DEFAULT 0,
  c8 numeric,
  c9 text NOT NULL DEFAULT '',
  c10 bytea,
  c11 bytea,
  t20_id_0 integer,
  t48_id_1 integer,
  t28_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t20_id_0) REFERENCES t20(id) ON DELETE CASCADE,
  FOREIGN KEY (t48_id_1) REFERENCES t48(id) ON DELETE CASCADE,
  FOREIGN KEY (t28_id_2) REFERENCES t28(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t53_c1_idx ON t53(c1);
CREATE UNIQUE INDEX t53_c2_created_uq ON t53(c2, created_at);
CREATE TABLE t54 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 bytea,
  c2 numeric,
  c3 text,
  c4 text NOT NULL DEFAULT '',
  c5 integer NOT NULL DEFAULT 0,
  c6 numeric,
  c7 numeric,
  c8 double precision,
  c9 integer NOT NULL DEFAULT 0,
  c10 text,
  t17_id_0 integer,
  t49_id_1 integer,
  t49_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t17_id_0) REFERENCES t17(id) ON DELETE CASCADE,
  FOREIGN KEY (t49_id_1) REFERENCES t49(id) ON DELETE CASCADE,
  FOREIGN KEY (t49_id_2) REFERENCES t49(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t54_c1_idx ON t54(c1);
CREATE UNIQUE INDEX t54_c2_created_uq ON t54(c2, created_at);
CREATE TABLE t55 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 text,
  c2 double precision,
  c3 numeric,
  c4 integer,
  c5 text NOT NULL DEFAULT '',
  c6 double precision,
  c7 text,
  c8 double precision,
  c9 double precision,
  c10 double precision,
  c11 bytea,
  t8_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t8_id_0) REFERENCES t8(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t55_c1_idx ON t55(c1);
CREATE UNIQUE INDEX t55_c2_created_uq ON t55(c2, created_at);
CREATE TABLE t56 (
  id integer PRIMARY KEY,
  c0 text,
  c1 numeric,
  c2 text NOT NULL DEFAULT '',
  c3 text,
  c4 text NOT NULL DEFAULT '',
  c5 text,
  c6 integer,
  t26_id_0 integer,
  t21_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t26_id_0) REFERENCES t26(id) ON DELETE CASCADE,
  FOREIGN KEY (t21_id_1) REFERENCES t21(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t56_c1_idx ON t56(c1);
CREATE UNIQUE INDEX t56_c2_created_uq ON t56(c2, created_at);
CREATE TABLE t57 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 integer,
  c2 text,
  c3 integer NOT NULL DEFAULT 0,
  c4 numeric,
  c5 numeric,
  c6 integer NOT NULL DEFAULT 0,
  c7 bytea,
  c8 text NOT NULL DEFAULT '',
  c9 integer,
  c10 integer NOT NULL DEFAULT 0,
  c11 integer NOT NULL DEFAULT 0,
  t24_id_0 integer,
  t30_id_1 integer,
  t0_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t24_id_0) REFERENCES t24(id) ON DELETE CASCADE,
  FOREIGN KEY (t30_id_1) REFERENCES t30(id) ON DELETE CASCADE,
  FOREIGN KEY (t0_id_2) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t57_c1_idx ON t57(c1);
CREATE UNIQUE INDEX t57_c2_created_uq ON t57(c2, created_at);
CREATE TABLE t58 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 integer NOT NULL DEFAULT 0,
  c2 numeric,
  c3 integer NOT NULL DEFAULT 0,
  c4 integer NOT NULL DEFAULT 0,
  c5 numeric,
  c6 bytea,
  c7 text NOT NULL DEFAULT '',
  c8 text NOT NULL DEFAULT '',
  c9 bytea,
  t57_id_0 integer,
  t14_id_1 integer,
  t31_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t57_id_0) REFERENCES t57(id) ON DELETE CASCADE,
  FOREIGN KEY (t14_id_1) REFERENCES t14(id) ON DELETE CASCADE,
  FOREIGN KEY (t31_id_2) REFERENCES t31(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t58_c1_idx ON t58(c1);
CREATE UNIQUE INDEX t58_c2_created_uq ON t58(c2, created_at);
CREATE TABLE t59 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 numeric,
  c2 numeric,
  c3 integer,
  c4 numeric,
  c5 double precision,
  c6 text NOT NULL DEFAULT '',
  c7 text NOT NULL DEFAULT '',
  t46_id_0 integer,
  t10_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t46_id_0) REFERENCES t46(id) ON DELETE CASCADE,
  FOREIGN KEY (t10_id_1) REFERENCES t10(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t59_c1_idx ON t59(c1);
CREATE UNIQUE INDEX t59_c2_created_uq ON t59(c2, created_at);
CREATE TABLE t60 (
  id integer PRIMARY KEY,
  c0 text,
  c1 bytea,
  c2 bytea,
  c3 integer,
  c4 numeric,
  c5 bytea,
  c6 bytea,
  c7 text NOT NULL DEFAULT '',
  c8 integer,
  c9 integer,
  c10 text NOT NULL DEFAULT '',
  c11 numeric,
  t55_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t55_id_0) REFERENCES t55(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t60_c1_idx ON t60(c1);
CREATE UNIQUE INDEX t60_c2_created_uq ON t60(c2, created_at);
CREATE TABLE t61 (
  id integer PRIMARY KEY,
  c0 text,
  c1 integer,
  c2 double precision,
  c3 numeric,
  c4 double precision,
  c5 text,
  c6 numeric,
  c7 double precision,
  c8 double precision,
  c9 integer NOT NULL DEFAULT 0,
  c10 numeric,
  c11 double precision,
  t16_id_0 integer,
  t53_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t16_id_0) REFERENCES t16(id) ON DELETE CASCADE,
  FOREIGN KEY (t53_id_1) REFERENCES t53(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t61_c1_idx ON t61(c1);
CREATE UNIQUE INDEX t61_c2_created_uq ON t61(c2, created_at);
CREATE TABLE t62 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 integer,
  c2 text NOT NULL DEFAULT '',
  c3 bytea,
  c4 integer,
  c5 double precision,
  t41_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t41_id_0) REFERENCES t41(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t62_c1_idx ON t62(c1);
CREATE UNIQUE INDEX t62_c2_created_uq ON t62(c2, created_at);
CREATE TABLE t63 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 text NOT NULL DEFAULT '',
  c2 integer,
  c3 integer NOT NULL DEFAULT 0,
  c4 integer,
  c5 text,
  t53_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t53_id_0) REFERENCES t53(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t63_c1_idx ON t63(c1);
CREATE UNIQUE INDEX t63_c2_created_uq ON t63(c2, created_at);
CREATE TABLE t64 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 text,
  c3 text,
  c4 numeric,
  t14_id_0 integer,
  t27_id_1 integer,
  t59_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t14_id_0) REFERENCES t14(id) ON DELETE CASCADE,
  FOREIGN KEY (t27_id_1) REFERENCES t27(id) ON DELETE CASCADE,
  FOREIGN KEY (t59_id_2) REFERENCES t59(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t64_c1_idx ON t64(c1);
CREATE UNIQUE INDEX t64_c2_created_uq ON t64(c2, created_at);
CREATE TABLE t65 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 double precision,
  c2 text,
  c3 bytea,
  c4 bytea,
  c5 text NOT NULL DEFAULT '',
  c6 text NOT NULL DEFAULT '',
  c7 integer,
  c8 integer NOT NULL DEFAULT 0,
  t39_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t39_id_0) REFERENCES t39(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t65_c1_idx ON t65(c1);
CREATE UNIQUE INDEX t65_c2_created_uq ON t65(c2, created_at);
CREATE TABLE t66 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 integer,
  c2 double precision,
  c3 bytea,
  c4 text NOT NULL DEFAULT '',
  c5 numeric,
  t25_id_0 integer,
  t9_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t25_id_0) REFERENCES t25(id) ON DELETE CASCADE,
  FOREIGN KEY (t9_id_1) REFERENCES t9(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t66_c1_idx ON t66(c1);
CREATE UNIQUE INDEX t66_c2_created_uq ON t66(c2, created_at);
CREATE TABLE t67 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 text NOT NULL DEFAULT '',
  c2 integer NOT NULL DEFAULT 0,
  c3 double precision,
  c4 integer NOT NULL DEFAULT 0,
  c5 text NOT NULL DEFAULT '',
  c6 bytea,
  c7 integer NOT NULL DEFAULT 0,
  t5_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t5_id_0) REFERENCES t5(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t67_c1_idx ON t67(c1);
CREATE UNIQUE INDEX t67_c2_created_uq ON t67(c2, created_at);
CREATE TABLE t68 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 numeric,
  c2 text NOT NULL DEFAULT '',
  c3 double precision,
  c4 integer,
  c5 bytea,
  c6 text NOT NULL DEFAULT '',
  c7 double precision,
  c8 integer,
  c9 integer NOT NULL DEFAULT 0,
  t62_id_0 integer,
  t13_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t62_id_0) REFERENCES t62(id) ON DELETE CASCADE,
  FOREIGN KEY (t13_id_1) REFERENCES t13(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t68_c1_idx ON t68(c1);
CREATE UNIQUE INDEX t68_c2_created_uq ON t68(c2, created_at);
CREATE TABLE t69 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 text NOT NULL DEFAULT '',
  c2 integer NOT NULL DEFAULT 0,
  c3 numeric,
  c4 text NOT NULL DEFAULT '',
  c5 text,
  c6 numeric,
  c7 text,
  c8 text NOT NULL DEFAULT '',
  c9 bytea,
  c10 text NOT NULL DEFAULT '',
  t68_id_0 integer,
  t61_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t68_id_0) REFERENCES t68(id) ON DELETE CASCADE,
  FOREIGN KEY (t61_id_1) REFERENCES t61(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t69_c1_idx ON t69(c1);
CREATE UNIQUE INDEX t69_c2_created_uq ON t69(c2, created_at);
CREATE TABLE t70 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 integer NOT NULL DEFAULT 0,
  c2 text NOT NULL DEFAULT '',
  c3 bytea,
  c4 double precision,
  c5 double precision,
  c6 integer NOT NULL DEFAULT 0,
  c7 text,
  c8 integer NOT NULL DEFAULT 0,
  c9 integer,
  c10 double precision,
  c11 numeric,
  t59_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t59_id_0) REFERENCES t59(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t70_c1_idx ON t70(c1);
CREATE UNIQUE INDEX t70_c2_created_uq ON t70(c2, created_at);
CREATE TABLE t71 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 integer,
  c2 numeric,
  c3 integer NOT NULL DEFAULT 0,
  c4 double precision,
  c5 text,
  c6 numeric,
  c7 text,
  c8 double precision,
  c9 integer NOT NULL DEFAULT 0,
  c10 double precision,
  t35_id_0 integer,
  t35_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t35_id_0) REFERENCES t35(id) ON DELETE CASCADE,
  FOREIGN KEY (t35_id_1) REFERENCES t35(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t71_c1_idx ON t71(c1);
CREATE UNIQUE INDEX t71_c2_created_uq ON t71(c2, created_at);
CREATE TABLE t72 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 integer,
  c3 text,
  c4 text NOT NULL DEFAULT '',
  t62_id_0 integer,
  t71_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t62_id_0) REFERENCES t62(id) ON DELETE CASCADE,
  FOREIGN KEY (t71_id_1) REFERENCES t71(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t72_c1_idx ON t72(c1);
CREATE UNIQUE INDEX t72_c2_created_uq ON t72(c2, created_at);
CREATE TABLE t73 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 numeric,
  c2 text NOT NULL DEFAULT '',
  c3 text NOT NULL DEFAULT '',
  c4 numeric,
  c5 numeric,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer,
  t37_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t37_id_0) REFERENCES t37(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t73_c1_idx ON t73(c1);
CREATE UNIQUE INDEX t73_c2_created_uq ON t73(c2, created_at);
CREATE TABLE t74 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 text NOT NULL DEFAULT '',
  c2 text,
  c3 double precision,
  c4 text NOT NULL DEFAULT '',
  c5 bytea,
  c6 double precision,
  c7 numeric,
  t67_id_0 integer,
  t44_id_1 integer,
  t54_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t67_id_0) REFERENCES t67(id) ON DELETE CASCADE,
  FOREIGN KEY (t44_id_1) REFERENCES t44(id) ON DELETE CASCADE,
  FOREIGN KEY (t54_id_2) REFERENCES t54(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t74_c1_idx ON t74(c1);
CREATE UNIQUE INDEX t74_c2_created_uq ON t74(c2, created_at);
CREATE TABLE t75 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 text NOT NULL DEFAULT '',
  c2 numeric,
  c3 double precision,
  c4 double precision,
  c5 double precision,
  c6 text,
  c7 integer,
  c8 text NOT NULL DEFAULT '',
  c9 text,
  t15_id_0 integer,
  t68_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t15_id_0) REFERENCES t15(id) ON DELETE CASCADE,
  FOREIGN KEY (t68_id_1) REFERENCES t68(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t75_c1_idx ON t75(c1);
CREATE UNIQUE INDEX t75_c2_created_uq ON t75(c2, created_at);
CREATE TABLE t76 (
  id integer PRIMARY KEY,
  c0 text,
  c1 text,
  c2 text NOT NULL DEFAULT '',
  c3 numeric,
  c4 double precision,
  c5 text NOT NULL DEFAULT '',
  c6 bytea,
  t36_id_0 integer,
  t12_id_1 integer,
  t24_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t36_id_0) REFERENCES t36(id) ON DELETE CASCADE,
  FOREIGN KEY (t12_id_1) REFERENCES t12(id) ON DELETE CASCADE,
  FOREIGN KEY (t24_id_2) REFERENCES t24(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t76_c1_idx ON t76(c1);
CREATE UNIQUE INDEX t76_c2_created_uq ON t76(c2, created_at);
CREATE TABLE t77 (
  id integer PRIMARY KEY,
  c0 text,
  c1 double precision,
  c2 text,
  c3 double precision,
  c4 integer,
  c5 text NOT NULL DEFAULT '',
  c6 bytea,
  c7 text,
  c8 double precision,
  t6_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t6_id_0) REFERENCES t6(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t77_c1_idx ON t77(c1);
CREATE UNIQUE INDEX t77_c2_created_uq ON t77(c2, created_at);
CREATE TABLE t78 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 text,
  c2 text NOT NULL DEFAULT '',
  c3 integer NOT NULL DEFAULT 0,
  c4 integer NOT NULL DEFAULT 0,
  c5 numeric,
  c6 integer,
  c7 integer NOT NULL DEFAULT 0,
  c8 integer,
  t36_id_0 integer,
  t60_id_1 integer,
  t61_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t36_id_0) REFERENCES t36(id) ON DELETE CASCADE,
  FOREIGN KEY (t60_id_1) REFERENCES t60(id) ON DELETE CASCADE,
  FOREIGN KEY (t61_id_2) REFERENCES t61(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t78_c1_idx ON t78(c1);
CREATE UNIQUE INDEX t78_c2_created_uq ON t78(c2, created_at);
CREATE TABLE t79 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 text,
  c2 integer,
  c3 double precision,
  c4 integer NOT NULL DEFAULT 0,
  c5 numeric,
  c6 integer,
  c7 integer NOT NULL DEFAULT 0,
  c8 integer,
  c9 numeric,
  c10 numeric,
  c11 integer,
  t6_id_0 integer,
  t19_id_1 integer,
  t19_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t6_id_0) REFERENCES t6(id) ON DELETE CASCADE,
  FOREIGN KEY (t19_id_1) REFERENCES t19(id) ON DELETE CASCADE,
  FOREIGN KEY (t19_id_2) REFERENCES t19(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t79_c1_idx ON t79(c1);
CREATE UNIQUE INDEX t79_c2_created_uq ON t79(c2, created_at);
CREATE TABLE t80 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 text,
  c2 integer,
  c3 bytea,
  c4 integer NOT NULL DEFAULT 0,
  c5 numeric,
  c6 bytea,
  c7 bytea,
  c8 integer NOT NULL DEFAULT 0,
  t28_id_0 integer,
  t66_id_1 integer,
  t48_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t28_id_0) REFERENCES t28(id) ON DELETE CASCADE,
  FOREIGN KEY (t66_id_1) REFERENCES t66(id) ON DELETE CASCADE,
  FOREIGN KEY (t48_id_2) REFERENCES t48(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t80_c1_idx ON t80(c1);
CREATE UNIQUE INDEX t80_c2_created_uq ON t80(c2, created_at);
CREATE TABLE t81 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 double precision,
  c2 integer NOT NULL DEFAULT 0,
  c3 bytea,
  c4 numeric,
  c5 double precision,
  c6 bytea,
  c7 bytea,
  c8 integer,
  c9 bytea,
  c10 text NOT NULL DEFAULT '',
  c11 integer,
  t80_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t80_id_0) REFERENCES t80(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t81_c1_idx ON t81(c1);
CREATE UNIQUE INDEX t81_c2_created_uq ON t81(c2, created_at);
CREATE TABLE t82 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 text NOT NULL DEFAULT '',
  c2 integer,
  c3 text,
  c4 text,
  c5 text,
  c6 bytea,
  c7 integer,
  t0_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t0_id_0) REFERENCES t0(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t82_c1_idx ON t82(c1);
CREATE UNIQUE INDEX t82_c2_created_uq ON t82(c2, created_at);
CREATE TABLE t83 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 text NOT NULL DEFAULT '',
  c2 bytea,
  c3 numeric,
  c4 double precision,
  c5 integer,
  c6 text,
  c7 double precision,
  c8 text NOT NULL DEFAULT '',
  c9 double precision,
  c10 text NOT NULL DEFAULT '',
  t9_id_0 integer,
  t29_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t9_id_0) REFERENCES t9(id) ON DELETE CASCADE,
  FOREIGN KEY (t29_id_1) REFERENCES t29(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t83_c1_idx ON t83(c1);
CREATE UNIQUE INDEX t83_c2_created_uq ON t83(c2, created_at);
CREATE TABLE t84 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 integer NOT NULL DEFAULT 0,
  c2 text NOT NULL DEFAULT '',
  c3 bytea,
  c4 text NOT NULL DEFAULT '',
  c5 integer NOT NULL DEFAULT 0,
  c6 text,
  c7 numeric,
  c8 integer,
  t28_id_0 integer,
  t82_id_1 integer,
  t19_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t28_id_0) REFERENCES t28(id) ON DELETE CASCADE,
  FOREIGN KEY (t82_id_1) REFERENCES t82(id) ON DELETE CASCADE,
  FOREIGN KEY (t19_id_2) REFERENCES t19(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t84_c1_idx ON t84(c1);
CREATE UNIQUE INDEX t84_c2_created_uq ON t84(c2, created_at);
CREATE TABLE t85 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 text,
  c2 integer,
  c3 integer,
  c4 text,
  c5 integer NOT NULL DEFAULT 0,
  c6 double precision,
  c7 bytea,
  c8 text NOT NULL DEFAULT '',
  t36_id_0 integer,
  t56_id_1 integer,
  t15_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t36_id_0) REFERENCES t36(id) ON DELETE CASCADE,
  FOREIGN KEY (t56_id_1) REFERENCES t56(id) ON DELETE CASCADE,
  FOREIGN KEY (t15_id_2) REFERENCES t15(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t85_c1_idx ON t85(c1);
CREATE UNIQUE INDEX t85_c2_created_uq ON t85(c2, created_at);
CREATE TABLE t86 (
  id integer PRIMARY KEY,
  c0 text NOT NULL DEFAULT '',
  c1 double precision,
  c2 text NOT NULL DEFAULT '',
  c3 numeric,
  c4 double precision,
  c5 bytea,
  c6 bytea,
  c7 numeric,
  c8 numeric,
  c9 integer,
  c10 bytea,
  c11 integer,
  t41_id_0 integer,
  t77_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t41_id_0) REFERENCES t41(id) ON DELETE CASCADE,
  FOREIGN KEY (t77_id_1) REFERENCES t77(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t86_c1_idx ON t86(c1);
CREATE UNIQUE INDEX t86_c2_created_uq ON t86(c2, created_at);
CREATE TABLE t87 (
  id integer PRIMARY KEY,
  c0 integer,
  c1 integer,
  c2 text,
  c3 text NOT NULL DEFAULT '',
  c4 integer NOT NULL DEFAULT 0,
  c5 integer NOT NULL DEFAULT 0,
  c6 bytea,
  c7 bytea,
  c8 integer,
  t34_id_0 integer,
  t73_id_1 integer,
  t5_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t34_id_0) REFERENCES t34(id) ON DELETE CASCADE,
  FOREIGN KEY (t73_id_1) REFERENCES t73(id) ON DELETE CASCADE,
  FOREIGN KEY (t5_id_2) REFERENCES t5(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t87_c1_idx ON t87(c1);
CREATE UNIQUE INDEX t87_c2_created_uq ON t87(c2, created_at);
CREATE TABLE t88 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 bytea,
  c2 text NOT NULL DEFAULT '',
  c3 numeric,
  c4 double precision,
  c5 text,
  c6 bytea,
  t81_id_0 integer,
  t62_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t81_id_0) REFERENCES t81(id) ON DELETE CASCADE,
  FOREIGN KEY (t62_id_1) REFERENCES t62(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t88_c1_idx ON t88(c1);
CREATE UNIQUE INDEX t88_c2_created_uq ON t88(c2, created_at);
CREATE TABLE t89 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 double precision,
  c2 numeric,
  c3 double precision,
  c4 double precision,
  c5 text NOT NULL DEFAULT '',
  t20_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t20_id_0) REFERENCES t20(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t89_c1_idx ON t89(c1);
CREATE UNIQUE INDEX t89_c2_created_uq ON t89(c2, created_at);
CREATE TABLE t90 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 text NOT NULL DEFAULT '',
  c2 numeric,
  c3 double precision,
  c4 text NOT NULL DEFAULT '',
  c5 numeric,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer NOT NULL DEFAULT 0,
  c8 bytea,
  c9 integer,
  t11_id_0 integer,
  t40_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t11_id_0) REFERENCES t11(id) ON DELETE CASCADE,
  FOREIGN KEY (t40_id_1) REFERENCES t40(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t90_c1_idx ON t90(c1);
CREATE UNIQUE INDEX t90_c2_created_uq ON t90(c2, created_at);
CREATE TABLE t91 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 integer,
  c2 integer NOT NULL DEFAULT 0,
  c3 numeric,
  c4 integer NOT NULL DEFAULT 0,
  c5 bytea,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer,
  c8 text NOT NULL DEFAULT '',
  t59_id_0 integer,
  t52_id_1 integer,
  t6_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t59_id_0) REFERENCES t59(id) ON DELETE CASCADE,
  FOREIGN KEY (t52_id_1) REFERENCES t52(id) ON DELETE CASCADE,
  FOREIGN KEY (t6_id_2) REFERENCES t6(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t91_c1_idx ON t91(c1);
CREATE UNIQUE INDEX t91_c2_created_uq ON t91(c2, created_at);
CREATE TABLE t92 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 double precision,
  c2 bytea,
  c3 integer NOT NULL DEFAULT 0,
  c4 numeric,
  c5 text NOT NULL DEFAULT '',
  c6 numeric,
  c7 integer NOT NULL DEFAULT 0,
  t26_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t26_id_0) REFERENCES t26(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t92_c1_idx ON t92(c1);
CREATE UNIQUE INDEX t92_c2_created_uq ON t92(c2, created_at);
CREATE TABLE t93 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 double precision,
  c3 numeric,
  c4 text NOT NULL DEFAULT '',
  c5 numeric,
  c6 integer,
  c7 integer,
  c8 text NOT NULL DEFAULT '',
  t30_id_0 integer,
  t90_id_1 integer,
  t20_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t30_id_0) REFERENCES t30(id) ON DELETE CASCADE,
  FOREIGN KEY (t90_id_1) REFERENCES t90(id) ON DELETE CASCADE,
  FOREIGN KEY (t20_id_2) REFERENCES t20(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t93_c1_idx ON t93(c1);
CREATE UNIQUE INDEX t93_c2_created_uq ON t93(c2, created_at);
CREATE TABLE t94 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 integer,
  c2 bytea,
  c3 numeric,
  c4 integer,
  c5 text,
  c6 integer NOT NULL DEFAULT 0,
  c7 integer,
  c8 numeric,
  t82_id_0 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t82_id_0) REFERENCES t82(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t94_c1_idx ON t94(c1);
CREATE UNIQUE INDEX t94_c2_created_uq ON t94(c2, created_at);
CREATE TABLE t95 (
  id integer PRIMARY KEY,
  c0 numeric,
  c1 text NOT NULL DEFAULT '',
  c2 double precision,
  c3 bytea,
  c4 text NOT NULL DEFAULT '',
  c5 double precision,
  c6 numeric,
  t60_id_0 integer,
  t31_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t60_id_0) REFERENCES t60(id) ON DELETE CASCADE,
  FOREIGN KEY (t31_id_1) REFERENCES t31(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t95_c1_idx ON t95(c1);
CREATE UNIQUE INDEX t95_c2_created_uq ON t95(c2, created_at);
CREATE TABLE t96 (
  id integer PRIMARY KEY,
  c0 bytea,
  c1 text,
  c2 numeric,
  c3 text,
  c4 bytea,
  c5 bytea,
  c6 text NOT NULL DEFAULT '',
  c7 text,
  c8 integer NOT NULL DEFAULT 0,
  c9 integer,
  c10 double precision,
  c11 integer NOT NULL DEFAULT 0,
  t43_id_0 integer,
  t64_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t43_id_0) REFERENCES t43(id) ON DELETE CASCADE,
  FOREIGN KEY (t64_id_1) REFERENCES t64(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t96_c1_idx ON t96(c1);
CREATE UNIQUE INDEX t96_c2_created_uq ON t96(c2, created_at);
CREATE TABLE t97 (
  id integer PRIMARY KEY,
  c0 integer NOT NULL DEFAULT 0,
  c1 integer,
  c2 double precision,
  c3 text NOT NULL DEFAULT '',
  c4 double precision,
  c5 integer NOT NULL DEFAULT 0,
  c6 bytea,
  c7 bytea,
  c8 text NOT NULL DEFAULT '',
  t19_id_0 integer,
  t57_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t19_id_0) REFERENCES t19(id) ON DELETE CASCADE,
  FOREIGN KEY (t57_id_1) REFERENCES t57(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t97_c1_idx ON t97(c1);
CREATE UNIQUE INDEX t97_c2_created_uq ON t97(c2, created_at);
CREATE TABLE t98 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 double precision,
  c2 bytea,
  c3 integer NOT NULL DEFAULT 0,
  c4 bytea,
  c5 numeric,
  c6 numeric,
  c7 double precision,
  c8 integer NOT NULL DEFAULT 0,
  c9 text,
  c10 text NOT NULL DEFAULT '',
  c11 text,
  t49_id_0 integer,
  t29_id_1 integer,
  t52_id_2 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t49_id_0) REFERENCES t49(id) ON DELETE CASCADE,
  FOREIGN KEY (t29_id_1) REFERENCES t29(id) ON DELETE CASCADE,
  FOREIGN KEY (t52_id_2) REFERENCES t52(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t98_c1_idx ON t98(c1);
CREATE UNIQUE INDEX t98_c2_created_uq ON t98(c2, created_at);
CREATE TABLE t99 (
  id integer PRIMARY KEY,
  c0 double precision,
  c1 text NOT NULL DEFAULT '',
  c2 numeric,
  c3 text NOT NULL DEFAULT '',
  c4 integer NOT NULL DEFAULT 0,
  t49_id_0 integer,
  t84_id_1 integer,
  created_at timestamptz NOT NULL DEFAULT now(),
  FOREIGN KEY (t49_id_0) REFERENCES t49(id) ON DELETE CASCADE,
  FOREIGN KEY (t84_id_1) REFERENCES t84(id) ON DELETE CASCADE,
  CHECK (c0 IS NULL OR length(c0::text) < 1000)
);
CREATE INDEX t99_c1_idx ON t99(c1);
CREATE UNIQUE INDEX t99_c2_created_uq ON t99(c2, created_at);
CREATE TRIGGER trg0 BEFORE INSERT ON t0 FOR EACH ROW EXECUTE FUNCTION noop();
CREATE TRIGGER trg1 BEFORE INSERT ON t1 FOR EACH ROW EXECUTE FUNCTION noop();
CREATE TRIGGER trg2 BEFORE INSERT ON t2 FOR EACH ROW EXECUTE FUNCTION noop();
CREATE TRIGGER trg3 BEFORE INSERT ON t3 FOR EACH ROW EXECUTE FUNCTION noop();
CREATE TRIGGER trg4 BEFORE INSERT ON t4 FOR EACH ROW EXECUTE FUNCTION noop();
CREATE VIEW v0 AS SELECT a.id, a.c1, b.id AS bid FROM t10 a JOIN t11 b ON b.id = a.id;
CREATE VIEW v1 AS SELECT a.id, a.c1, b.id AS bid FROM t11 a JOIN t12 b ON b.id = a.id;
CREATE VIEW v2 AS SELECT a.id, a.c1, b.id AS bid FROM t12 a JOIN t13 b ON b.id = a.id;
CREATE VIEW v3 AS SELECT a.id, a.c1, b.id AS bid FROM t13 a JOIN t14 b ON b.id = a.id;
CREATE VIEW v4 AS SELECT a.id, a.c1, b.id AS bid FROM t14 a JOIN t15 b ON b.id = a.id;
