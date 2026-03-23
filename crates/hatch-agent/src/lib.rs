//! Agent abstractions and the default generic agent implementation.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

mod agent;
mod context;
mod generic;

pub use agent::Agent;
pub use context::AgentContext;
pub use generic::GenericAgent;

pub use async_trait::async_trait;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hatch_bus::MessageBus;
    use hatch_core::{RunId, Task, TaskId};
    use hatch_llm::LlmProvider;

    use crate::AgentContext;

    struct NoopLlm;

    #[async_trait::async_trait]
    impl LlmProvider for NoopLlm {
        async fn complete(
            &self,
            _req: hatch_llm::CompletionRequest,
        ) -> hatch_core::Result<hatch_llm::CompletionResponse> {
            Ok(hatch_llm::CompletionResponse {
                content: "ok".into(),
                model: None,
            })
        }

        async fn complete_stream(
            &self,
            _req: hatch_llm::CompletionRequest,
        ) -> hatch_core::Result<std::pin::Pin<Box<hatch_llm::CompletionStream>>> {
            Err(hatch_core::HatchError::Llm("noop stream".into()))
        }
    }

    #[tokio::test]
    async fn agent_context_holds_expected_fields() {
        let task = Task {
            id: TaskId::new_v4(),
            name: "t".into(),
            description: "d".into(),
            agent_type: "generic".into(),
            dependencies: vec![],
        };
        let run_id = RunId::new_v4();
        let llm: std::sync::Arc<dyn LlmProvider> = std::sync::Arc::new(NoopLlm);
        let bus = Arc::new(MessageBus::new(16));
        let ctx = AgentContext {
            task,
            run_id,
            llm,
            bus,
            system_prompt: "sys".into(),
            model: "test-model".into(),
        };
        assert_eq!(ctx.system_prompt, "sys");
        assert_eq!(ctx.model, "test-model");
        assert_eq!(ctx.run_id, run_id);
        assert_eq!(ctx.task.agent_type, "generic");
    }
}
