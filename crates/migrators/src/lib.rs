//! Migrator definitions are data (`migrators/*.toml`); this crate reads them and turns a project's migration files into
//! the core's [`Migration`] list. Adding a migrator means adding a file, not code, unless it needs a new order model.

use std::collections::BTreeMap;
use std::fmt;

use migate_core::ledger::MigratorChecksum;
use migate_core::order::{Migration, OrderModel, OutOfOrder, Policy};
use regex::Regex;
use serde::Deserialize;

const BUNDLED: &[(&str, &str)] = &[
    ("payload", include_str!("../migrators/payload.toml")),
    ("prisma", include_str!("../migrators/prisma.toml")),
    ("drizzle", include_str!("../migrators/drizzle.toml")),
    ("wrangler-d1", include_str!("../migrators/wrangler-d1.toml")),
    ("rails", include_str!("../migrators/rails.toml")),
    ("django", include_str!("../migrators/django.toml")),
    ("supabase", include_str!("../migrators/supabase.toml")),
    ("dbmate", include_str!("../migrators/dbmate.toml")),
];

#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum OrderName {
    Lexical,
    Numeric,
    TimestampPrefix,
    SingleVersion,
    Graph,
}

/// A migration's parents, read from its own file: the bracketed list `list` opens (the pattern matches up to and
/// including the list's `[` or `(`, which ends at its matching bracket), each parent matching `parent` with named groups `group`
/// and `id`. Ids matching `skip` name no concrete migration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphDef {
    list: String,
    parent: String,
    skip: Option<String>,
}

#[derive(Debug, Clone)]
struct Graph {
    list: Regex,
    parent: Regex,
    skip: Option<Regex>,
}

