//! Core types and error types for the HATCH multi-agent orchestration framework.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

mod error;
mod types;

pub use error::{HatchError, Result};
pub use types::{
    AgentOutput, Artifact, ArtifactKind, ExecutionPlan, Task, TaskSpec,
};

/// Unique identifier for an agent instance.
pub type AgentId = uuid::Uuid;
/// Unique identifier for a task within an execution plan.
pub type TaskId = uuid::Uuid;
/// Unique identifier for a full orchestration run.
pub type RunId = uuid::Uuid;
