//! LLM provider trait and concrete backends (OpenAI, Ollama).

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

mod message;
mod ollama;
mod openai;
mod providers;
mod stream;
mod types;

pub use message::{Message, MessageRole};
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use providers::{llm_from_env, LlmProviderKind};
pub use stream::CompletionStream;
pub use types::{CompletionRequest, CompletionResponse};

pub use async_trait::async_trait;

use std::pin::Pin;
use std::sync::Arc;

use hatch_core::Result;

/// Async LLM completion API used by planner and agents.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Returns a single completion string from the provider.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;

    /// Returns a stream of completion fragments (provider-specific chunking).
    async fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<CompletionStream>>>;
}

/// Shared pointer to a configured [`LlmProvider`].
pub type SharedLlm = Arc<dyn LlmProvider>;
