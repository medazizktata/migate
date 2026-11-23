//! The canonical schema model: what every engine's reader produces and what the diff compares
//! (`docs/research/architecture.md` §4).
//!
//! A snapshot is a flat, sorted set of objects keyed by (kind, namespace, parent, name), plus the set of kinds the
//! reader can see. A kind a reader cannot see is reported as "not checked", never as "none". Types keep the engine's
//! own text (`raw`) next to a normalised family, because a before/after diff on one engine needs only equality, while
//! rules such as "narrowed type" compare families and parameters.
//!
//! The JSON form is published as `schema/snapshot.schema.json` (generated from these types by a test) so out-of-tree
//! readers can produce it; [`Snapshot`] validates everything it is given, including JSON from such readers.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The snapshot JSON format this code reads and writes.
pub const FORMAT: u32 = 1;

/// The kinds of schema object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Namespace,
    Table,
    Column,
    PrimaryKey,
    Unique,
    Check,
    ForeignKey,
    Index,
    View,
    Trigger,
    Routine,
    Enum,
    Sequence,
}

impl Kind {
    /// Whether objects of this kind belong to a table (their `parent`).
    pub fn has_parent(self) -> bool {
        matches!(
            self,
            Kind::Column
                | Kind::PrimaryKey
                | Kind::Unique
                | Kind::Check
                | Kind::ForeignKey
                | Kind::Index
                | Kind::Trigger
        )
    }
}

/// A normalised type family, for rules that compare types across a change (narrowing, for example).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TypeFamily {
    Integer,
    Decimal,
    Float,
    Text,
    Binary,
    Boolean,
    Date,
    Time,
    Timestamp,
    Uuid,
    Json,
    Array,
    Enum,
    Other,
}

/// A column's type: the engine's own text, plus a family and parameters where they apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DataType {
    /// The engine's own rendering, e.g. `character varying(200)` or `INTEGER`.
    pub raw: String,
    pub family: TypeFamily,
    /// Size limits the engine enforces. `None` is unbounded, or declared but not enforced (SQLite ignores the size in
    /// `VARCHAR(20)` or `DECIMAL(10,2)`; the declaration stays in `raw`). Narrowing rules read these.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Generated {
    pub expression: String,
    /// Stored (computed on write) rather than virtual (computed on read).
    pub stored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Column {
    /// Position in the table, from 0.
    pub ordinal: u32,
    pub data_type: DataType,
    pub nullable: bool,
    /// The default expression as the engine renders it; absent when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<Generated>,
    /// An identity or autoincrement column.
    #[serde(default)]
    pub identity: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ForeignKey {
    pub columns: Vec<String>,
    pub ref_namespace: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    /// e.g. `CASCADE`, `RESTRICT`, `NO ACTION`.
    pub on_delete: String,
    pub on_update: String,
    /// The constraint may be deferred to commit (`DEFERRABLE`).
    #[serde(default)]
    pub deferrable: bool,
    /// Checked at commit unless set otherwise (`INITIALLY DEFERRED`). SQLite defers only in this case.
    #[serde(default)]
    pub initially_deferred: bool,
    /// False for a constraint added `NOT VALID` (Postgres) that has not been validated yet.
    #[serde(default = "yes")]
    pub validated: bool,
}

/// One part of an index key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum IndexPart {
    Column {
        name: String,
        #[serde(default)]
        descending: bool,
    },
    Expression {
        sql: String,
        #[serde(default)]
        descending: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Index {
    pub unique: bool,
    pub parts: Vec<IndexPart>,
    /// A partial index's `WHERE` clause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    /// The access method where the engine has several (e.g. `btree`, `gin`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// PostgreSQL `NULLS NOT DISTINCT`: two NULLs collide.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nulls_not_distinct: bool,
}

/// What an object is, tagged by `kind` in JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Detail {
    Namespace {},
    Table {
        /// SQLite `STRICT`.
        #[serde(default)]
        strict: bool,
        /// SQLite `WITHOUT ROWID`.
        #[serde(default)]
        without_rowid: bool,
    },
    Column(Column),
    PrimaryKey {
        columns: Vec<String>,
    },
    Unique {
        columns: Vec<String>,
        /// PostgreSQL `NULLS NOT DISTINCT`: two NULLs collide.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        nulls_not_distinct: bool,
    },
    Check {
        expression: String,
        #[serde(default = "yes")]
        validated: bool,
    },
    ForeignKey(ForeignKey),
    Index(Index),
    View {
        /// The definition as the engine renders it.
        definition: String,
        #[serde(default)]
        materialized: bool,
    },
    Trigger {
        /// e.g. `BEFORE`, `AFTER`, `INSTEAD OF`.
        timing: String,
        /// e.g. `INSERT`, `UPDATE`, `DELETE`.
        events: Vec<String>,
        definition: String,
    },
    Routine {
        /// The identity arguments, e.g. `integer, text`.
        arguments: String,
        definition_sha256: String,
    },
    Enum {
        values: Vec<String>,
    },
    Sequence {
        data_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owned_by: Option<String>,
    },
}

fn yes() -> bool {
    true
}

/// One schema object. `namespace` is the schema (Postgres, SQL Server), the database (MySQL) or `main` (SQLite).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub struct Object {
    pub namespace: String,
    pub name: String,
    /// The table a column, key, constraint, index or trigger belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(flatten)]
    pub detail: Detail,
}

