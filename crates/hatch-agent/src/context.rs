use std::sync::Arc;

use hatch_bus::MessageBus;
use hatch_core::{RunId, Task};
use hatch_llm::SharedLlm;

/// Runtime inputs passed to an [`Agent::run`](crate::Agent) invocation.
pub struct AgentContext {
    /// Task slice from the execution plan.
    pub task: Task,
    /// Parent run identifier.
    pub run_id: RunId,
    /// Shared LLM client.
    pub llm: SharedLlm,
    /// Bus for publishing lifecycle events.
    pub bus: Arc<MessageBus>,
    /// Effective system prompt (from template or override).
    pub system_prompt: String,
    /// Model name to pass to the LLM provider.
    pub model: String,
}