/// Migrations listed in one JSON journal (the file `path_pattern` matches) rather than one file each: the array
/// `entries`, each entry's `id` and `version` fields, and `file`, where each migration's code lives relative to the
/// journal's folder (`{id}` stands for the entry's id).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    entries: String,
    id: String,
    version: String,
    file: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ChecksumName {
    WholeFile,
    None,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryKind {
    /// Every applied migration is a row (Payload, Prisma, Rails, Django, Wrangler).
    Full,
    /// Only the current version is stored (Alembic, golang-migrate).
    HeadOnly,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub kind: HistoryKind,
    pub query: String,
    /// The same query where Postgres keeps the history elsewhere (Drizzle uses its own schema); `query` otherwise.
    #[serde(default)]
    pub postgres: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Migrate {
    /// Run in the checkout. `{db}` is the throwaway database's path, `{dir}` its folder, `{database}` the database's
    /// name in the project's own configuration (`[sandbox] database_name`).
    pub command: String,
    /// Variables for every command the run starts (migrate, setup, seed, tests).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Variables for the migrate command only, such as a run mode the tests must not inherit.
    #[serde(default)]
    pub migrate_env: BTreeMap<String, String>,
    /// Where the tool writes the database when it picks the file name itself (a `*` in the file name matches).
    pub database: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Def {
    id: String,
    name: String,
    path_pattern: String,
    journal: Option<Journal>,
    order: OrderName,
    graph: Option<GraphDef>,
    out_of_order: String,
    #[serde(default)]
    silent_skip: bool,
    #[serde(default)]
    foreign_keys_off_for_rebuilds: bool,
    #[serde(default = "block")]
    duplicate_versions: String,
    checksum: ChecksumName,
    history: History,
    migrate: Migrate,
    references: Option<ReferencesDef>,
}

/// Where the base version's code says it uses a column (MG212), for an ORM that keeps that knowledge in code.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferencesDef {
    /// The files that knowledge lives in, as globs relative to the project (`**/*.prisma`); migrations never count.
    files: Vec<String>,
    /// Regex patterns, with `{column}` (the database's name), `{field}` (its camelCase form), `{stem}` (without a
    /// trailing `_id`, a foreign key's field) and `{table}` substituted, each escaped.
    patterns: Vec<String>,
    /// "uses": a match means the column is used. "unless": every column is used unless a pattern matches (Rails, which
    /// loads them all and is told to skip one with ignored_columns).
    #[serde(default = "uses")]
    mode: String,
    /// Count a file only if it also names the table: for schema code that declares a table and its columns together.
    #[serde(default)]
    with_table: bool,
    /// Count a line only if the table's name is within this many lines of it: for raw SQL, where a column's name
    /// alone is too common and one large file names many tables.
    table_within: Option<usize>,
    /// Files that are not the code serving traffic (tests, vendored libraries), as globs; they never count.
    #[serde(default)]
    exclude: Vec<String>,
}

fn uses() -> String {
    "uses".into()
}

/// What the base version's code says about a column a migration drops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnUse {
    /// The migrator's file has no rule for this: nothing can be said.
    Unknown,
    NotUsed,
    /// Used, at these places (`path`, 1-based line).
    UsedAt(Vec<(String, u32)>),
    /// Used by every model unless told otherwise (Rails without ignored_columns).
    UsedByDefault,
}

/// One migrator, read from its data file and validated.
#[derive(Debug, Clone)]
pub struct Migrator {
    pub id: String,
    pub name: String,
    pub history: History,
    pub migrate: Migrate,
    /// The tool rebuilds a table only with foreign keys really off, so a rebuild cannot cascade (MG104 stays quiet).
    pub foreign_keys_off_for_rebuilds: bool,
    pattern: Regex,
    references: Option<(Vec<Regex>, Vec<Regex>, ReferencesDef)>,
    /// The one folder (relative to the project, no trailing `/`) whose files count, when the project names it.
    dir: Option<String>,
    journal: Option<Journal>,
    graph: Option<Graph>,
    policy: Policy,
    checksum: MigratorChecksum,
}

fn block() -> String {
    "block".into()
}

impl Migrator {
    pub fn parse(text: &str) -> Result<Migrator, Error> {
        let d: Def =
            toml::from_str(text).map_err(|e| Error(format!("invalid migrator file: {e}")))?;
        let pattern = Regex::new(&d.path_pattern)
            .map_err(|e| Error(format!("{}: path_pattern: {e}", d.id)))?;
        if d.journal.is_none() {
            for group in ["id", "version"] {
                if !pattern.capture_names().flatten().any(|n| n == group) {
                    return Err(Error(format!(
                        "{}: path_pattern needs a named group `{group}`",
                        d.id
                    )));
                }
            }
        }
        if (d.order == OrderName::Graph) != d.graph.is_some() {
            return Err(Error(format!(
                "{}: `graph` is required with order = \"graph\" and only then",
                d.id
            )));
        }
        let regex = |field: &str, s: &str| {
            Regex::new(s).map_err(|e| Error(format!("{}: graph.{field}: {e}", d.id)))
        };
        let graph = match &d.graph {
            Some(g) => {
                let parent = regex("parent", &g.parent)?;
                for group in ["group", "id"] {
                    if !parent.capture_names().flatten().any(|n| n == group) {
                        return Err(Error(format!(
                            "{}: graph.parent needs a named group `{group}`",
                            d.id
                        )));
                    }
                }
                Some(Graph {
                    list: regex("list", &g.list)?,
                    parent,
                    skip: g.skip.as_deref().map(|s| regex("skip", s)).transpose()?,
                })
            }
            None => None,
        };
        let globs = |list: &[String]| -> Result<Vec<Regex>, Error> {
            list.iter()
                .map(|g| {
                    Regex::new(&glob_regex(g))
                        .map_err(|e| Error(format!("{}: references glob {g}: {e}", d.id)))
                })
                .collect()
        };
        let references = match d.references {
            Some(r) => Some((globs(&r.files)?, globs(&r.exclude)?, r)),
            None => None,
        };
        let level = |field: &str, value: &str| match value {
            "block" => Ok(OutOfOrder::Block),
            "warn" => Ok(OutOfOrder::Warn),
            "allow" => Ok(OutOfOrder::Allow),
            other => Err(Error(format!(
                "{}: {field} must be block, warn or allow, not {other:?}",
                d.id
            ))),
        };
        let out_of_order = level("out_of_order", &d.out_of_order)?;
        let duplicate_versions = level("duplicate_versions", &d.duplicate_versions)?;
        let model = match d.order {
            OrderName::Lexical => OrderModel::Lexical,
            OrderName::Numeric => OrderModel::Numeric,
            OrderName::TimestampPrefix => OrderModel::TimestampPrefix,
            OrderName::SingleVersion => OrderModel::SingleVersion,
            OrderName::Graph => OrderModel::Graph,
        };
        Ok(Migrator {
            id: d.id,
            name: d.name,
            history: d.history,
            migrate: d.migrate,
            foreign_keys_off_for_rebuilds: d.foreign_keys_off_for_rebuilds,
            pattern,
            references,
            dir: None,
            journal: d.journal,
            graph,
            policy: Policy {
                model,
                out_of_order,
                silent_skip: d.silent_skip,
                duplicate_versions,
            },
            checksum: match d.checksum {
                ChecksumName::WholeFile => MigratorChecksum::WholeFile,
                ChecksumName::None => MigratorChecksum::None,
            },
        })
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// Replace what the migrator does with a migration older than the last applied one, for a project whose own
    /// runner differs from the stock tool (one that refuses out-of-order migrations, say).
    pub fn set_out_of_order(&mut self, out_of_order: OutOfOrder) {
        self.policy.out_of_order = out_of_order;
    }

    /// Only files under `dir` (relative to the project) are this migrator's: a project with two migration sets
    /// (D1 and Supabase, say) names the one this tool runs, as Wrangler's `migrations_dir` does.
    pub fn set_migrations_dir(&mut self, dir: &str) {
        let dir = dir.trim_start_matches("./").trim_end_matches('/');
        self.dir = (!dir.is_empty()).then(|| dir.to_string());
    }

    /// What the base version's code (`files`: path and text, relative to the project) says about `table.column`.
    pub fn column_use(&self, files: &[(String, String)], table: &str, column: &str) -> ColumnUse {
        let Some((globs, excluded, def)) = &self.references else {
            return ColumnUse::Unknown;
        };
        let fill = |p: &str| {
            p.replace("{column}", &regex::escape(column))
                .replace("{field}", &regex::escape(&camel(column)))
                .replace(
                    "{stem}",
                    &regex::escape(column.strip_suffix("_id").unwrap_or(column)),
                )
                .replace("{table}", &regex::escape(table))
        };
        let Ok(patterns) = def
            .patterns
            .iter()
            .map(|p| Regex::new(&fill(p)))
            .collect::<Result<Vec<_>, _>>()
        else {
            return ColumnUse::Unknown;
        };
        let names_table = Regex::new(&format!(
            r"\b(?:{}|{})\b",
            regex::escape(table),
            regex::escape(&camel(table))
        ))
        .expect("an escaped name is a valid pattern");
        let mut hits = Vec::new();
        for (path, text) in files {
            if !globs.iter().any(|g| g.is_match(path))
                || excluded.iter().any(|g| g.is_match(path))
                || self.pattern.is_match(path)
                || self.matches(path)
            {
                continue;
            }
            if def.with_table && !names_table.is_match(text) {
                continue;
            }
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                let near = |w: usize| {
                    lines[n.saturating_sub(w)..(n + w + 1).min(lines.len())]
                        .iter()
                        .any(|l| names_table.is_match(l))
                };
                if patterns.iter().any(|p| p.is_match(line)) && def.table_within.is_none_or(near) {
                    hits.push((path.clone(), u32::try_from(n + 1).unwrap_or(u32::MAX)));
                }
            }
        }
        match (def.mode.as_str(), hits.is_empty()) {
            ("unless", true) => ColumnUse::UsedByDefault,
            ("unless", false) | (_, true) => ColumnUse::NotUsed,
            _ => ColumnUse::UsedAt(hits),
        }
    }

    fn in_dir(&self, path: &str) -> bool {
        self.dir.as_deref().is_none_or(|dir| {
            path.strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/'))
        })
    }

    pub fn checksum(&self) -> MigratorChecksum {
        self.checksum
    }

    /// Whether a project file (path relative to the project, with '/') is one this migrator reads.
    pub fn matches(&self, path: &str) -> bool {
        self.captures(path).is_some()
    }

    /// The pattern's id, version and group for a file this migrator reads: from its own path, or, under
    /// `migrations_dir`, as if that folder were the tool's `migrations/`, so a set kept in a folder of another name
    /// (one per database adapter: `src/migrations/postgres`) is read too.
    fn captures(&self, path: &str) -> Option<(String, String, String)> {
        if !self.in_dir(path) {
            return None;
        }
        let read = |p: &str| {
            self.pattern.captures(p).map(|c| {
                let part = |name: &str| c.name(name).map_or("", |m| m.as_str()).to_string();
                (part("id"), part("version"), part("group"))
            })
        };
        read(path).or_else(|| {
            let rest = path.strip_prefix(self.dir.as_deref()?)?.strip_prefix('/')?;
            read(&format!("migrations/{rest}"))
        })
    }

    /// The project's migrations, from `(path relative to the project with '/', contents)` pairs. Files the pattern
    /// does not match are ignored (Payload's `index.ts`, Rails' `schema.rb`).
    pub fn migrations(&self, files: &[(String, String)]) -> Result<Vec<Migration>, Error> {
        Ok(self
            .migration_files(files)?
            .into_iter()
            .map(|(m, _)| m)
            .collect())
    }

    /// Like [`Migrator::migrations`], with the path of the file that holds each migration (for hashing): the matched
    /// file itself, or for a journal the file its `file` setting names.
    pub fn migration_files(
        &self,
        files: &[(String, String)],
    ) -> Result<Vec<(Migration, String)>, Error> {
        let mut out = Vec::new();
        for (path, text) in files {
            let Some((id, version, group)) = self.captures(path) else {
                continue;
            };
            match &self.journal {
                Some(journal) => out.extend(journal_entries(path, text, journal)?),
                None => {
                    let parents = self
                        .graph
                        .as_ref()
                        .map_or_else(Vec::new, |g| graph_parents(text, g));
                    let migration = Migration {
                        id: if group.is_empty() {
                            id.to_string()
                        } else {
                            format!("{group}.{id}")
                        },
                        version,
                        group,
                        parents,
                    };
                    out.push((migration, path.clone()));
                }
            }
        }
        Ok(out)
    }
}

/// What migate knows about the tools a project's commands run (`tools.toml`). The runner reads it and names no tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tools {
    /// Variables passed through to sandbox commands, besides the operating system's own.
    pub pass_env: Vec<String>,
    pub pass_env_prefixes: Vec<String>,
    /// Folders never searched for migrations.
    pub skip_dirs: Vec<String>,
    /// Marks before a passing or skipped test.
    pub test_marks: Vec<String>,
    /// A package manager's own lines about a command it wrapped failing.
    pub wrapper_lines: Vec<String>,
    /// Tables a host keeps in every database for itself.
    pub internal_tables: Vec<String>,
    /// Where package managers install PostgreSQL's server programs, `{major}` standing for the major version.
    pub postgres_bin_dirs: Vec<String>,
    /// Variables set for every sandbox command (`{dir}` is the run's database folder); the project's own values win.
    pub env: BTreeMap<String, String>,
}

