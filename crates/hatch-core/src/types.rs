use serde::{Deserialize, Serialize};

use crate::{AgentId, RunId, TaskId};

/// Kind of artifact produced by an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Source or script code.
    Code,
    /// Configuration files.
    Config,
    /// Markdown documentation.
    Markdown,
    /// Other textual content.
    Other,
}

/// Named output artifact with typed kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Logical name (e.g. filename).
    pub name: String,
    /// Raw text content.
    pub content: String,
    /// Classification of the artifact.
    pub kind: ArtifactKind,
}

/// Result returned from a single agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    /// Agent instance that produced this output.
    pub agent_id: AgentId,
    /// Task this output corresponds to.
    pub task_id: TaskId,
    /// Primary textual response.
    pub content: String,
    /// Structured artifacts extracted or attached by the agent.
    pub artifacts: Vec<Artifact>,
}

/// A task to execute as part of a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Stable identifier for dependency edges.
    pub id: TaskId,
    /// Short task title.
    pub name: String,
    /// Detailed instructions for the agent.
    pub description: String,
    /// Template key, e.g. `frontend`, `backend`.
    pub agent_type: String,
    /// Tasks that must complete before this one starts.
    pub dependencies: Vec<TaskId>,
}

/// Intermediate shape returned by the planner LLM (no task IDs yet).
#[derive(Debug, Clone, Deserialize)]
pub struct TaskSpec {
    /// Short task title.
    pub name: String,
    /// Detailed instructions for the agent.
    pub description: String,
    /// Template key, e.g. `frontend`, `backend`.
    pub agent_type: String,
    /// Names of tasks that must complete first (resolved to [`TaskId`] by the planner).
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Full execution plan for a user intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// Run identifier.
    pub run_id: RunId,
    /// Original user intent.
    pub intent: String,
    /// Ordered task graph.
    pub tasks: Vec<Task>,
}
