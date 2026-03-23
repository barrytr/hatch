//! Supervisor: collect agent results from the bus and produce a merged run result.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

mod supervisor;

pub use supervisor::{RunResult, Supervisor};
