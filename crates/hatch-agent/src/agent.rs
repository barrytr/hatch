use async_trait::async_trait;
use hatch_core::{AgentId, AgentOutput, Result};

use crate::AgentContext;

/// Executable agent contract implemented by concrete agent types.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Stable instance identifier.
    fn id(&self) -> AgentId;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Template key (e.g. `frontend`).
    fn agent_type(&self) -> &str;

    /// Executes the assigned task using the provided [`AgentContext`].
    async fn run(&self, ctx: AgentContext) -> Result<AgentOutput>;
}
