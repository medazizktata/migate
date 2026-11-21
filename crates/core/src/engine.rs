//! The engine port: what a run needs from a database engine. Engines (SQLite, Postgres, later others) plug in by
//! implementing these traits; the runner never names one. Declarations only: the I/O lives in each engine crate.

use std::collections::BTreeMap;

use crate::Finding;
use crate::schema::Snapshot;

/// One throwaway database for one run. Dropping it removes the database (and stops its server, if it has one).
pub trait Sandbox {
    /// Placeholders the project's commands may use, by name: `db` for a database file, `url` for a server.
    fn placeholders(&self) -> Vec<(&'static str, String)>;

    /// The database the migrate command wrote, or why there is none (it most likely wrote somewhere else). `history`
    /// is the migrator's history query, for engines that must pick the database among several candidates.
    fn open(&self, history: &str) -> Result<Box<dyn Database + '_>, String>;

    /// Syntax in a migration's `sql` that the engine version production runs (`target`, from `target_version`)
    /// rejects: (line, feature, the first version that runs it). `None` for an engine that runs the migrations on
    /// that very version instead of reading them, so no migration needs scanning. An error when `target` names no
    /// version of this engine.
    fn newer_syntax(&self, sql: &str, target: &str) -> Result<Option<Syntax>, String>;
}

/// Syntax a target engine version rejects: (line, feature, the first version that runs it).
pub type Syntax = Vec<(u32, String, String)>;

/// What re-writing the rows a table holds, each unchanged and under its own id, shows about one of its triggers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerProbe {
    /// `refused` of the `checked` rows (out of `held`) were refused; `message` is the first refusal.
    Refused {
        refused: u64,
        checked: u64,
        held: u64,
        message: String,
    },
    /// None of the `checked` rows (out of `held`) was refused.
    Accepted { checked: u64, held: u64 },
    /// The table holds no row to judge it by.
    NoRows,
    /// The rows could not judge it, and why.
    Unknown(String),
}

/// The migrated database, as the run reads and changes it.
pub trait Database {
    fn snapshot(&self) -> Result<Snapshot, String>;
    /// How many rows `sql` returns (the migrator's history query).
    fn count_rows(&self, sql: &str) -> Result<usize, String>;
    /// Every table's row count, by `namespace.table`.
    fn row_counts(&self) -> Result<BTreeMap<String, u64>, String>;
    /// Every column's count of non-NULL values, by `namespace.table.column`: what a migration can blank without losing
    /// a row.
    fn value_counts(&self) -> Result<BTreeMap<String, u64>, String>;
    /// Why this reader may misread the database, if it might: the database was last written by a newer engine
    /// version than the one reading it.
    fn newer_writer(&self) -> Option<String>;
    /// When the migrations ran on a different engine version than production's (`target`), what that leaves
    /// unchecked. Read right after the migrations, as [`Database::newer_writer`] is.
    fn target_gap(&self, target: &str) -> Result<Option<Finding>, String>;
    /// Whether `trigger` on `table` would refuse any row the table holds if it were written again. Never writes to the
    /// database itself.
    fn probe_trigger(&self, table: &str, trigger: &str) -> Result<TriggerProbe, String>;
    /// Every object the schema model does not cover, as `category name` -> its definition text, so a change to one is
    /// reported as not checked rather than passing in silence.
    fn unmodelled(&self) -> Result<BTreeMap<String, String>, String>;
    /// Drop every table and view not named in `keep` (a query, such as the history query). Throwaway databases only.
    fn drop_all_but(&self, keep: &str) -> Result<(), String>;
}
