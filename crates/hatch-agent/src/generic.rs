use async_trait::async_trait;
use hatch_bus::HatchMessage;
use hatch_core::{AgentId, AgentOutput, Artifact, ArtifactKind, HatchError, Result};
use hatch_llm::{CompletionRequest, Message, MessageRole};
use tracing::{info, instrument};

use crate::agent::Agent;
use crate::context::AgentContext;

/// Default agent that delegates work to an LLM using a configured system prompt.
#[derive(Debug)]
pub struct GenericAgent {
    id: AgentId,
    name: String,
    agent_type: String,
}

impl GenericAgent {
    /// Creates a new generic agent instance.
    pub fn new(id: AgentId, name: impl Into<String>, agent_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            agent_type: agent_type.into(),
        }
    }
}

#[async_trait]
impl Agent for GenericAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn agent_type(&self) -> &str {
        &self.agent_type
    }

    #[instrument(skip(ctx), fields(agent_id = %self.id, task_id = %ctx.task.id))]
    async fn run(&self, ctx: AgentContext) -> Result<AgentOutput> {
        const FILE_JSON_SYSTEM_INSTRUCTIONS: &str = r#"Output format requirements (IMPORTANT):
- You MUST respond with ONLY valid JSON (no markdown).
- JSON schema:
  {
    "content": "<short human summary of what you produced>",
    "artifacts": [
      {
        "name": "<relative file path, e.g. frontend/src/App.tsx or package.json>",
        "content": "<exact file text>",
        "kind": "code" | "config" | "markdown" | "other"
      }
    ]
  }
- Ensure every file path is relative (no leading `/`).
- Put as many files as needed to make the project runnable.
- If you cannot produce code, still output a JSON with at least 1 artifact containing an explanation."#;

        let system = format!("{}\n\n{}", ctx.system_prompt, FILE_JSON_SYSTEM_INSTRUCTIONS);
        let user_prompt = format!(
            "Task: {}\n\nDescription:\n{}\n\nGenerate the full set of files.\nRemember: ONLY valid JSON.",
            ctx.task.name, ctx.task.description
        );

        let _ = ctx.bus.publish(HatchMessage::AgentProgress {
            agent_id: self.id,
            message: format!("starting {}", ctx.task.name),
        });

        let req = CompletionRequest {
            model: ctx.model.clone(),
            messages: vec![Message {
                role: MessageRole::User,
                content: user_prompt,
            }],
            max_tokens: Some(4096),
            temperature: Some(0.2),
            system: Some(system),
        };

        info!(target: "hatch_agent", "generic agent calling llm");
        let resp = ctx.llm.complete(req).await?;
        let raw = resp.content.trim();
        if raw.is_empty() {
            return Err(HatchError::Llm("empty completion from provider".into()));
        }

        let parsed = try_parse_agent_json(raw).ok();

        let output = if let Some(parsed) = parsed {
            let artifacts = parsed
                .artifacts
                .into_iter()
                .map(|a| Artifact {
                    name: a.name,
                    content: a.content,
                    kind: parse_kind(&a.kind),
                })
                .collect::<Vec<_>>();

            AgentOutput {
                agent_id: self.id,
                task_id: ctx.task.id,
                content: parsed.content,
                artifacts,
            }
        } else {
            // Fallback: keep prior behavior if a model ignores the strict JSON constraint.
            AgentOutput {
                agent_id: self.id,
                task_id: ctx.task.id,
                content: resp.content.clone(),
                artifacts: vec![Artifact {
                    name: format!("{}_response.md", ctx.task.name.replace(' ', "_")),
                    content: resp.content,
                    kind: ArtifactKind::Markdown,
                }],
            }
        };

        let _ = ctx.bus.publish(HatchMessage::AgentProgress {
            agent_id: self.id,
            message: format!("finished {}", ctx.task.name),
        });

        Ok(output)
    }
}

#[derive(Debug, serde::Deserialize)]
struct AgentJsonArtifact {
    name: String,
    content: String,
    #[serde(default)]
    kind: String,
}

#[derive(Debug, serde::Deserialize)]
struct AgentJsonOutput {
    content: String,
    artifacts: Vec<AgentJsonArtifact>,
}

fn strip_json_fences(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix("```") {
        let rest = inner
            .trim_start_matches("json")
            .trim_start_matches("JSON")
            .trim_start();
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim();
        }
        return rest.trim();
    }
    trimmed
}

fn try_parse_agent_json(raw: &str) -> std::result::Result<AgentJsonOutput, serde_json::Error> {
    let json_str = strip_json_fences(raw);
    serde_json::from_str::<AgentJsonOutput>(json_str)
}

fn parse_kind(kind: &str) -> ArtifactKind {
    match kind.to_ascii_lowercase().as_str() {
        "code" => ArtifactKind::Code,
        "config" => ArtifactKind::Config,
        "markdown" => ArtifactKind::Markdown,
        _ => ArtifactKind::Other,
    }
}
