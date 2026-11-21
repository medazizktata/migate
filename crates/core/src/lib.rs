//! migate's core: the parts that know no database, host or migrator.
//!
//! Everything here is a pure function over data; I/O lives in adapter crates (see
//! `docs/research/architecture.md`, sections 0 and 3).

pub mod contract;
pub mod diff;
pub mod engine;
pub mod finding;
pub mod hash;
pub mod history;
pub mod ledger;
pub mod order;
pub mod rules;
pub mod schema;

pub use finding::{Finding, Severity};
