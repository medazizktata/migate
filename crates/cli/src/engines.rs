//! The composition root for engines: the one module that names engine crates. The runner (`sandbox.rs`) works
//! through the `engine` port only, so an engine is added here and in the config, not in the runner.

use std::path::Path;
use std::process::{Child, Command};

use migate_core::engine;
use migate_migrators::tools;

use crate::config::{Engine, Sandbox};

/// The throwaway database for one run, in `dir`: a file for SQLite (`database` is where the project's tool writes
/// it, when that is not the file migate names), a server for PostgreSQL, started through `spawn`.
pub fn start(
    cfg: &Sandbox,
    dir: &Path,
    database: Option<String>,
    spawn: &mut dyn FnMut(&mut Command) -> std::io::Result<Child>,
) -> Result<Box<dyn engine::Sandbox>, String> {
    Ok(match cfg.engine {
        Engine::Sqlite => Box::new(migate_engine_sqlite::Throwaway::new(
            dir.join("sandbox.db"),
            database,
        )),
        Engine::Postgres => Box::new(migate_engine_postgres::Server::start(
            dir,
            cfg.target_version.as_deref(),
            &tools().postgres_bin_dirs,
            spawn,
        )?),
    })
}

/// Stop what a killed run's database left running in `dir` (the folder `start` was given), before the folder goes:
/// a PostgreSQL server outlives the run that started it; a SQLite file leaves nothing running.
pub fn stop_left_behind(dir: &Path) {
    migate_engine_postgres::stop_left_server(dir);
}
