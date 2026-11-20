//! Against a real throwaway Postgres: every kind the reader claims, read from a schema built to exercise each one.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use migate_core::engine::Sandbox;
use migate_core::schema::{Detail, IndexPart, Kind, Object, Snapshot, TypeFamily};
use migate_engine_postgres::Server;

/// A run folder removed when the test ends (after the server, which is declared later and dropped first).
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "migate-pg-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn psql(server: &Server, sql: &str) {
    let out = Command::new("psql")
        .args([
            "-X",
            "-q",
            "-v",
            "ON_ERROR_STOP=1",
            "-d",
            server.url(),
            "-c",
            sql,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

const SCHEMA: &str = r#"
create schema shop;
create type shop.mood as enum ('sad', 'ok', 'happy');
create sequence shop.invoice_no;
create table shop.users (
  id bigint generated always as identity primary key,
  email varchar(320) not null unique,
  name text collate "C",
  price numeric(10, 2) check (price > 0),
  mood shop.mood default 'ok',
  slug text generated always as (lower(name)) stored
);
create table shop.orders (
  id serial primary key,
  user_id bigint references shop.users (id) on delete cascade deferrable initially deferred,
  total numeric,
  note text
);
alter table shop.orders add constraint orders_total_check check (total >= 0) not valid;
alter table shop.orders add constraint orders_user_strict foreign key (user_id) references shop.users (id) not valid;
create index orders_note_lower on shop.orders (lower(note) desc, id) where note is not null;
create view shop.big_orders as select id, total from shop.orders where total > 100;
create materialized view shop.order_totals as select user_id, sum(total) as s from shop.orders group by user_id;
create function shop.touch() returns trigger language plpgsql as $$ begin new.note := coalesce(new.note, ''); return new; end $$;
create function shop.add(a int, b int) returns int language sql as 'select a + b';
create function shop.add(a int, b int, c int) returns int language sql as 'select a + b + c';
create trigger orders_touch before insert or update on shop.orders for each row execute function shop.touch();
"#;

fn get<'a>(s: &'a Snapshot, kind: Kind, parent: Option<&str>, name: &str) -> &'a Object {
    s.get(kind, "shop", parent, name)
        .unwrap_or_else(|| panic!("missing {kind:?} {parent:?}.{name}"))
}