/// The bundled `tools.toml`, parsed once. It ships inside the binary and a test parses it, so a failure here is a
/// build mistake, not an input error.
pub fn tools() -> &'static Tools {
    static TOOLS: std::sync::OnceLock<Tools> = std::sync::OnceLock::new();
    TOOLS.get_or_init(|| {
        toml::from_str(include_str!("../tools.toml")).expect("the bundled tools.toml parses")
    })
}

/// Every bundled migrator.
pub fn bundled() -> Result<Vec<Migrator>, Error> {
    BUNDLED
        .iter()
        .map(|(_, text)| Migrator::parse(text))
        .collect()
}

/// A bundled migrator by id.
pub fn get(id: &str) -> Option<Result<Migrator, Error>> {
    BUNDLED
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, text)| Migrator::parse(text))
}

/// `snake_case` as `camelCase`: an ORM field's usual name for a column.
fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut up = false;
    for c in name.chars() {
        if c == '_' {
            up = !out.is_empty();
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// A glob (`**/`, `*`, `?`, `{a,b}`) as an anchored regex over a project-relative path.
fn glob_regex(glob: &str) -> String {
    let mut re = String::from("^");
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    re.push_str("(?:.*/)?");
                } else {
                    re.push_str(".*");
                }
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '{' => re.push_str("(?:"),
            '}' => re.push(')'),
            ',' => re.push('|'),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    re.push('$');
    re
}

/// The migrations a JSON journal lists, each with the file that holds its code.
fn journal_entries(path: &str, text: &str, j: &Journal) -> Result<Vec<(Migration, String)>, Error> {
    let doc: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Error(format!("{path}: not a migration journal: {e}")))?;
    let entries = doc
        .get(&j.entries)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error(format!("{path}: no `{}` array", j.entries)))?;
    let folder = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    entries
        .iter()
        .map(|e| {
            let field = |name: &str| match e.get(name) {
                Some(serde_json::Value::String(s)) => Some(s.clone()),
                Some(serde_json::Value::Number(n)) => Some(n.to_string()),
                _ => None,
            };
            let (Some(id), Some(version)) = (field(&j.id), field(&j.version)) else {
                return Err(Error(format!(
                    "{path}: an entry has no `{}` or no `{}`",
                    j.id, j.version
                )));
            };
            let file = tidy(&format!("{folder}/{}", j.file.replace("{id}", &id)));
            Ok((Migration::linear(id, version), file))
        })
        .collect()
}