impl Object {
    pub fn kind(&self) -> Kind {
        match self.detail {
            Detail::Namespace {} => Kind::Namespace,
            Detail::Table { .. } => Kind::Table,
            Detail::Column(_) => Kind::Column,
            Detail::PrimaryKey { .. } => Kind::PrimaryKey,
            Detail::Unique { .. } => Kind::Unique,
            Detail::Check { .. } => Kind::Check,
            Detail::ForeignKey(_) => Kind::ForeignKey,
            Detail::Index(_) => Kind::Index,
            Detail::View { .. } => Kind::View,
            Detail::Trigger { .. } => Kind::Trigger,
            Detail::Routine { .. } => Kind::Routine,
            Detail::Enum { .. } => Kind::Enum,
            Detail::Sequence { .. } => Kind::Sequence,
        }
    }

    /// The identity of an object within a snapshot.
    pub fn key(&self) -> (Kind, &str, Option<&str>, &str) {
        (
            self.kind(),
            &self.namespace,
            self.parent.as_deref(),
            &self.name,
        )
    }
}

/// The JSON form of a snapshot, as published in `schema/snapshot.schema.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SnapshotFile {
    /// Always 1 for this version of the format.
    pub format: u32,
    /// e.g. `sqlite`, `postgres`.
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    /// The kinds this reader can see. Every object's kind must be one of them.
    pub capabilities: BTreeSet<Kind>,
    pub objects: Vec<Object>,
}

/// Why a snapshot was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotError(pub String);

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SnapshotError {}

/// A validated schema snapshot: objects sorted by key, every key unique, every kind within the declared capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SnapshotFile", into = "SnapshotFile")]
pub struct Snapshot {
    engine: String,
    engine_version: Option<String>,
    capabilities: BTreeSet<Kind>,
    objects: Vec<Object>,
}

impl Snapshot {
    pub fn new(
        engine: impl Into<String>,
        engine_version: Option<String>,
        capabilities: BTreeSet<Kind>,
        mut objects: Vec<Object>,
    ) -> Result<Snapshot, SnapshotError> {
        let engine = engine.into();
        if engine.is_empty() {
            return Err(SnapshotError("engine is empty".into()));
        }
        for o in &objects {
            let kind = o.kind();
            let what = format!("{kind:?} {}.{}", o.namespace, o.name);
            if o.name.is_empty() || o.namespace.is_empty() {
                return Err(SnapshotError(format!("{what}: empty name or namespace")));
            }
            if !capabilities.contains(&kind) {
                return Err(SnapshotError(format!(
                    "{what}: the reader declared it cannot see {kind:?} objects but produced one"
                )));
            }
            match (kind.has_parent(), &o.parent) {
                (true, None) => return Err(SnapshotError(format!("{what}: needs a parent table"))),
                (false, Some(_)) => {
                    return Err(SnapshotError(format!("{what}: cannot have a parent")));
                }
                (true, Some(p)) if p.is_empty() => {
                    return Err(SnapshotError(format!("{what}: empty parent")));
                }
                _ => {}
            }
        }
        objects.sort_by(|a, b| a.key().cmp(&b.key()));
        if let Some(w) = objects.windows(2).find(|w| w[0].key() == w[1].key()) {
            return Err(SnapshotError(format!("duplicate object {:?}", w[0].key())));
        }
        Ok(Snapshot {
            engine,
            engine_version,
            capabilities,
            objects,
        })
    }

