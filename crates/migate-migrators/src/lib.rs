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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum GraphKind {
    Django,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum VersionSource {
    #[default]
    Path,
    DrizzleJournal,
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
    #[serde(default)]
    version_source: VersionSource,
    order: OrderName,
    graph: Option<GraphKind>,
    out_of_order: String,
    #[serde(default)]
    silent_skip: bool,
    #[serde(default = "block")]
    duplicate_versions: String,
    checksum: ChecksumName,
    history: History,
    migrate: Migrate,
}

/// One migrator, read from its data file and validated.
#[derive(Debug, Clone)]
pub struct Migrator {
    pub id: String,
    pub name: String,
    pub history: History,
    pub migrate: Migrate,
    pattern: Regex,
    version_source: VersionSource,
    graph: Option<GraphKind>,
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
        if d.version_source == VersionSource::Path {
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
            pattern,
            version_source: d.version_source,
            graph: d.graph,
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

    pub fn checksum(&self) -> MigratorChecksum {
        self.checksum
    }

    /// Whether a project file (path relative to the project, with '/') is one this migrator reads.
    pub fn matches(&self, path: &str) -> bool {
        self.pattern.is_match(path)
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

    /// Like [`Migrator::migrations`], with the path of the file that holds each migration (for hashing). For stable
    /// Drizzle that is `<folder>/<tag>.sql` next to `meta/_journal.json`.
    pub fn migration_files(
        &self,
        files: &[(String, String)],
    ) -> Result<Vec<(Migration, String)>, Error> {
        let mut out = Vec::new();
        for (path, text) in files {
            let Some(caps) = self.pattern.captures(path) else {
                continue;
            };
            match self.version_source {
                VersionSource::DrizzleJournal => {
                    let folder = path.strip_suffix("meta/_journal.json").unwrap_or_default();
                    out.extend(drizzle_journal(path, text)?.into_iter().map(|m| {
                        let file = format!("{folder}{}.sql", m.id);
                        (m, file)
                    }));
                }
                VersionSource::Path => {
                    let id = &caps["id"];
                    let version = &caps["version"];
                    let group = caps.name("group").map_or("", |g| g.as_str());
                    let parents = match self.graph {
                        Some(GraphKind::Django) => django_parents(text),
                        None => Vec::new(),
                    };
                    let migration = Migration {
                        id: if group.is_empty() {
                            id.to_string()
                        } else {
                            format!("{group}.{id}")
                        },
                        version: version.to_string(),
                        group: group.to_string(),
                        parents,
                    };
                    out.push((migration, path.clone()));
                }
            }
        }
        Ok(out)
    }
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

#[derive(Deserialize)]
struct Journal {
    entries: Vec<JournalEntry>,
}

#[derive(Deserialize)]
struct JournalEntry {
    when: u64,
    tag: String,
}

/// Stable Drizzle's journal: each entry's `when` is the version `__drizzle_migrations.created_at` records.
fn drizzle_journal(path: &str, text: &str) -> Result<Vec<Migration>, Error> {
    let j: Journal = serde_json::from_str(text)
        .map_err(|e| Error(format!("{path}: not a Drizzle journal: {e}")))?;
    Ok(j.entries
        .into_iter()
        .map(|e| Migration::linear(e.tag, e.when.to_string()))
        .collect())
}

/// Same-app and cross-app parents from a Django migration's `dependencies = [("app", "0001_initial"), ...]`.
/// `__first__`/`__latest__` and `swappable_dependency(...)` name no concrete migration and are skipped.
fn django_parents(text: &str) -> Vec<String> {
    let Some(start) = text.find("dependencies") else {
        return Vec::new();
    };
    let rest = &text[start..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    // The list ends at its matching bracket; tuples inside use parentheses, so bracket depth is enough.
    let mut depth = 0usize;
    let mut end = rest.len();
    for (i, ch) in rest[open..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let tuple =
        Regex::new(r#"\(\s*['"]([\w.]+)['"]\s*,\s*['"]([\w.]+)['"]\s*\)"#).expect("valid regex");
    tuple
        .captures_iter(&rest[open..end])
        .filter(|c| !c[2].starts_with("__"))
        .map(|c| format!("{}/{}", &c[1], &c[2]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn ran(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
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
                "django"
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
        let graph_without_kind =
            include_str!("../migrators/django.toml").replace("graph = \"django\"\n", "");
        assert!(Migrator::parse(&graph_without_kind).is_err());
    }

    #[test]
    fn payload_reads_migrations_and_skips_index() {
        let m = get("payload").unwrap().unwrap();
        let got = m
            .migrations(&files(&[
                ("src/migrations/20261118_232700_init.ts", ""),
                ("src/migrations/20261119_080000_add_v2.ts", ""),
                ("src/migrations/index.ts", ""),
            ]))
            .unwrap();
        let ids: Vec<(&str, &str)> = got
            .iter()
            .map(|g| (g.id.as_str(), g.version.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
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
            check(m.policy(), &got, &ran(&["20261118_232700_init"])),
            vec![]
        );
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
