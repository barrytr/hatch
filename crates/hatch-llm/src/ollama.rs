use std::pin::Pin;

use async_trait::async_trait;
use futures_util::{stream, StreamExt};
use hatch_core::{HatchError, Result};
use serde::Deserialize;
use serde_json::json;
use tracing::debug;

use crate::message::MessageRole;
use crate::stream::CompletionStream;
use crate::types::{CompletionRequest, CompletionResponse};
use crate::LlmProvider;

/// Default Ollama API base URL.
pub const DEFAULT_OLLAMA_BASE: &str = "http://127.0.0.1:11434";

/// Ollama local inference API client.
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    /// Creates a client targeting `DEFAULT_OLLAMA_BASE`.
    pub fn new() -> Result<Self> {
        Self::with_base(DEFAULT_OLLAMA_BASE)
    }

    /// Creates a client with a custom base (e.g. `http://host:11434`).
    pub fn with_base(base_url: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .build()
                .map_err(|e| HatchError::Http(e.to_string()))?,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.base_url)
    }

    fn build_messages(req: &CompletionRequest) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        if let Some(sys) = &req.system {
            out.push(json!({"role": "system", "content": sys}));
        }
        for m in &req.messages {
            let role = match m.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            };
            out.push(json!({"role": role, "content": m.content}));
        }
        out
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: DEFAULT_OLLAMA_BASE.to_string(),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let url = self.chat_url();
        let body = json!({
            "model": req.model,
            "messages": Self::build_messages(&req),
            "stream": false,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            }
        });

        debug!(target: "hatch_llm", "Ollama complete model={} url={}", req.model, url);

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| HatchError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(HatchError::Llm(format!("Ollama HTTP {status}: {text}")));
        }

        let v: OllamaChatResponse = resp
            .json()
            .await
            .map_err(|e| HatchError::Http(e.to_string()))?;

        let content = v.message.content;

        Ok(CompletionResponse {
            content,
            model: Some(req.model),
        })
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<CompletionStream>>> {
        let url = self.chat_url();
        let body = json!({
            "model": req.model,
            "messages": Self::build_messages(&req),
            "stream": true,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            }
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| HatchError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(HatchError::Llm(format!("Ollama HTTP {status}: {text}")));
        }

        let byte_stream = resp.bytes_stream();
        let string_stream = byte_stream.flat_map(|chunk_result| {
            stream::iter(match chunk_result {
                Ok(bytes) => vec![Ok(String::from_utf8_lossy(&bytes).to_string())],
                Err(e) => vec![Err(HatchError::Http(e.to_string()))],
            })
        });

        let parsed = OllamaNdjsonStream {
            line_buffer: String::new(),
            upstream: Box::pin(string_stream),
        };

        Ok(Box::pin(CompletionStream::new(parsed)))
    }
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaMsg,
}

#[derive(Debug, Deserialize)]
struct OllamaMsg {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamLine {
    message: Option<OllamaMsg>,
}

struct OllamaNdjsonStream<S> {
    line_buffer: String,
    upstream: Pin<Box<S>>,
}

impl<S> futures_util::Stream for OllamaNdjsonStream<S>
where
    S: futures_util::Stream<Item = Result<String>> + Send + Unpin,
{
    type Item = Result<String>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        loop {
            while let Some(pos) = this.line_buffer.find('\n') {
                let line = this.line_buffer[..pos].to_string();
                this.line_buffer.drain(..=pos);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(ev) = serde_json::from_str::<OllamaStreamLine>(line) {
                    if let Some(m) = ev.message {
                        if !m.content.is_empty() {
                            return std::task::Poll::Ready(Some(Ok(m.content)));
                        }
                    }
                }
            }

            match Pin::new(&mut this.upstream).poll_next(cx) {
                std::task::Poll::Ready(Some(Ok(piece))) => {
                    this.line_buffer.push_str(&piece);
                }
                std::task::Poll::Ready(Some(Err(e))) => {
                    return std::task::Poll::Ready(Some(Err(e)));
                }
                std::task::Poll::Ready(None) => {
                    let rest = this.line_buffer.trim();
                    if rest.is_empty() {
                        return std::task::Poll::Ready(None);
                    }
                    if let Ok(ev) = serde_json::from_str::<OllamaStreamLine>(rest) {
                        if let Some(m) = ev.message {
                            if !m.content.is_empty() {
                                this.line_buffer.clear();
                                return std::task::Poll::Ready(Some(Ok(m.content)));
                            }
                        }
                    }
                    this.line_buffer.clear();
                    return std::task::Poll::Ready(None);
                }
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}