    /// The same snapshot with only the objects `keep` accepts (removing a table and its columns, indexes and triggers
    /// keeps every remaining key unique and every parent present).
    pub fn retain(mut self, keep: impl FnMut(&Object) -> bool) -> Snapshot {
        self.objects.retain(keep);
        self
    }

    /// The engine's own version, as the reader reported it.
    pub fn engine_version(&self) -> Option<&str> {
        self.engine_version.as_deref()
    }

    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// Whether the reader could see this kind. A kind it cannot see is "not checked", never "none".
    pub fn sees(&self, kind: Kind) -> bool {
        self.capabilities.contains(&kind)
    }

    /// All objects, sorted by key.
    pub fn objects(&self) -> &[Object] {
        &self.objects
    }

    pub fn get(
        &self,
        kind: Kind,
        namespace: &str,
        parent: Option<&str>,
        name: &str,
    ) -> Option<&Object> {
        let key = (kind, namespace, parent, name);
        self.objects
            .binary_search_by(|o| o.key().cmp(&key))
            .ok()
            .map(|i| &self.objects[i])
    }
}

impl TryFrom<SnapshotFile> for Snapshot {
    type Error = SnapshotError;

    fn try_from(f: SnapshotFile) -> Result<Snapshot, SnapshotError> {
        if f.format != FORMAT {
            return Err(SnapshotError(format!(
                "snapshot format {} is not supported (this migate reads format {FORMAT})",
                f.format
            )));
        }
        Snapshot::new(f.engine, f.engine_version, f.capabilities, f.objects)
    }
}

