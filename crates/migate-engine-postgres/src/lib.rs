//! migate's PostgreSQL engine, behind the engine port: a throwaway local server per run, and a reader that turns the
//! catalog into the canonical [`Snapshot`].
//!
//! The server is the machine's own Postgres (`initdb`, then `postgres` started as a child in its own process group, so
//! an interrupted run can stop it). The reader talks to it through `psql`, which ships with those programs, asking for
//! JSON: one catalog query per object kind (`docs/research/architecture.md` section 4).

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use migate_core::engine;
use migate_core::schema::{
    Column, DataType, Detail, ForeignKey, Generated, Index, IndexPart, Kind, Object, Snapshot,
    TypeFamily,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Postgres has every kind the model knows.
pub const CAPABILITIES: [Kind; 13] = [
    Kind::Namespace,
    Kind::Table,
    Kind::Column,
    Kind::PrimaryKey,
    Kind::Unique,
    Kind::Check,
    Kind::ForeignKey,
    Kind::Index,
    Kind::View,
    Kind::Trigger,
    Kind::Routine,
    Kind::Enum,
    Kind::Sequence,
];

const DATABASE: &str = "migate";
const USER: &str = "migate";

/// A throwaway Postgres: a fresh cluster in `dir`, served on a free local port until dropped.
#[derive(Debug)]
pub struct Server {
    bin: PathBuf,
    url: String,
    child: Child,
}

impl Server {
    /// `initdb` a cluster under `dir`, start it, create the `migate` database.
    pub fn start(dir: &Path) -> Result<Server, String> {
        let bin = bindir()?;
        let data = dir.join("pgdata");
        let out = Command::new(bin.join("initdb"))
            .args([
                "-D",
                &data.to_string_lossy(),
                "-U",
                USER,
                "--auth=trust",
                "-E",
                "UTF8",
                "--no-locale",
            ])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run initdb: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "initdb failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let port = free_port()?;
        let child = Command::new(bin.join("postgres"))
            .args(["-D", &data.to_string_lossy(), "-p", &port.to_string()])
            // TCP only: a Unix socket under the run's folder can pass the OS path limit (about 103 bytes on macOS)
            // when TMPDIR is long, and then the server never starts.
            .args(["-c", "unix_socket_directories="])
            // Throwaway data: durability settings only cost time.
            .args([
                "-c",
                "listen_addresses=127.0.0.1",
                "-c",
                "fsync=off",
                "-c",
                "synchronous_commit=off",
            ])
            .args(["-c", "full_page_writes=off"])
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start postgres: {e}"))?;
        let admin = format!("postgresql://{USER}@127.0.0.1:{port}/postgres");
        let server = Server {
            url: format!("postgresql://{USER}@127.0.0.1:{port}/{DATABASE}"),
            bin,
            child,
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if psql(&server.bin, &admin, "select 1").is_ok() {
                break;
            }
            if Instant::now() > deadline {
                return Err("the throwaway postgres did not accept connections within 30 s".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        psql(&server.bin, &admin, &format!("create database {DATABASE}"))?;
        Ok(server)
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn version(&self) -> Result<String, String> {
        psql(
            &self.bin,
            &self.url,
            "select current_setting('server_version')",
        )
    }

    /// The schema of the `migate` database.
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        snapshot(&self.bin, &self.url)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SIGINT is Postgres's fast shutdown; SIGKILL if it has not gone after ten seconds.
        let pid = self.child.id().to_string();
        let _ = Command::new("kill").args(["-INT", &pid]).status();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Stop the server a killed run left running in `dir` (the folder [`Server::start`] was given), if any. Its data
/// folder names the server's process id; that process is signalled only if it is still that very server, since
/// process ids are reused.
pub fn stop_left_server(dir: &Path) {
    let data = dir.join("pgdata");
    let Some(pid) = std::fs::read_to_string(data.join("postmaster.pid"))
        .ok()
        .and_then(|text| text.lines().next()?.trim().parse::<u32>().ok())
    else {
        return;
    };
    let pid = pid.to_string();
    let command = Command::new("ps")
        .args(["-p", &pid, "-o", "command="])
        .output();
    let is_that_server = command.is_ok_and(|out| {
        let line = String::from_utf8_lossy(&out.stdout);
        line.contains("postgres") && line.contains(&*data.to_string_lossy())
    });
    if !is_that_server {
        return;
    }
    let _ = Command::new("kill").args(["-INT", &pid]).status();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let alive = Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status();
        if !alive.is_ok_and(|s| s.success()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = Command::new("kill").args(["-KILL", &pid]).status();
}

impl engine::Sandbox for Server {
    fn placeholders(&self) -> Vec<(&'static str, String)> {
        vec![("url", self.url.clone())]
    }

    fn process_group(&self) -> Option<u32> {
        Some(self.child.id())
    }

    fn open(&self, _history: &str) -> Result<Box<dyn engine::Database + '_>, String> {
        Ok(Box::new(Db(self)))
    }
}

struct Db<'a>(&'a Server);

impl engine::Database for Db<'_> {
    fn snapshot(&self) -> Result<Snapshot, String> {
        self.0.snapshot()
    }

    fn count_rows(&self, sql: &str) -> Result<usize, String> {
        let n = psql(
            &self.0.bin,
            &self.0.url,
            &format!("select count(*) from ({sql}) q"),
        )?;
        n.trim().parse().map_err(|e| format!("count `{n}`: {e}"))
    }

    fn row_counts(&self) -> Result<BTreeMap<String, u64>, String> {
        let rows = rows(
            &self.0.bin,
            &self.0.url,
            &format!(
                "select n.nspname as ns, c.relname as name, \
             (xpath('/row/n/text()', query_to_xml(format('select count(*) as n from %I.%I', n.nspname, c.relname), \
             false, true, '')))[1]::text::bigint as n \
             from pg_class c join pg_namespace n on n.oid = c.relnamespace \
             where c.relkind in ('r', 'p') and not c.relispartition and {USER_SCHEMA}"
            ),
        )?;
        Ok(rows
            .iter()
            .map(|r| {
                (
                    format!("{}.{}", s(r, "ns"), s(r, "name")),
                    r["n"].as_u64().unwrap_or(0),
                )
            })
            .collect())
    }

    fn drop_all_but(&self, keep: &str) -> Result<(), String> {
        let kept: BTreeSet<String> = keep
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();
        let objects = rows(
            &self.0.bin,
            &self.0.url,
            &format!(
                "select n.nspname as ns, c.relname as name, case c.relkind when 'v' then 'VIEW' \
             when 'm' then 'MATERIALIZED VIEW' else 'TABLE' end as kind \
             from pg_class c join pg_namespace n on n.oid = c.relnamespace \
             where c.relkind in ('r', 'p', 'v', 'm') and not c.relispartition and {USER_SCHEMA} order by 3 desc"
            ),
        )?;
        let drops: Vec<String> = objects
            .iter()
            .filter(|o| !kept.contains(&s(o, "name").to_ascii_lowercase()))
            .map(|o| {
                format!(
                    "drop {} if exists {}.{} cascade;",
                    s(o, "kind"),
                    quoted(s(o, "ns")),
                    quoted(s(o, "name"))
                )
            })
            .collect();
        if drops.is_empty() {
            return Ok(());
        }
        psql(&self.0.bin, &self.0.url, &drops.concat()).map(|_| ())
    }
}

/// Where the Postgres programs are: `MIGATE_PG_BIN`, then `postgres` on PATH, then the usual install folders.
fn bindir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("MIGATE_PG_BIN") {
        return Ok(PathBuf::from(dir));
    }
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .find(|d| d.join("postgres").is_file() && d.join("initdb").is_file());
    if let Some(dir) = on_path {
        return Ok(dir);
    }
    // Homebrew keeps the server out of PATH (`pg_config` there is often libpq's); Debian and Ubuntu keep it per version.
    let mut found: Vec<(u32, PathBuf)> = Vec::new();
    for (root, prefix) in [
        ("/opt/homebrew/opt", "postgresql@"),
        ("/usr/local/opt", "postgresql@"),
        ("/usr/lib/postgresql", ""),
    ] {
        for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(version) = name
                .strip_prefix(prefix)
                .and_then(|v| v.parse::<u32>().ok())
            else {
                continue;
            };
            let bin = entry.path().join("bin");
            if bin.join("postgres").is_file() {
                found.push((version, bin));
            }
        }
    }
    found.sort();
    found.pop().map(|(_, bin)| bin).ok_or_else(|| {
        "no PostgreSQL server programs found (postgres, initdb): install PostgreSQL, put its bin folder on PATH, or set \
         MIGATE_PG_BIN"
            .into()
    })
}

fn free_port() -> Result<u16, String> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("cannot find a free port: {e}"))
}

