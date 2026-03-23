use hatch_core::{AgentId, AgentOutput, ExecutionPlan, RunId, TaskId};
use serde::{Deserialize, Serialize};

/// Events exchanged across planner, agents, spawner, and supervisor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HatchMessage {
    /// Planner produced a concrete [`ExecutionPlan`].
    PlanReady(ExecutionPlan),
    /// An agent task has started executing.
    AgentStarted {
        /// Agent instance identifier.
        agent_id: AgentId,
        /// Task being executed.
        task_id: TaskId,
    },
    /// Non-terminal progress update from an agent.
    AgentProgress {
        /// Agent reporting progress.
        agent_id: AgentId,
        /// Human-readable progress line.
        message: String,
    },
    /// Agent finished successfully with output.
    AgentDone(AgentOutput),
    /// Agent failed with an error string.
    AgentFailed {
        /// Agent that failed.
        agent_id: AgentId,
        /// Error description.
        error: String,
    },
    /// Entire run finished with merged outputs.
    RunComplete {
        /// Run identifier.
        run_id: RunId,
        /// Collected successful outputs.
        outputs: Vec<AgentOutput>,
    },
}
