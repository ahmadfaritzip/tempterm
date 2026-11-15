//! Process supervisor and process definitions for temp-terminal.
//! Corresponds to Go `tempterm/internal/procs`.

pub mod def;
pub mod file;
pub mod production_runner;
pub mod ready;
pub mod restart;
pub mod supervisor;
pub mod trust;
pub mod watch;