fn col(s: &Snapshot, table: &str, name: &str) -> migate_core::schema::Column {
    match &get(s, Kind::Column, Some(table), name).detail {
        Detail::Column(c) => c.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_kind_is_read_from_a_real_server() {
    let dir = Dir::new();
    let server = Server::start(&dir.0).unwrap();
    psql(&server, SCHEMA);
    let s = server.snapshot().unwrap();
    assert_eq!(s.engine(), "postgres");

    let id = col(&s, "users", "id");
    assert!(id.identity && !id.nullable);
    let email = col(&s, "users", "email");
    assert_eq!(
        (email.data_type.family, email.data_type.length),
        (TypeFamily::Text, Some(320))
    );
    assert_eq!(col(&s, "users", "name").collation.as_deref(), Some("C"));
    let price = col(&s, "users", "price");
    assert_eq!(
        (price.data_type.precision, price.data_type.scale),
        (Some(10), Some(2))
    );
    let mood = col(&s, "users", "mood");
    assert_eq!(mood.data_type.family, TypeFamily::Enum);
    assert_eq!(mood.default.as_deref(), Some("'ok'::shop.mood"));
    let slug = col(&s, "users", "slug");
    let generated = slug.generated.unwrap();
    assert!(
        generated.stored && generated.expression.contains("lower"),
        "{generated:?}"
    );
    assert!(
        col(&s, "orders", "id").identity,
        "serial counts as identity"
    );
    assert_eq!(col(&s, "orders", "note").ordinal, 3);

    assert!(
        matches!(&get(&s, Kind::PrimaryKey, Some("users"), "users_pkey").detail,
        Detail::PrimaryKey { columns } if columns == &["id"])
    );
    assert!(
        matches!(&get(&s, Kind::Unique, Some("users"), "users_email_key").detail,
        Detail::Unique { columns } if columns == &["email"])
    );
    match &get(&s, Kind::Check, Some("orders"), "orders_total_check").detail {
        Detail::Check {
            expression,
            validated,
        } => {
            assert_eq!(
                (expression.as_str(), *validated),
                ("total >= 0::numeric", false)
            );
        }
        other => panic!("{other:?}"),
    }
    match &get(&s, Kind::ForeignKey, Some("orders"), "orders_user_id_fkey").detail {
        Detail::ForeignKey(fk) => {
            assert_eq!(
                (
                    fk.ref_namespace.as_str(),
                    fk.ref_table.as_str(),
                    fk.ref_columns.as_slice(),
                    fk.on_delete.as_str()
                ),
                ("shop", "users", &["id".to_string()][..], "CASCADE")
            );
            assert!(fk.deferrable && fk.initially_deferred && fk.validated);
        }
        other => panic!("{other:?}"),
    }
    match &get(&s, Kind::ForeignKey, Some("orders"), "orders_user_strict").detail {
        Detail::ForeignKey(fk) => assert!(!fk.validated && !fk.deferrable),
        other => panic!("{other:?}"),
    }
    match &get(&s, Kind::Index, Some("orders"), "orders_note_lower").detail {
        Detail::Index(i) => {
            assert!(!i.unique);
            assert_eq!(i.method.as_deref(), Some("btree"));
            assert_eq!(i.predicate.as_deref(), Some("note IS NOT NULL"));
            assert_eq!(
                i.parts,
                vec![
                    IndexPart::Expression {
                        sql: "lower(note)".into(),
                        descending: true
                    },
                    IndexPart::Column {
                        name: "id".into(),
                        descending: false
                    },
                ]
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(
        s.get(Kind::Index, "shop", Some("users"), "users_email_key")
            .is_none(),
        "constraint indexes are the constraint"
    );
    assert!(matches!(
        &get(&s, Kind::View, None, "big_orders").detail,
        Detail::View {
            materialized: false,
            ..
        }
    ));
    assert!(matches!(
        &get(&s, Kind::View, None, "order_totals").detail,
        Detail::View {
            materialized: true,
            ..
        }
    ));
    match &get(&s, Kind::Trigger, Some("orders"), "orders_touch").detail {
        Detail::Trigger { timing, events, .. } => {
            assert_eq!(
                (timing.as_str(), events.as_slice()),
                ("BEFORE", &["INSERT".to_string(), "UPDATE".to_string()][..])
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(
        s.get(Kind::Routine, "shop", None, "add(a integer, b integer)")
            .is_some()
    );
    assert!(
        s.get(
            Kind::Routine,
            "shop",
            None,
            "add(a integer, b integer, c integer)"
        )
        .is_some()
    );
    assert!(matches!(&get(&s, Kind::Enum, None, "mood").detail,
        Detail::Enum { values } if values == &["sad", "ok", "happy"]));
    assert!(matches!(
        &get(&s, Kind::Sequence, None, "invoice_no").detail,
        Detail::Sequence { owned_by: None, .. }
    ));
    assert!(
        matches!(&get(&s, Kind::Sequence, None, "orders_id_seq").detail,
        Detail::Sequence { owned_by: Some(o), .. } if o == "orders.id")
    );
    assert!(
        s.get(Kind::Sequence, "shop", None, "users_id_seq")
            .is_none(),
        "an identity's own sequence is the column's"
    );
}

#[test]
fn the_database_port_counts_and_empties_a_real_server() {
    let dir = Dir::new();
    let server = Server::start(&dir.0).unwrap();
    psql(
        &server,
        "create table history (name text); insert into history values ('0001'), ('0002');
                   create table users (id int); insert into users values (1), (2), (3);
                   create view v as select id from users;",
    );
    let db = server.open("select name from history").unwrap();
    assert_eq!(db.count_rows("select name from history").unwrap(), 2);
    let counts = db.row_counts().unwrap();
    assert_eq!(
        (counts.get("public.users"), counts.get("public.history")),
        (Some(&3), Some(&2))
    );
    assert!(db.count_rows("select * from missing").is_err());
    db.drop_all_but("select name from history").unwrap();
    let left = server.snapshot().unwrap();
    let tables: Vec<&str> = left
        .objects()
        .iter()
        .filter(|o| o.kind() == Kind::Table)
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(tables, ["history"]);
    assert!(left.objects().iter().all(|o| o.kind() != Kind::View));
    assert_eq!(server.placeholders()[0].0, "url");
}

#[test]
fn a_dropped_server_is_gone() {
    let dir = Dir::new();
    let server = Server::start(&dir.0).unwrap();
    let pid = server.process_group().unwrap();
    drop(server);
    let alive = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .unwrap()
        .success();
    assert!(!alive, "postgres {pid} still running");
}