impl From<Snapshot> for SnapshotFile {
    fn from(s: Snapshot) -> SnapshotFile {
        SnapshotFile {
            format: FORMAT,
            engine: s.engine,
            engine_version: s.engine_version,
            capabilities: s.capabilities,
            objects: s.objects,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQLITE: &str = include_str!("../fixtures/snapshot-sqlite.json");
    const POSTGRES: &str = include_str!("../fixtures/snapshot-postgres.json");
    const SCHEMA_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../schema/snapshot.schema.json"
    );

    fn table(name: &str) -> Object {
        Object {
            namespace: "main".into(),
            name: name.into(),
            parent: None,
            detail: Detail::Table {
                strict: false,
                without_rowid: false,
            },
        }
    }

    fn column(table: &str, name: &str) -> Object {
        Object {
            namespace: "main".into(),
            name: name.into(),
            parent: Some(table.into()),
            detail: Detail::Column(Column {
                ordinal: 0,
                data_type: DataType {
                    raw: "INTEGER".into(),
                    family: TypeFamily::Integer,
                    length: None,
                    precision: None,
                    scale: None,
                },
                nullable: false,
                default: None,
                generated: None,
                identity: false,
                collation: None,
            }),
        }
    }

    fn caps(kinds: &[Kind]) -> BTreeSet<Kind> {
        kinds.iter().copied().collect()
    }

    #[test]
    fn a_well_formed_snapshot_is_sorted_and_searchable() {
        let s = Snapshot::new(
            "sqlite",
            None,
            caps(&[Kind::Table, Kind::Column]),
            vec![column("users", "id"), table("users"), table("accounts")],
        )
        .unwrap();
        let names: Vec<&str> = s.objects().iter().map(|o| o.name.as_str()).collect();
        assert_eq!(
            names,
            ["accounts", "users", "id"],
            "tables sort before columns, then by name"
        );
        assert!(s.get(Kind::Column, "main", Some("users"), "id").is_some());
        assert!(
            s.get(Kind::Column, "main", Some("accounts"), "id")
                .is_none()
        );
        assert!(s.sees(Kind::Column) && !s.sees(Kind::Trigger));
    }

    #[test]
    fn invalid_snapshots_are_refused() {
        let c = caps(&[Kind::Table, Kind::Column]);
        let refused = |objects: Vec<Object>, caps: BTreeSet<Kind>, why: &str| {
            let err = Snapshot::new("sqlite", None, caps, objects).unwrap_err();
            assert!(err.0.contains(why), "{err} should mention {why}");
        };
        refused(vec![table("t"), table("t")], c.clone(), "duplicate");
        refused(vec![table("t")], caps(&[Kind::Column]), "cannot see Table");
        let mut orphan = column("t", "id");
        orphan.parent = None;
        refused(vec![orphan], c.clone(), "needs a parent");
        let mut parented = table("t");
        parented.parent = Some("x".into());
        refused(vec![parented], c.clone(), "cannot have a parent");
        refused(vec![table("")], c.clone(), "empty name");
        assert!(Snapshot::new("", None, c, vec![]).is_err());
    }

    #[test]
    fn the_wrong_format_version_is_refused() {
        let text = SQLITE.replacen("\"format\": 1", "\"format\": 2", 1);
        let err = serde_json::from_str::<Snapshot>(&text).unwrap_err();
        assert!(
            err.to_string().contains("format 2 is not supported"),
            "{err}"
        );
    }

    #[test]
    fn json_from_a_reader_is_validated_too() {
        // A reader that declares no triggers but emits one is refused on load, not trusted.
        let text = SQLITE.replacen("\"trigger\",", "", 1);
        let err = serde_json::from_str::<Snapshot>(&text).unwrap_err();
        assert!(err.to_string().contains("cannot see Trigger"), "{err}");
    }

    #[test]
    fn a_misspelled_field_from_a_reader_is_refused_not_ignored() {
        // Objects are flattened in JSON; without this, a typo in an optional field would silently drop it.
        for (from, to) in [
            ("\"default\": \"0\"", "\"defualt\": \"0\""), // column
            (
                "\"on_update\": \"NO ACTION\"",
                "\"on_update\": \"NO ACTION\", \"deferable\": true",
            ), // foreign key
            (
                "\"predicate\": \"deleted = 0\"",
                "\"predicat\": \"deleted = 0\"",
            ), // index
            ("\"strict\": false", "\"strikt\": false"),   // table (a struct variant)
        ] {
            assert_eq!(SQLITE.matches(from).count(), 1, "fixture lost {from}");
            let text = SQLITE.replacen(from, to, 1);
            assert!(
                serde_json::from_str::<Snapshot>(&text).is_err(),
                "accepted {to}"
            );
        }
    }

    fn round_trip(text: &str, engine: &str, version: &str, kinds: usize) {
        let first: Snapshot = serde_json::from_str(text).unwrap();
        assert_eq!(
            (first.engine(), first.engine_version()),
            (engine, Some(version))
        );
        let seen: BTreeSet<Kind> = first.objects().iter().map(Object::kind).collect();
        assert_eq!(seen.len(), kinds, "the fixture exercises {kinds} kinds");
        let json = serde_json::to_string_pretty(&first).unwrap();
        let second: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(first, second, "JSON round trip");
        assert_eq!(
            json,
            serde_json::to_string_pretty(&second).unwrap(),
            "stable output"
        );
    }

    #[test]
    fn the_sqlite_fixture_round_trips() {
        round_trip(SQLITE, "sqlite", "3.53.4", 9);
    }

    #[test]
    fn the_postgres_fixture_round_trips() {
        round_trip(POSTGRES, "postgres", "18.6", 13);
    }

    fn generated_schema() -> String {
        let schema = schemars::schema_for!(SnapshotFile);
        serde_json::to_string_pretty(&schema).unwrap() + "\n"
    }

    /// The published schema must match the types. To update it after changing them:
    /// `MIGATE_BLESS=1 cargo test -p migate-core published_schema`.
    #[test]
    fn the_published_schema_matches_the_types() {
        let generated = generated_schema();
        if std::env::var_os("MIGATE_BLESS").is_some() {
            std::fs::write(SCHEMA_PATH, &generated).unwrap();
        }
        let published = std::fs::read_to_string(SCHEMA_PATH).unwrap_or_default();
        assert!(
            published == generated,
            "schema/snapshot.schema.json is out of date: run MIGATE_BLESS=1 cargo test -p migate-core published_schema"
        );
    }

    #[test]
    fn the_fixtures_validate_against_the_published_schema_and_a_broken_one_does_not() {
        let schema: serde_json::Value = serde_json::from_str(&generated_schema()).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for (name, text) in [("sqlite", SQLITE), ("postgres", POSTGRES)] {
            let instance: serde_json::Value = serde_json::from_str(text).unwrap();
            let errors: Vec<String> = validator
                .iter_errors(&instance)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "{name}: {errors:?}");
        }
        // Calibration: the validator must reject an object of an unknown kind.
        let broken: serde_json::Value =
            serde_json::from_str(&SQLITE.replacen("\"kind\": \"table\"", "\"kind\": \"tabel\"", 1))
                .unwrap();
        assert!(
            !validator.is_valid(&broken),
            "the schema validator accepted an unknown kind"
        );
    }
}