/// Run one statement (or several) with `psql`; its output, trimmed.
fn psql(bin: &Path, url: &str, sql: &str) -> Result<String, String> {
    let out = Command::new(bin.join("psql"))
        .args([
            "-X",
            "-A",
            "-t",
            "-q",
            "-v",
            "ON_ERROR_STOP=1",
            "-d",
            url,
            "-c",
            sql,
        ])
        .env("PGCONNECT_TIMEOUT", "5")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run psql: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The rows of `query` as JSON objects.
fn rows(bin: &Path, url: &str, query: &str) -> Result<Vec<Value>, String> {
    let text = psql(
        bin,
        url,
        &format!("select coalesce(json_agg(q), '[]') from ({query}) q"),
    )?;
    match serde_json::from_str(&text).map_err(|e| format!("catalog JSON: {e}"))? {
        Value::Array(rows) => Ok(rows),
        other => Err(format!("catalog JSON: expected an array, got {other}")),
    }
}

fn s<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or_default()
}

fn opt(row: &Value, key: &str) -> Option<String> {
    row[key].as_str().map(str::to_string)
}

fn names(row: &Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn quoted(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Schemas the project owns: not the system's, not another session's temporary ones.
const USER_SCHEMA: &str =
    "n.nspname not in ('pg_catalog', 'information_schema') and n.nspname not like 'pg\\_%'";

/// Objects an extension installed are the extension's, not the project's.
const NOT_EXTENSION: &str =
    "not exists (select 1 from pg_depend e where e.objid = {oid} and e.deptype = 'e')";

/// A table the project defines: ordinary or partitioned, not one partition of another.
const TABLE: &str = "c.relkind in ('r', 'p') and not c.relispartition";

/// Read the schema of the database at `url` into the canonical snapshot.
pub fn snapshot(bin: &Path, url: &str) -> Result<Snapshot, String> {
    let q = |sql: String| rows(bin, url, &sql);
    let mut objects = Vec::new();
    let top = |ns: &str, name: &str, detail| Object {
        namespace: ns.into(),
        name: name.into(),
        parent: None,
        detail,
    };
    let on = |ns: &str, table: &str, name: &str, detail| Object {
        namespace: ns.into(),
        name: name.into(),
        parent: Some(table.into()),
        detail,
    };
    let not_ext = |oid: &str| NOT_EXTENSION.replace("{oid}", oid);

    for r in q(format!(
        "select n.nspname as ns from pg_namespace n where {USER_SCHEMA} and {}",
        not_ext("n.oid")
    ))? {
        objects.push(top(s(&r, "ns"), s(&r, "ns"), Detail::Namespace {}));
    }
    for r in q(format!(
        "select n.nspname as ns, c.relname as name from pg_class c join pg_namespace n on n.oid = c.relnamespace \
         where {TABLE} and {USER_SCHEMA} and {}",
        not_ext("c.oid")
    ))? {
        objects.push(top(
            s(&r, "ns"),
            s(&r, "name"),
            Detail::Table {
                strict: false,
                without_rowid: false,
            },
        ));
    }

    let columns = q(format!(
        "select n.nspname as ns, c.relname as tbl, a.attname as name, a.attnum as num, \
         format_type(a.atttypid, a.atttypmod) as raw, t.typname, t.typcategory as cat, a.atttypmod as typmod, \
         a.attnotnull as notnull, a.attgenerated as gen, a.attidentity as ident, \
         pg_get_expr(d.adbin, d.adrelid) as expr, \
         case when a.attcollation <> t.typcollation then co.collname end as collation \
         from pg_attribute a join pg_class c on c.oid = a.attrelid join pg_namespace n on n.oid = c.relnamespace \
         join pg_type t on t.oid = a.atttypid \
         left join pg_attrdef d on d.adrelid = a.attrelid and d.adnum = a.attnum \
         left join pg_collation co on co.oid = a.attcollation \
         where {TABLE} and a.attnum > 0 and not a.attisdropped and {USER_SCHEMA} and {} \
         order by 1, 2, a.attnum",
        not_ext("c.oid")
    ))?;
    let mut ordinal: BTreeMap<(String, String), u32> = BTreeMap::new();
    for r in &columns {
        let n = ordinal
            .entry((s(r, "ns").into(), s(r, "tbl").into()))
            .or_default();
        let generated = (!s(r, "gen").is_empty()).then(|| Generated {
            expression: opt(r, "expr").unwrap_or_default(),
            stored: s(r, "gen") == "s",
        });
        let default = if generated.is_some() {
            None
        } else {
            opt(r, "expr")
        };
        let serial = default
            .as_deref()
            .is_some_and(|d| d.starts_with("nextval("));
        objects.push(on(
            s(r, "ns"),
            s(r, "tbl"),
            s(r, "name"),
            Detail::Column(Column {
                ordinal: *n,
                data_type: data_type(r),
                nullable: !r["notnull"].as_bool().unwrap_or(false),
                default,
                generated,
                identity: !s(r, "ident").is_empty() || serial,
                collation: opt(r, "collation"),
            }),
        ));
        *n += 1;
    }

    for r in q(format!(
        "select n.nspname as ns, c.relname as tbl, con.conname as name, con.contype as type, \
         (select json_agg(a.attname order by k.i) from unnest(con.conkey) with ordinality k(num, i) \
          join pg_attribute a on a.attrelid = con.conrelid and a.attnum = k.num) as cols, \
         pg_get_constraintdef(con.oid, true) as def, con.convalidated as validated, \
         rn.nspname as ref_ns, rc.relname as ref_table, \
         (select json_agg(a.attname order by k.i) from unnest(con.confkey) with ordinality k(num, i) \
          join pg_attribute a on a.attrelid = con.confrelid and a.attnum = k.num) as ref_cols, \
         con.confdeltype as on_delete, con.confupdtype as on_update, \
         con.condeferrable as deferrable, con.condeferred as deferred \
         from pg_constraint con join pg_class c on c.oid = con.conrelid \
         join pg_namespace n on n.oid = c.relnamespace \
         left join pg_class rc on rc.oid = con.confrelid left join pg_namespace rn on rn.oid = rc.relnamespace \
         where con.contype in ('p', 'u', 'c', 'f') and {TABLE} and {USER_SCHEMA}"
    ))? {
        let validated = r["validated"].as_bool().unwrap_or(true);
        let detail = match s(&r, "type") {
            "p" => Detail::PrimaryKey {
                columns: names(&r, "cols"),
            },
            "u" => Detail::Unique {
                columns: names(&r, "cols"),
            },
            "c" => Detail::Check {
                expression: check_expression(s(&r, "def")),
                validated,
            },
            _ => Detail::ForeignKey(ForeignKey {
                columns: names(&r, "cols"),
                ref_namespace: s(&r, "ref_ns").into(),
                ref_table: s(&r, "ref_table").into(),
                ref_columns: names(&r, "ref_cols"),
                on_delete: action(s(&r, "on_delete")).into(),
                on_update: action(s(&r, "on_update")).into(),
                deferrable: r["deferrable"].as_bool().unwrap_or(false),
                initially_deferred: r["deferred"].as_bool().unwrap_or(false),
                validated,
            }),
        };
        objects.push(on(s(&r, "ns"), s(&r, "tbl"), s(&r, "name"), detail));
    }

    for r in q(format!(
        "select n.nspname as ns, c.relname as tbl, i.relname as name, x.indisunique as uniq, am.amname as method, \
         pg_get_expr(x.indpred, x.indrelid, true) as predicate, \
         (select json_agg(json_build_object('col', a.attname, \
            'expr', case when k.num = 0 then pg_get_indexdef(x.indexrelid, k.i::int, true) end, \
            'desc', pg_index_column_has_property(x.indexrelid, k.i::int, 'desc')) order by k.i) \
          from unnest(x.indkey::int2[]) with ordinality k(num, i) \
          left join pg_attribute a on a.attrelid = x.indrelid and a.attnum = k.num \
          where k.i <= x.indnkeyatts) as parts \
         from pg_index x join pg_class i on i.oid = x.indexrelid join pg_class c on c.oid = x.indrelid \
         join pg_namespace n on n.oid = c.relnamespace join pg_am am on am.oid = i.relam \
         where {TABLE} and {USER_SCHEMA} \
         and not exists (select 1 from pg_constraint k where k.conindid = x.indexrelid and k.contype in ('p', 'u', 'x'))"
    ))? {
        let parts = r["parts"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .map(|p| {
                        let descending = p["desc"].as_bool().unwrap_or(false);
                        match p["expr"].as_str() {
                            Some(sql) => IndexPart::Expression {
                                sql: sql.into(),
                                descending,
                            },
                            None => IndexPart::Column {
                                name: s(p, "col").into(),
                                descending,
                            },
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        objects.push(on(
            s(&r, "ns"),
            s(&r, "tbl"),
            s(&r, "name"),
            Detail::Index(Index {
                unique: r["uniq"].as_bool().unwrap_or(false),
                parts,
                predicate: opt(&r, "predicate"),
                method: opt(&r, "method"),
            }),
        ));
    }

    for r in q(format!(
        "select n.nspname as ns, c.relname as name, pg_get_viewdef(c.oid, true) as def, c.relkind = 'm' as mat \
         from pg_class c join pg_namespace n on n.oid = c.relnamespace \
         where c.relkind in ('v', 'm') and {USER_SCHEMA} and {}",
        not_ext("c.oid")
    ))? {
        objects.push(top(
            s(&r, "ns"),
            s(&r, "name"),
            Detail::View {
                definition: s(&r, "def").trim().into(),
                materialized: r["mat"].as_bool().unwrap_or(false),
            },
        ));
    }

    for r in q(format!(
        "select n.nspname as ns, c.relname as tbl, tg.tgname as name, tg.tgtype as type, \
         pg_get_triggerdef(tg.oid, true) as def \
         from pg_trigger tg join pg_class c on c.oid = tg.tgrelid join pg_namespace n on n.oid = c.relnamespace \
         where not tg.tgisinternal and {TABLE} and {USER_SCHEMA}"
    ))? {
        let (timing, events) = trigger_shape(r["type"].as_u64().unwrap_or(0));
        objects.push(on(
            s(&r, "ns"),
            s(&r, "tbl"),
            s(&r, "name"),
            Detail::Trigger {
                timing,
                events,
                definition: s(&r, "def").into(),
            },
        ));
    }

    for r in q(format!(
        "select n.nspname as ns, p.proname as name, pg_get_function_identity_arguments(p.oid) as args, \
         pg_get_functiondef(p.oid) as def \
         from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
         where p.prokind in ('f', 'p') and {USER_SCHEMA} and {}",
        not_ext("p.oid")
    ))? {
        // Overloads share a name, so the key carries the arguments.
        let args = s(&r, "args");
        let digest = Sha256::digest(s(&r, "def").as_bytes());
        objects.push(top(
            s(&r, "ns"),
            &format!("{}({args})", s(&r, "name")),
            Detail::Routine {
                arguments: args.into(),
                definition_sha256: digest.iter().map(|b| format!("{b:02x}")).collect(),
            },
        ));
    }

    for r in q(format!(
        "select n.nspname as ns, t.typname as name, \
         (select json_agg(e.enumlabel order by e.enumsortorder) from pg_enum e where e.enumtypid = t.oid) as vals \
         from pg_type t join pg_namespace n on n.oid = t.typnamespace \
         where t.typtype = 'e' and {USER_SCHEMA} and {}",
        not_ext("t.oid")
    ))? {
        objects.push(top(
            s(&r, "ns"),
            s(&r, "name"),
            Detail::Enum {
                values: names(&r, "vals"),
            },
        ));
    }

    for r in q(format!(
        "select n.nspname as ns, c.relname as name, format_type(sq.seqtypid, null) as type, \
         (select oc.relname || '.' || a.attname from pg_depend d join pg_class oc on oc.oid = d.refobjid \
          join pg_attribute a on a.attrelid = d.refobjid and a.attnum = d.refobjsubid \
          where d.objid = c.oid and d.deptype = 'a' and d.refobjsubid > 0) as owned_by \
         from pg_class c join pg_namespace n on n.oid = c.relnamespace join pg_sequence sq on sq.seqrelid = c.oid \
         where c.relkind = 'S' and {USER_SCHEMA} and {} \
         and not exists (select 1 from pg_depend d where d.objid = c.oid and d.deptype = 'i')",
        not_ext("c.oid")
    ))? {
        // Identity columns' own sequences (deptype 'i') are the column's detail, not objects of their own.
        objects.push(top(
            s(&r, "ns"),
            s(&r, "name"),
            Detail::Sequence {
                data_type: s(&r, "type").into(),
                owned_by: opt(&r, "owned_by"),
            },
        ));
    }

    let version = psql(bin, url, "select current_setting('server_version')")?;
    Snapshot::new(
        "postgres",
        Some(version),
        CAPABILITIES.into_iter().collect(),
        objects,
    )
    .map_err(|e| e.0)
}

/// A column's type: the engine's own rendering, a family the rules compare, and the size limits Postgres enforces.
fn data_type(r: &Value) -> DataType {
    let typname = s(r, "typname");
    let family = match (typname, s(r, "cat")) {
        ("int2" | "int4" | "int8", _) => TypeFamily::Integer,
        ("numeric", _) => TypeFamily::Decimal,
        ("float4" | "float8", _) => TypeFamily::Float,
        ("text" | "varchar" | "bpchar" | "char" | "name" | "citext", _) => TypeFamily::Text,
        ("bytea", _) => TypeFamily::Binary,
        ("bool", _) => TypeFamily::Boolean,
        ("date", _) => TypeFamily::Date,
        ("time" | "timetz", _) => TypeFamily::Time,
        ("timestamp" | "timestamptz", _) => TypeFamily::Timestamp,
        ("uuid", _) => TypeFamily::Uuid,
        ("json" | "jsonb", _) => TypeFamily::Json,
        (_, "A") => TypeFamily::Array,
        (_, "E") => TypeFamily::Enum,
        _ => TypeFamily::Other,
    };
    // atttypmod carries the declared size: n + 4 for varchar(n) and char(n), ((p << 16) | s) + 4 for numeric(p, s).
    let typmod = r["typmod"].as_i64().unwrap_or(-1);
    let modifier = u32::try_from(typmod - 4).ok().filter(|_| typmod >= 4);
    let (length, precision, scale) = match (typname, modifier) {
        ("varchar" | "bpchar", Some(n)) => (Some(n), None, None),
        ("numeric", Some(m)) => (None, Some(m >> 16), Some(m & 0xffff)),
        _ => (None, None, None),
    };
    DataType {
        raw: s(r, "raw").into(),
        family,
        length,
        precision,
        scale,
    }
}

/// `CHECK ((price > 0)) NOT VALID` -> `price > 0`.
fn check_expression(def: &str) -> String {
    let body = def
        .trim_start_matches("CHECK")
        .trim()
        .trim_end_matches(" NOT VALID");
    let mut body = body.trim();
    while body.starts_with('(') && body.ends_with(')') && balanced(&body[1..body.len() - 1]) {
        body = body[1..body.len() - 1].trim();
    }
    body.to_string()
}

/// Whether parentheses balance (so outer ones wrap the whole expression), ignoring those inside quotes.
fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    let mut quote = None;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// pg_constraint's action letters.
fn action(code: &str) -> &'static str {
    match code {
        "r" => "RESTRICT",
        "c" => "CASCADE",
        "n" => "SET NULL",
        "d" => "SET DEFAULT",
        _ => "NO ACTION",
    }
}

/// pg_trigger.tgtype bits: 2 BEFORE, 64 INSTEAD OF (else AFTER); 4 INSERT, 8 DELETE, 16 UPDATE, 32 TRUNCATE.
fn trigger_shape(bits: u64) -> (String, Vec<String>) {
    let timing = match (bits & 2 != 0, bits & 64 != 0) {
        (_, true) => "INSTEAD OF",
        (true, false) => "BEFORE",
        (false, false) => "AFTER",
    };
    let events = [
        (4, "INSERT"),
        (8, "DELETE"),
        (16, "UPDATE"),
        (32, "TRUNCATE"),
    ]
    .into_iter()
    .filter(|(bit, _)| bits & bit != 0)
    .map(|(_, e)| e.to_string())
    .collect();
    (timing.into(), events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_expressions_lose_only_their_wrapping() {
        assert_eq!(check_expression("CHECK ((price > 0))"), "price > 0");
        assert_eq!(check_expression("CHECK (price > 0) NOT VALID"), "price > 0");
        assert_eq!(
            check_expression("CHECK ((a > 0) AND (b > 0))"),
            "(a > 0) AND (b > 0)"
        );
        assert_eq!(
            check_expression("CHECK ((name <> ')'::text))"),
            "name <> ')'::text"
        );
    }

    #[test]
    fn trigger_bits_decode() {
        assert_eq!(
            trigger_shape(2 | 4 | 16 | 1),
            ("BEFORE".into(), vec!["INSERT".into(), "UPDATE".into()])
        );
        assert_eq!(trigger_shape(8), ("AFTER".into(), vec!["DELETE".into()]));
        assert_eq!(
            trigger_shape(64 | 4 | 1),
            ("INSTEAD OF".into(), vec!["INSERT".into()])
        );
    }
}
