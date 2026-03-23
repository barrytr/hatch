use std::sync::Arc;

use hatch_core::{HatchError, Result};

use crate::{OllamaProvider, OpenAiProvider};
use crate::SharedLlm;

/// Which backend to construct from environment defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProviderKind {
    /// OpenAI HTTPS API (`api.openai.com`).
    Openai,
    /// Local Ollama HTTP API (default `http://127.0.0.1:11434`).
    Ollama,
}

impl LlmProviderKind {
    /// Parses `openai` or `ollama` (case-insensitive).
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "openai" => Ok(Self::Openai),
            "ollama" => Ok(Self::Ollama),
            other => Err(HatchError::Config(format!(
                "unknown LLM provider '{other}', expected openai|ollama"
            ))),
        }
    }
}

/// Reads `HATCH_DEFAULT_PROVIDER` (default `openai`) and builds a shared provider.
pub fn llm_from_env() -> Result<SharedLlm> {
    let raw = std::env::var("HATCH_DEFAULT_PROVIDER").unwrap_or_else(|_| "openai".into());
    let kind = LlmProviderKind::parse(&raw)?;
    match kind {
        LlmProviderKind::Openai => Ok(Arc::new(OpenAiProvider::from_env()?)),
        LlmProviderKind::Ollama => Ok(Arc::new(OllamaProvider::new()?)),
    }
}
