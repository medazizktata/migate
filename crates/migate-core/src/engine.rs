//! The engine port: what a run needs from a database engine. Engines (SQLite, Postgres, later others) plug in by
//! implementing these traits; the runner never names one. Declarations only: the I/O lives in each engine crate.

use std::collections::BTreeMap;

use crate::schema::Snapshot;

/// One throwaway database for one run. Dropping it removes the database (and stops its server, if it has one).
pub trait Sandbox {
    /// Placeholders the project's commands may use, by name: `db` for a database file, `url` for a server.
    fn placeholders(&self) -> Vec<(&'static str, String)>;

    /// The database the migrate command wrote, or why there is none (it most likely wrote somewhere else). `history`
    /// is the migrator's history query, for engines that must pick the database among several candidates.
    fn open(&self, history: &str) -> Result<Box<dyn Database + '_>, String>;
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
    /// Every object the schema model does not cover, as `category name` -> its definition text, so a change to one is
    /// reported as not checked rather than passing in silence.
    fn unmodelled(&self) -> Result<BTreeMap<String, String>, String>;
    /// Drop every table and view not named in `keep` (a query, such as the history query). Throwaway databases only.
    fn drop_all_but(&self, keep: &str) -> Result<(), String>;
}