/// `a/b/../c` as `a/c`, without a leading `/` for a path relative to the project.
fn tidy(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// A migration's parents as `group/id`, from the list its file declares.
fn graph_parents(text: &str, g: &Graph) -> Vec<String> {
    let Some(open) = g
        .list
        .find(text)
        .map(|m| m.end())
        .filter(|&end| text[..end].ends_with(['[', '(']))
    else {
        return Vec::new();
    };
    // The list ends at its matching bracket; anything nested inside it is balanced.
    let mut depth = 1usize;
    let mut end = text.len();
    for (i, ch) in text[open..].char_indices() {
        match ch {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    g.parent
        .captures_iter(&text[open..end])
        .filter(|c| !g.skip.as_ref().is_some_and(|s| s.is_match(&c["id"])))
        .map(|c| format!("{}/{}", &c["group"], &c["id"]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_tools_file_parses() {
        let t = tools();
        assert!(
            t.internal_tables.iter().any(|x| x == "_cf_KV")
                && t.env.contains_key("WRANGLER_LOG_PATH")
        );
        assert!(
            t.test_marks.iter().any(|m| m == "\u{2193}")
                && t.skip_dirs.iter().any(|d| d == "node_modules")
        );
    }
    use migate_core::order::check;
    use migate_core::{Finding, Severity};

    fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect()
    }

    fn rules(findings: &[Finding]) -> Vec<(&'static str, Severity)> {
        findings.iter().map(|f| (f.rule, f.severity)).collect()
    }

    /// What production ran, each given as it is recorded: a version, or a name (both, as the order rule matches either).
    fn ran(v: &[&str]) -> Vec<migate_core::order::Ran> {
        v.iter()
            .map(|s| migate_core::order::Ran {
                id: Some(s.to_string()),
                version: s.to_string(),
            })
            .collect()
    }

    #[test]
    fn every_bundled_migrator_parses() {
        let all = bundled().expect("bundled migrators are valid");
        let ids: Vec<&str> = all.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "payload",
                "prisma",
                "drizzle",
                "wrangler-d1",
                "rails",
                "django",
                "supabase",
                "dbmate",
            ]
        );
        for (name, _) in BUNDLED {
            assert_eq!(
                get(name).unwrap().unwrap().id,
                *name,
                "file name matches id"
            );
        }
    }

    #[test]
    fn supabase_reads_its_migrations_folder_and_versions() {
        let m = get("supabase").unwrap().unwrap();
        let got = m
            .migrations(&files(&[
                (
                    "apps/web/supabase/migrations/20260716120000_web_portal_schema.sql",
                    "",
                ),
                ("apps/web/supabase/seed.sql", ""),
            ]))
            .unwrap();
        assert_eq!(
            got,
            vec![Migration::linear(
                "20260716120000_web_portal_schema",
                "20260716120000"
            )]
        );
        assert!(
            m.history
                .query
                .contains("supabase_migrations.schema_migrations")
        );
    }

    #[test]
    fn each_orm_says_in_its_own_way_whether_old_code_uses_a_column() {
        let used = |m: &str, files: &[(&str, &str)], table: &str, column: &str| {
            get(m)
                .unwrap()
                .unwrap()
                .column_use(&files_of(files), table, column)
        };
        let at = |p: &str, l: u32| ColumnUse::UsedAt(vec![(p.to_string(), l)]);
        // Rails loads every column unless the model ignores it.
        let model = "app/models/user.rb";
        assert_eq!(
            used(
                "rails",
                &[(model, "class User < ApplicationRecord\nend\n")],
                "users",
                "legacy"
            ),
            ColumnUse::UsedByDefault
        );
        assert_eq!(
            used(
                "rails",
                &[(model, "self.ignored_columns += %w[legacy]\n")],
                "users",
                "legacy"
            ),
            ColumnUse::NotUsed
        );
        // Prisma: a field named like the column, or mapped to it.
        let schema = "prisma/schema.prisma";
        assert_eq!(
            used(
                "prisma",
                &[(
                    schema,
                    "model User {\n  stockCount Int @map(\"stock_count\")\n}\n"
                )],
                "User",
                "stock_count"
            ),
            at(schema, 2)
        );
        assert_eq!(
            used(
                "prisma",
                &[(schema, "model User {\n  id Int @id\n}\n")],
                "User",
                "stock_count"
            ),
            ColumnUse::NotUsed
        );
        // Django: a field, a foreign key's field (no _id), or db_column.
        let models = "shop/models.py";
        assert_eq!(
            used(
                "django",
                &[(
                    models,
                    "class Item(models.Model):\n    owner = models.ForeignKey(Owner)\n"
                )],
                "shop_item",
                "owner_id"
            ),
            at(models, 2)
        );
        assert_eq!(
            used(
                "django",
                &[(
                    models,
                    "    qty = models.IntegerField(db_column='quantity')\n"
                )],
                "shop_item",
                "quantity"
            ),
            at(models, 1)
        );
        // Payload: a collection field, camelCase of the column.
        let cars = "src/collections/Cars.ts";
        assert_eq!(
            used(
                "payload",
                &[(
                    cars,
                    "fields: [\n  { name: 'stockCount', type: 'number' },\n]"
                )],
                "cars",
                "stock_count"
            ),
            at(cars, 2)
        );
        // Drizzle and raw SQL: the column only counts in a file that names its table.
        let s = "src/db/schema.ts";
        assert_eq!(
            used(
                "drizzle",
                &[(
                    s,
                    "export const users = sqliteTable('users', {\n  legacy: text('legacy'),\n})"
                )],
                "users",
                "legacy"
            ),
            at(s, 2)
        );
        assert_eq!(
            used(
                "drizzle",
                &[(
                    s,
                    "export const posts = sqliteTable('posts', { legacy: text('legacy') })"
                )],
                "users",
                "legacy"
            ),
            ColumnUse::NotUsed
        );
        let q = "app/queries.py";
        assert_eq!(
            used(
                "wrangler-d1",
                &[(q, "rows = db.execute('SELECT legacy FROM users')\n")],
                "users",
                "legacy"
            ),
            at(q, 1)
        );
        // Raw SQL: tests, schema dumps and a table named far away are not the code serving traffic.
        let test = "apps/server/__tests__/users.test.ts";
        assert_eq!(
            used(
                "wrangler-d1",
                &[(test, "await db.execute('SELECT legacy FROM users')")],
                "users",
                "legacy"
            ),
            ColumnUse::NotUsed
        );
        let dump = "db/schema/10-objects.sql";
        assert_eq!(
            used(
                "wrangler-d1",
                &[(dump, "CREATE TABLE users (legacy text);")],
                "users",
                "legacy"
            ),
            ColumnUse::NotUsed
        );
        let far = format!("const t = 'users';\n{}const legacy = 1;\n", "\n".repeat(10));
        assert_eq!(
            used("wrangler-d1", &[(q, far.as_str())], "users", "legacy"),
            ColumnUse::NotUsed
        );
        // A migration's own text never counts, whatever it names.
        assert_eq!(
            used(
                "wrangler-d1",
                &[(
                    "migrations/0002_drop.sql",
                    "ALTER TABLE users DROP COLUMN legacy;"
                )],
                "users",
                "legacy"
            ),
            ColumnUse::NotUsed
        );
    }

    fn files_of(files: &[(&str, &str)]) -> Vec<(String, String)> {
        files
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect()
    }

    #[test]
    fn only_django_says_its_rebuilds_keep_foreign_keys_off() {
        // Measured 2026-11-21: Django 6.1 refuses to rebuild with foreign keys on; Rails 8.1's rebuild cascade-deleted
        // children (its PRAGMA foreign_keys = OFF is ignored inside the migration's transaction).
        for m in bundled().unwrap() {
            assert_eq!(
                m.foreign_keys_off_for_rebuilds,
                m.id == "django",
                "{}",
                m.id
            );
        }
    }

    #[test]
    fn invalid_definitions_are_refused() {
        let good = include_str!("../migrators/rails.toml");
        let no_version = good.replace("(?P<version>", "(?P<v>");
        assert!(
            Migrator::parse(&no_version)
                .unwrap_err()
                .0
                .contains("named group `version`")
        );
        let bad_policy = good.replace("out_of_order = \"warn\"", "out_of_order = \"maybe\"");
        assert!(Migrator::parse(&bad_policy).is_err());
        let unknown_field = format!("{good}\nsurprise = 1\n");
        assert!(Migrator::parse(&unknown_field).is_err());
        let django = include_str!("../migrators/django.toml");
        let start = django.find("[graph]").unwrap();
        let end = django.find("[history]").unwrap();
        let graph_order_without_graph = format!("{}{}", &django[..start], &django[end..]);
        assert!(
            Migrator::parse(&graph_order_without_graph)
                .unwrap_err()
                .0
                .contains("`graph` is required")
        );
        let parent_without_groups = django.replace("(?P<group>", "(?P<app>");
        assert!(
            Migrator::parse(&parent_without_groups)
                .unwrap_err()
                .0
                .contains("graph.parent needs a named group `group`")
        );
    }

    #[test]
    fn payload_reads_migrations_and_skips_index() {
        let m = get("payload").unwrap().unwrap();
        let got = m
            .migrations(&files(&[
                ("src/migrations/20261117_101500.ts", ""),
                ("src/migrations/20261117_101500.json", ""),
                ("src/migrations/20261118_232700_init.ts", ""),
                ("src/migrations/20261119_080000_add_v2.ts", ""),
                ("src/migrations/index.ts", ""),
            ]))
            .unwrap();
        let ids: Vec<(&str, &str)> = got
            .iter()
            .map(|g| (g.id.as_str(), g.version.as_str()))
            .collect();
        // `payload migrate:create` without a name writes the timestamp alone (seen on real projects).
        assert_eq!(
            ids,
            [
                ("20261117_101500", "20261117_101500"),
                ("20261118_232700_init", "20261118_232700"),
                ("20261119_080000_add_v2", "20261119_080000")
            ]
        );
        // Payload's history stores the stem; a branch merged with an older timestamp warns.
        let late = m
            .migrations(&files(&[
                ("migrations/20261118_232700_init.ts", ""),
                ("migrations/20261117_000000_late.ts", ""),
            ]))
            .unwrap();
        assert_eq!(
            rules(&check(m.policy(), &late, &ran(&["20261118_232700_init"]))),
            vec![("MG004", Severity::Warning)]
        );
        assert_eq!(
            check(
                m.policy(),
                &got,
                &ran(&["20261117_101500", "20261118_232700_init"])
            ),
            vec![]
        );
    }

    #[test]
    fn migrations_dir_reads_a_set_in_a_folder_of_another_name() {
        // Seen on a real Payload project: one set per database adapter, each in its own folder under `migrations/`.
        let mut m = get("payload").unwrap().unwrap();
        let sets = files(&[
            ("src/migrations/postgres/20241008_105308.ts", ""),
            ("src/migrations/postgres/index.ts", ""),
            ("src/migrations/mongodb/20250128_074922_migration.ts", ""),
        ]);
        assert!(
            m.migrations(&sets).unwrap().is_empty(),
            "no set is under a folder named migrations"
        );
        m.set_migrations_dir("src/migrations/postgres");
        let ids: Vec<String> = m
            .migrations(&sets)
            .unwrap()
            .into_iter()
            .map(|g| g.id)
            .collect();
        assert_eq!(ids, ["20241008_105308"]);
        assert!(m.matches("src/migrations/postgres/20241008_105308.ts"));
        assert!(!m.matches("src/migrations/mongodb/20250128_074922_migration.ts"));
    }

    #[test]
    fn prisma_reads_migration_folders() {
        let m = get("prisma").unwrap().unwrap();
        let got = m
            .migrations(&files(&[
                ("prisma/migrations/20260101000000_init/migration.sql", ""),
                ("prisma/migrations/migration_lock.toml", ""),
            ]))
            .unwrap();
        assert_eq!(
            got,
            vec![Migration::linear("20260101000000_init", "20260101000000")]
        );
    }

    #[test]
    fn wrangler_orders_numerically_and_flags_duplicates() {
        let m = get("wrangler-d1").unwrap().unwrap();
        let got = m
            .migrations(&files(&[
                ("migrations/0009_a.sql", ""),
                ("migrations/0010_b.sql", ""),
            ]))
            .unwrap();
        assert_eq!(check(m.policy(), &got, &ran(&["0009_a.sql"])), vec![]);
        let dup = m
            .migrations(&files(&[
                ("migrations/0002_a.sql", ""),
                ("migrations/0002_b.sql", ""),
            ]))
            .unwrap();
        // Wrangler applies both (whole file names in d1_migrations): a warning.
        assert_eq!(
            rules(&check(m.policy(), &dup, &[])),
            vec![("MG006", Severity::Warning)]
        );
    }

    #[test]
    fn rails_run_mode_is_for_the_migrate_command_only() {
        let m = get("rails").unwrap().unwrap();
        assert_eq!(
            m.migrate.migrate_env.get("RAILS_ENV").map(String::as_str),
            Some("production")
        );
        assert!(
            !m.migrate.env.contains_key("RAILS_ENV"),
            "the tests would run in production mode"
        );
    }

    #[test]
    fn bundled_node_commands_never_download_packages() {
        for m in bundled().unwrap() {
            if m.migrate.command.contains("npx ") {
                assert!(
                    m.migrate.command.contains("npx --no "),
                    "{}: {}",
                    m.id,
                    m.migrate.command
                );
            }
        }
    }

    #[test]
    fn rails_history_stores_versions() {
        let m = get("rails").unwrap().unwrap();
        let got = m
            .migrations(&files(&[
                ("db/migrate/20260101000000_create_orders.rb", ""),
                ("db/migrate/20251231000000_late_branch.rb", ""),
                ("db/schema.rb", ""),
            ]))
            .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(
            rules(&check(m.policy(), &got, &ran(&["20260101000000"]))),
            vec![("MG004", Severity::Warning)]
        );
    }

    const JOURNAL: &str = r#"{"version":"7","dialect":"sqlite","entries":[
        {"idx":0,"version":"6","when":1700000000000,"tag":"0000_init","breakpoints":true},
        {"idx":1,"version":"6","when":1700000500000,"tag":"0001_add","breakpoints":true}]}"#;

    #[test]
    fn drizzle_skips_older_migrations_silently_so_it_is_an_error() {
        let m = get("drizzle").unwrap().unwrap();
        let got = m
            .migrations(&files(&[("drizzle/meta/_journal.json", JOURNAL)]))
            .unwrap();
        assert_eq!(
            got,
            vec![
                Migration::linear("0000_init", "1700000000000"),
                Migration::linear("0001_add", "1700000500000")
            ]
        );
        // Production already ran a migration timestamped later than 0001_add (merged from another branch).
        assert_eq!(check(m.policy(), &got, &ran(&["1700000000000"])), vec![]);
        let f = check(m.policy(), &got, &ran(&["1700000000000", "1700000900000"]));
        assert_eq!(rules(&f), vec![("MG004", Severity::Error)]);
        assert!(f[0].message.contains("silently skip"));
    }

    #[test]
    fn migration_files_know_where_each_migration_lives() {
        let drizzle = get("drizzle").unwrap().unwrap();
        let got = drizzle
            .migration_files(&files(&[("db/drizzle/meta/_journal.json", JOURNAL)]))
            .unwrap();
        let paths: Vec<&str> = got.iter().map(|(_, p)| p.as_str()).collect();
        assert_eq!(
            paths,
            ["db/drizzle/0000_init.sql", "db/drizzle/0001_add.sql"]
        );
        let rails = get("rails").unwrap().unwrap();
        assert!(rails.matches("db/migrate/20260101000000_create_orders.rb"));
        assert!(!rails.matches("db/schema.rb"));
        let got = rails
            .migration_files(&files(&[(
                "db/migrate/20260101000000_create_orders.rb",
                "",
            )]))
            .unwrap();
        assert_eq!(got[0].1, "db/migrate/20260101000000_create_orders.rb");
    }

    #[test]
    fn drizzle_refuses_a_broken_journal() {
        let m = get("drizzle").unwrap().unwrap();
        assert!(
            m.migrations(&files(&[("drizzle/meta/_journal.json", "{")]))
                .is_err()
        );
    }

    const D1: &str =
        "class Migration(migrations.Migration):\n    dependencies = []\n    operations = []\n";
    const D2A: &str = "from django.conf import settings\nclass Migration(migrations.Migration):\n    dependencies = [\n        ('shop', '0001_initial'),\n        migrations.swappable_dependency(settings.AUTH_USER_MODEL),\n        ('auth', '__first__'),\n    ]\n";
    const D2B: &str = "class Migration(migrations.Migration):\n    dependencies = [(\"shop\", \"0001_initial\")]\n";
    const MERGE: &str = "class Migration(migrations.Migration):\n    dependencies = [('shop', '0002_add_a'), ('shop', '0002_add_b')]\n";

    #[test]
    fn django_reads_the_dependency_graph() {
        let m = get("django").unwrap().unwrap();
        let got = m
            .migrations(&files(&[
                ("shop/migrations/0001_initial.py", D1),
                ("shop/migrations/0002_add_a.py", D2A),
            ]))
            .unwrap();
        assert_eq!(got[1].id, "shop.0002_add_a");
        assert_eq!(got[1].parents, ["shop/0001_initial"]);
        assert_eq!(check(m.policy(), &got, &[]), vec![]);
        // The word in a comment or a docstring before the class is not the list (seen in a real project).
        let commented = format!(
            "# Note: this has a couple dependencies on the codebase\nROOTS = {{'a': ROOT['a']}}\n{D2A}"
        );
        let got = m
            .migrations(&files(&[
                ("shop/migrations/0001_initial.py", D1),
                ("shop/migrations/0002_add_a.py", &commented),
            ]))
            .unwrap();
        assert_eq!(got[1].parents, ["shop/0001_initial"]);
        // An annotated attribute, and a commented-out list before the real one (both seen in a blind review).
        for text in [
            "class Migration(migrations.Migration):\n    dependencies: list = [('shop', '0001_initial')]\n".to_string(),
            format!("class Migration(migrations.Migration):\n    # dependencies = [('shop', '0000_gone')]\n{}", &D2A[D2A.find("    dependencies").unwrap()..]),
            // Django accepts a tuple too; it ends at its own closing parenthesis.
            "class Migration(migrations.Migration):\n    dependencies = (\n        ('shop', '0001_initial'),\n    )\n    operations = [migrations.RunPython(f, ('shop', '0000_gone'))]\n".to_string(),
        ] {
            let got = m
                .migrations(&files(&[
                    ("shop/migrations/0001_initial.py", D1),
                    ("shop/migrations/0002_add_a.py", &text),
                ]))
                .unwrap();
            assert_eq!(got[1].parents, ["shop/0001_initial"], "{text}");
        }
    }

    #[test]
    fn django_two_heads_fail_and_a_merge_passes() {
        let m = get("django").unwrap().unwrap();
        let forked = files(&[
            ("shop/migrations/0001_initial.py", D1),
            ("shop/migrations/0002_add_a.py", D2A),
            ("shop/migrations/0002_add_b.py", D2B),
            ("shop/migrations/__init__.py", ""),
        ]);
        let got = m.migrations(&forked).unwrap();
        assert_eq!(
            rules(&check(m.policy(), &got, &[])),
            vec![("MG005", Severity::Error)]
        );
        let mut merged = forked.clone();
        merged.push(("shop/migrations/0003_merge.py".into(), MERGE.into()));
        assert_eq!(
            check(m.policy(), &m.migrations(&merged).unwrap(), &[]),
            vec![]
        );
    }
}
