//! Agent spawner: load templates and run generic agents as Tokio tasks.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

mod spawner;
mod template;

pub use spawner::Spawner;
pub use template::{load_templates_from_dir, AgentTemplate};
