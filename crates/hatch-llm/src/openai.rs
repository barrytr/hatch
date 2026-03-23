use std::pin::Pin;

use async_trait::async_trait;
use futures_util::{stream, StreamExt};
use hatch_core::{HatchError, Result};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, error};

use crate::message::MessageRole;
use crate::stream::CompletionStream;
use crate::types::{CompletionRequest, CompletionResponse};
use crate::LlmProvider;

const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";

/// OpenAI Chat Completions API client.
pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
}

impl OpenAiProvider {
    /// Builds a provider using `OPENAI_API_KEY` from the environment.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
            HatchError::Config("OPENAI_API_KEY is not set".to_string())
        })?;
        Ok(Self {
            client: reqwest::Client::builder()
                .build()
                .map_err(|e| HatchError::Http(e.to_string()))?,
            api_key,
        })
    }

    /// Creates a provider with an explicit API key (tests or injected config).
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .build()
                .map_err(|e| HatchError::Http(e.to_string()))?,
            api_key: api_key.into(),
        })
    }

    fn headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        let auth = format!("Bearer {}", self.api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth).map_err(|e| HatchError::Http(e.to_string()))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
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

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let body = json!({
            "model": req.model,
            "messages": Self::build_messages(&req),
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "stream": false,
        });

        debug!(target: "hatch_llm", "OpenAI complete model={}", req.model);

        let resp = self
            .client
            .post(OPENAI_URL)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await
            .map_err(|e| HatchError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            error!(target: "hatch_llm", %status, body = %text, "OpenAI error response");
            return Err(HatchError::Llm(format!("OpenAI HTTP {status}: {text}")));
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| HatchError::Http(e.to_string()))?;

        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| HatchError::Llm("OpenAI missing choices[0].message.content".into()))?
            .to_string();

        let model = v["model"].as_str().map(str::to_string);

        Ok(CompletionResponse { content, model })
    }

    async fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<CompletionStream>>> {
        let body = json!({
            "model": req.model,
            "messages": Self::build_messages(&req),
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
            "stream": true,
        });

        let resp = self
            .client
            .post(OPENAI_URL)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await
            .map_err(|e| HatchError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(HatchError::Llm(format!("OpenAI HTTP {status}: {text}")));
        }

        let byte_stream = resp.bytes_stream();
        let out = byte_stream.flat_map(|chunk_result| {
            stream::iter(match chunk_result {
                Ok(bytes) => {
                    let s = String::from_utf8_lossy(&bytes).to_string();
                    vec![Ok(s)]
                }
                Err(e) => vec![Err(HatchError::Http(e.to_string()))],
            })
        });

        let parsed = OpenAiSseStream {
            buffer: String::new(),
            upstream: Box::pin(out),
        };

        Ok(Box::pin(CompletionStream::new(parsed)))
    }
}

struct OpenAiSseStream<S> {
    buffer: String,
    upstream: Pin<Box<S>>,
}

impl<S> futures_util::Stream for OpenAiSseStream<S>
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
            if let Some(out) = try_emit_openai_delta(&mut this.buffer) {
                return std::task::Poll::Ready(Some(Ok(out)));
            }

            match Pin::new(&mut this.upstream).poll_next(cx) {
                std::task::Poll::Ready(Some(Ok(piece))) => {
                    this.buffer.push_str(&piece);
                }
                std::task::Poll::Ready(Some(Err(e))) => {
                    return std::task::Poll::Ready(Some(Err(e)));
                }
                std::task::Poll::Ready(None) => {
                    if let Some(out) = try_emit_openai_delta(&mut this.buffer) {
                        return std::task::Poll::Ready(Some(Ok(out)));
                    }
                    return std::task::Poll::Ready(None);
                }
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}

fn try_emit_openai_delta(buffer: &mut String) -> Option<String> {
    while let Some(idx) = buffer.find("\n\n") {
        let block: String = buffer[..idx].to_string();
        buffer.drain(..=idx + 1);
        for line in block.lines() {
            let line = line.trim();
            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }
            let line = line.strip_prefix("data:").unwrap_or(line).trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(ev) = serde_json::from_str::<OpenAiStreamEvent>(line) {
                if let Some(delta) = ev
                    .choices
                    .and_then(|c| c.into_iter().next())
                    .and_then(|c| c.delta)
                    .and_then(|d| d.content)
                {
                    if !delta.is_empty() {
                        return Some(delta);
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamEvent {
    choices: Option<Vec<OpenAiStreamChoice>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: Option<OpenAiDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
}
