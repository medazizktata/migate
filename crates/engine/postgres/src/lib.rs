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
/// The throwaway server's superuser: `postgres`, as in a default install, Supabase and most managed services, since
/// real migrations name it (`GRANT ... TO postgres`, `ALTER DEFAULT PRIVILEGES FOR ROLE postgres`).
const USER: &str = "postgres";

/// A throwaway Postgres: a fresh cluster in `dir`, served on a free local port until dropped.
#[derive(Debug)]
pub struct Server {
    bin: PathBuf,
    url: String,
    child: Child,
}

impl Server {
    /// `initdb` a cluster under `dir`, start it, create the `migate` database. The server process is started by
    /// `spawn`, so the caller can start and record it in one step that an interrupt's clean-up cannot fall between.
    /// `target` is production's version (`target_version`): its major's installed server is used when there is one.
    /// `install_dirs` are where package managers put the server programs, as patterns such as
    /// `/opt/homebrew/opt/postgresql@{major}/bin`.
    pub fn start(
        dir: &Path,
        target: Option<&str>,
        install_dirs: &[String],
        spawn: &mut dyn FnMut(&mut Command) -> std::io::Result<Child>,
    ) -> Result<Server, String> {
        let bin = bindir(target.map(major).transpose()?, install_dirs)?;
        let data = dir.join("pgdata");
        let out = Command::new(bin.join("initdb"))
            .env_clear()
            .env("LC_ALL", "C")
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
        let log_path = dir.join("postgres.log");
        let log =
            std::fs::File::create(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
        let mut command = Command::new(bin.join("postgres"));
        command
            .env_clear()
            // macOS refuses to start a postmaster with no locale set ("became multithreaded during startup").
            .env("LC_ALL", "C")
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
            .stderr(log);
        let child = spawn(&mut command).map_err(|e| format!("cannot start postgres: {e}"))?;
        let admin = format!("postgresql://{USER}@127.0.0.1:{port}/postgres");
        let mut server = Server {
            url: format!("postgresql://{USER}@127.0.0.1:{port}/{DATABASE}"),
            bin,
            child,
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if psql(&server.bin, &admin, "select 1").is_ok() {
                break;
            }
            // A server that died (a busy port, a bad setting) will not come back: say why now, not in 30 s.
            if let Ok(Some(status)) = server.child.try_wait() {
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                let tail: Vec<&str> = log.lines().rev().take(5).collect();
                return Err(format!(
                    "the throwaway postgres exited at start ({status}):\n{}",
                    tail.into_iter().rev().collect::<Vec<_>>().join("\n")
                ));
            }
            if Instant::now() > deadline {
                return Err("the throwaway postgres did not accept connections within 30 s".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        psql(&server.bin, &admin, &format!("create database {DATABASE}"))?;
        Ok(server)
    }

    /// The server's process id (also its process group).
    pub fn pid(&self) -> u32 {
        self.child.id()
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
        let _ = Command::new("kill")
            .stderr(Stdio::null())
            .args(["-INT", &pid])
            .status();
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
    let _ = Command::new("kill")
        .stderr(Stdio::null())
        .args(["-INT", &pid])
        .status();
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
    let _ = Command::new("kill")
        .stderr(Stdio::null())
        .args(["-KILL", &pid])
        .status();
}

impl engine::Sandbox for Server {
    fn placeholders(&self) -> Vec<(&'static str, String)> {
        vec![("url", self.url.clone())]
    }

    fn open(&self, _history: &str) -> Result<Box<dyn engine::Database + '_>, String> {
        Ok(Box::new(Db(self)))
    }

    /// None to find by reading: the migrations run on production's major when it is installed, and when it is not,
    /// [`engine::Database::target_gap`] says so.
    fn newer_syntax(
        &self,
        _sql: &str,
        target: &str,
    ) -> Result<Option<migate_core::engine::Syntax>, String> {
        major(target).map(|_| None)
    }
}

/// The major version in `target` (`15`, `15.4`).
fn major(target: &str) -> Result<u32, String> {
    target
        .split('.')
        .next()
        .and_then(|m| m.trim().parse().ok())
        .ok_or_else(|| {
            format!("target_version `{target}` is not a PostgreSQL version such as 15 or 15.4")
        })
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

    fn value_counts(&self) -> Result<BTreeMap<String, u64>, String> {
        let columns = rows(
            &self.0.bin,
            &self.0.url,
            &format!(
                "select n.nspname as ns, c.relname as tbl, a.attname as col \
             from pg_attribute a join pg_class c on c.oid = a.attrelid join pg_namespace n on n.oid = c.relnamespace \
             where {TABLE} and a.attnum > 0 and not a.attisdropped and {USER_SCHEMA} order by 1, 2, a.attnum"
            ),
        )?;
        let mut by_table: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for c in &columns {
            by_table
                .entry((s(c, "ns").into(), s(c, "tbl").into()))
                .or_default()
                .push(s(c, "col").into());
        }
        if by_table.is_empty() {
            return Ok(BTreeMap::new());
        }
        // One query for every table: per table, a JSON object of column -> non-NULL count.
        let parts: Vec<String> = by_table
            .iter()
            .map(|((ns, tbl), cols)| {
                let fields: Vec<String> = cols
                    .iter()
                    .map(|c| format!("count({0}) as {0}", quoted(c)))
                    .collect();
                format!(
                    // A row, not json_build_object: a function takes at most 100 arguments, two per column.
                    "select {} as ns, {} as tbl, (select row_to_json(x) from (select {} from {}.{}) x) as counts",
                    literal(ns),
                    literal(tbl),
                    fields.join(", "),
                    quoted(ns),
                    quoted(tbl)
                )
            })
            .collect();
        let mut counts = BTreeMap::new();
        for r in rows(&self.0.bin, &self.0.url, &parts.join(" union all "))? {
            if let Some(cols) = r["counts"].as_object() {
                for (col, n) in cols {
                    counts.insert(
                        format!("{}.{}.{col}", s(&r, "ns"), s(&r, "tbl")),
                        n.as_u64().unwrap_or(0),
                    );
                }
            }
        }
        Ok(counts)
    }

    /// The server reads its own files: there is no separate reader to be older.
    fn newer_writer(&self) -> Option<String> {
        None
    }

    fn probe_trigger(&self, _table: &str, _trigger: &str) -> Result<engine::TriggerProbe, String> {
        Ok(engine::TriggerProbe::Unknown(
            "not built for PostgreSQL yet".into(),
        ))
    }

    fn target_gap(&self, target: &str) -> Result<Option<migate_core::Finding>, String> {
        let want = major(target)?;
        let running = self.0.version()?;
        if major(&running)? == want {
            return Ok(None);
        }
        Ok(Some(migate_core::Finding::new(
            "MG806",
            migate_core::Severity::Warning,
            "sandbox",
            format!(
                "the migrations ran on PostgreSQL {running}, production runs PostgreSQL {target}: install PostgreSQL \
                 {want} (or point MIGATE_PG_BIN at it) so they run on production's version"
            ),
        )))
    }

    /// None needed: the migrations run on production's own major version.
    fn schema_sql(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(BTreeMap::new())
    }
    /// None needed: PostgreSQL drops a referenced table only with its references, which the diff reports.
    fn table_identities(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(BTreeMap::new())
    }
    fn unmodelled(&self) -> Result<BTreeMap<String, String>, String> {
        let mut out = BTreeMap::new();
        for r in rows(&self.0.bin, &self.0.url, &unmodelled_query())? {
            out.insert(s(&r, "key").to_string(), s(&r, "def").to_string());
        }
        Ok(out)
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

/// Installed servers by major version, found through `install_dirs`: patterns such as
/// `/opt/homebrew/opt/postgresql@{major}/bin`, where `{major}` is the version in a folder's name.
fn installed(install_dirs: &[String]) -> Vec<(u32, PathBuf)> {
    let mut found = Vec::new();
    for pattern in install_dirs {
        let Some((before, after)) = pattern.split_once("{major}") else {
            continue;
        };
        let (parent, prefix) = before.rsplit_once('/').unwrap_or((".", before));
        for entry in std::fs::read_dir(if parent.is_empty() { "/" } else { parent })
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(major) = name
                .strip_prefix(prefix)
                .and_then(|v| v.parse::<u32>().ok())
            else {
                continue;
            };
            let bin = PathBuf::from(format!("{before}{major}{after}"));
            if bin.join("postgres").is_file() {
                found.push((major, bin));
            }
        }
    }
    found
}

/// Where the Postgres programs are: `MIGATE_PG_BIN`, then the installed server of major `want` (production's), then
/// `postgres` on PATH, then the newest in the usual install folders.
fn bindir(want: Option<u32>, install_dirs: &[String]) -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("MIGATE_PG_BIN") {
        return Ok(PathBuf::from(dir));
    }
    if let Some(bin) = want.and_then(|w| installed(install_dirs).into_iter().find(|(v, _)| *v == w))
    {
        return Ok(bin.1);
    }
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .find(|d| d.join("postgres").is_file() && d.join("initdb").is_file());
    if let Some(dir) = on_path {
        return Ok(dir);
    }
    let mut found = installed(install_dirs);
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
        // Nothing from the caller's environment: a PGSSLMODE or PGDATABASE meant for a real database would point
        // these calls elsewhere or make them fail.
        .env_clear()
        .env("PGCONNECT_TIMEOUT", "5")
        .env("PGCLIENTENCODING", "UTF8")
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

/// A SQL string literal.
fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
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

/// One query listing every catalogue object outside the model as (`key`, `def`). Where a catalogue's columns differ
/// between versions, the whole row as JSON is the definition: a before/after comparison on one server needs no more.
fn unmodelled_query() -> String {
    let not_ext = |oid: &str| NOT_EXTENSION.replace("{oid}", oid);
    let on_table = "join pg_class c on c.oid = {rel} join pg_namespace n on n.oid = c.relnamespace";
    let rel = |r: &str| on_table.replace("{rel}", r);
    let named = "n.nspname || '.' || c.relname";
    let parts = [
        format!(
            "select 'exclusion constraint ' || {named} || '.' || k.conname, pg_get_constraintdef(k.oid) \
             from pg_constraint k {} where k.contype = 'x' and {USER_SCHEMA}",
            rel("k.conrelid")
        ),
        format!(
            "select 'domain ' || n.nspname || '.' || t.typname, format_type(t.typbasetype, t.typtypmod) \
             || case when t.typnotnull then ' not null' else '' end || coalesce(' default ' || t.typdefault, '') \
             || coalesce((select ' ' || string_agg(pg_get_constraintdef(k.oid), ' ' order by k.conname) \
             from pg_constraint k where k.contypid = t.oid), '') \
             from pg_type t join pg_namespace n on n.oid = t.typnamespace \
             where t.typtype = 'd' and {USER_SCHEMA} and {}",
            not_ext("t.oid")
        ),
        format!(
            "select 'composite type ' || n.nspname || '.' || t.typname, (select string_agg(a.attname || ' ' \
             || format_type(a.atttypid, a.atttypmod), ', ' order by a.attnum) from pg_attribute a \
             where a.attrelid = t.typrelid and a.attnum > 0 and not a.attisdropped) \
             from pg_type t join pg_namespace n on n.oid = t.typnamespace join pg_class r on r.oid = t.typrelid \
             where t.typtype = 'c' and r.relkind = 'c' and {USER_SCHEMA} and {}",
            not_ext("t.oid")
        ),
        format!(
            "select 'range type ' || n.nspname || '.' || t.typname, (row_to_json(g)::jsonb - 'rngtypid')::text \
             from pg_type t join pg_namespace n on n.oid = t.typnamespace join pg_range g on g.rngtypid = t.oid \
             where {USER_SCHEMA} and {}",
            not_ext("t.oid")
        ),
        format!(
            "select case p.prokind when 'a' then 'aggregate ' else 'window function ' end || n.nspname || '.' \
             || p.proname || '(' || pg_get_function_identity_arguments(p.oid) || ')', \
             coalesce((select (row_to_json(g)::jsonb - 'aggfnoid')::text from pg_aggregate g \
             where g.aggfnoid = p.oid), p.prosrc) \
             from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
             where p.prokind in ('a', 'w') and {USER_SCHEMA} and {}",
            not_ext("p.oid")
        ),
        format!(
            "select 'index on a materialized view ' || {named} || '.' || i.relname, pg_get_indexdef(x.indexrelid) \
             from pg_index x join pg_class i on i.oid = x.indexrelid {} where c.relkind = 'm' and {USER_SCHEMA}",
            rel("x.indrelid")
        ),
        format!(
            "select 'index with INCLUDE columns ' || {named} || '.' || i.relname, pg_get_indexdef(x.indexrelid) \
             from pg_index x join pg_class i on i.oid = x.indexrelid {} \
             where x.indnkeyatts < x.indnatts and {TABLE} and {USER_SCHEMA}",
            rel("x.indrelid")
        ),
        format!(
            "select 'trigger on a view ' || {named} || '.' || g.tgname, pg_get_triggerdef(g.oid) \
             from pg_trigger g {} where c.relkind = 'v' and not g.tgisinternal and {USER_SCHEMA}",
            rel("g.tgrelid")
        ),
        format!(
            "select 'rule ' || {named} || '.' || w.rulename, pg_get_ruledef(w.oid) \
             from pg_rewrite w {} where w.rulename <> '_RETURN' and {USER_SCHEMA}",
            rel("w.ev_class")
        ),
        format!(
            "select 'row-level security ' || {named}, \
             case when c.relforcerowsecurity then 'enabled, forced' else 'enabled' end \
             from pg_class c join pg_namespace n on n.oid = c.relnamespace \
             where (c.relrowsecurity or c.relforcerowsecurity) and {USER_SCHEMA}"
        ),
        format!(
            "select 'policy ' || {named} || '.' || p.polname, p.polcmd::text || ' ' || p.polpermissive::text || ' ' \
             || array(select r::regrole::text from unnest(p.polroles) r where r <> 0)::text || ' ' \
             || coalesce(pg_get_expr(p.polqual, p.polrelid), '') || ' ' \
             || coalesce(pg_get_expr(p.polwithcheck, p.polrelid), '') \
             from pg_policy p {} where {USER_SCHEMA}",
            rel("p.polrelid")
        ),
        format!(
            "select 'statistics ' || n.nspname || '.' || x.stxname, pg_get_statisticsobjdef(x.oid) \
             from pg_statistic_ext x join pg_namespace n on n.oid = x.stxnamespace where {USER_SCHEMA}"
        ),
        format!(
            "select 'collation ' || n.nspname || '.' || k.collname, (row_to_json(k)::jsonb - 'oid')::text \
             from pg_collation k join pg_namespace n on n.oid = k.collnamespace where {USER_SCHEMA} and {}",
            not_ext("k.oid")
        ),
        format!(
            "select 'event trigger ' || v.evtname, (row_to_json(v)::jsonb - 'oid')::text from pg_event_trigger v \
             where {}",
            not_ext("v.oid")
        ),
        format!(
            "select case when c.relispartition then 'partition ' else 'inheritance ' end || {named}, \
             pns.nspname || '.' || pc.relname || coalesce(' ' || pg_get_expr(c.relpartbound, c.oid), '') \
             from pg_inherits h {} join pg_class pc on pc.oid = h.inhparent \
             join pg_namespace pns on pns.oid = pc.relnamespace where c.relkind in ('r', 'p', 'f') and {USER_SCHEMA}",
            rel("h.inhrelid")
        ),
        format!(
            "select 'foreign table ' || {named}, (select string_agg(a.attname || ' ' \
             || format_type(a.atttypid, a.atttypmod), ', ' order by a.attnum) from pg_attribute a \
             where a.attrelid = c.oid and a.attnum > 0 and not a.attisdropped) \
             from pg_class c join pg_namespace n on n.oid = c.relnamespace where c.relkind = 'f' and {USER_SCHEMA}"
        ),
        // Grants: an old version that loses SELECT breaks as surely as one that loses a column.
        format!(
            "select 'grants ' || {named}, coalesce(c.relacl::text, '') || coalesce((select ' ' \
             || string_agg(a.attname || a.attacl::text, ' ' order by a.attnum) from pg_attribute a \
             where a.attrelid = c.oid and a.attacl is not null), '') \
             from pg_class c join pg_namespace n on n.oid = c.relnamespace \
             where c.relkind in ('r', 'p', 'v', 'm', 'S', 'f') and {USER_SCHEMA} and (c.relacl is not null \
             or exists (select 1 from pg_attribute a where a.attrelid = c.oid and a.attacl is not null))"
        ),
        format!(
            "select 'grants on routine ' || n.nspname || '.' || p.proname || '(' \
             || pg_get_function_identity_arguments(p.oid) || ')', p.proacl::text \
             from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
             where p.proacl is not null and {USER_SCHEMA} and {}",
            not_ext("p.oid")
        ),
        format!(
            "select 'grants on schema ' || n.nspname, n.nspacl::text from pg_namespace n \
             where n.nspacl is not null and {USER_SCHEMA}"
        ),
    ];
    let all: Vec<String> = parts
        .iter()
        .map(|p| format!("select * from ({p}) u(key, def)"))
        .collect();
    all.join(" union all ")
}

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
         con.condeferrable as deferrable, con.condeferred as deferred, \
         (select (to_jsonb(ix) ->> 'indnullsnotdistinct')::boolean from pg_index ix where ix.indexrelid = con.conindid) as nnd \
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
                nulls_not_distinct: r["nnd"].as_bool().unwrap_or(false),
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
         (to_jsonb(x) ->> 'indnullsnotdistinct')::boolean as nnd, \
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
                nulls_not_distinct: r["nnd"].as_bool().unwrap_or(false),
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
         pg_get_functiondef(p.oid) as def, pg_get_function_arguments(p.oid) as sig, \
         coalesce(pg_get_function_result(p.oid), '') as returns \
         from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
         where p.prokind in ('f', 'p') and {USER_SCHEMA} and {} \
         and not exists (select 1 from pg_depend d where d.classid = 'pg_proc'::regclass and d.objid = p.oid \
         and d.deptype = 'i')",
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
                signature: s(&r, "sig").into(),
                returns: s(&r, "returns").into(),
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
