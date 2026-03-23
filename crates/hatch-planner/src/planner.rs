use std::collections::HashMap;

use hatch_core::{ExecutionPlan, HatchError, Result, RunId, Task, TaskId, TaskSpec};
use hatch_llm::{CompletionRequest, Message, MessageRole, SharedLlm};
use serde::Deserialize;
use tracing::{debug, instrument};

/// Meta-agent that decomposes a natural language intent into an [`ExecutionPlan`].
pub struct Planner {
    llm: SharedLlm,
    model: String,
}

impl Planner {
    /// Creates a planner with the given model name.
    pub fn new(llm: SharedLlm, model: impl Into<String>) -> Self {
        Self {
            llm,
            model: model.into(),
        }
    }

    /// Calls the LLM and parses a JSON plan body into tasks with stable IDs.
    #[instrument(skip(self, intent), fields(model = %self.model))]
    pub async fn plan(&self, intent: &str) -> Result<ExecutionPlan> {
        let run_id = RunId::new_v4();
        let user = format!(
            "User intent:\n{intent}\n\nReturn ONLY valid JSON with this shape (no markdown):\n{{\"tasks\":[{{\"name\":\"...\",\"description\":\"...\",\"agent_type\":\"frontend|backend|devops|data|generic\",\"dependencies\":[]}}]}}"
        );

        let req = CompletionRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: MessageRole::User,
                content: user,
            }],
            max_tokens: Some(2048),
            temperature: Some(0.0),
            system: Some(PLANNER_SYSTEM.to_string()),
        };

        debug!(target: "hatch_planner", "requesting plan from llm");
        let resp = self.llm.complete(req).await?;
        let text = strip_json_fences(&resp.content);
        parse_execution_plan_from_llm_json(run_id, intent, &text)
    }
}

/// System prompt sent to the LLM when building a plan.
pub const PLANNER_SYSTEM: &str = r#"You are the HATCH planner meta-agent.
Respond with ONLY a single JSON object (no markdown fences, no commentary).
Schema:
{
  "tasks": [
    {
      "name": "short title",
      "description": "what the sub-agent should do",
      "agent_type": "frontend|backend|devops|data|generic",
      "dependencies": []
    }
  ]
}
Use dependency task names in "dependencies" when order matters; otherwise use [].
# Integration constraints (IMPORTANT):
- Prefer a typical fullstack layout with top-level folders: `frontend/` and `backend/` when you have both roles.
- The backend must expose a REST API under `/api/*` and be runnable with `npm run dev` or `npm run start` inside `backend/`.
- The frontend must call the backend API (prefer `VITE_API_URL` / `REACT_APP_API_URL` env vars, otherwise use relative `/api` paths).
"#;

#[derive(Debug, Deserialize)]
struct LlmPlanBody {
    tasks: Vec<TaskSpec>,
}

fn strip_json_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(inner) = trimmed.strip_prefix("```") {
        let rest = inner
            .trim_start_matches("json")
            .trim_start_matches("JSON")
            .trim_start();
        if let Some(end) = rest.rfind("```") {
            return rest[..end].trim().to_string();
        }
        return rest.trim().to_string();
    }
    trimmed.to_string()
}

/// Parses planner JSON into an [`ExecutionPlan`] (exposed for unit tests).
pub fn parse_execution_plan_from_llm_json(
    run_id: RunId,
    intent: &str,
    json: &str,
) -> Result<ExecutionPlan> {
    let json = strip_json_fences(json);
    let body: LlmPlanBody = serde_json::from_str(&json).map_err(|e| {
        HatchError::Planner(format!("invalid planner json: {e}; body: {json:?}"))
    })?;

    if body.tasks.is_empty() {
        return Err(HatchError::Planner(
            "planner returned zero tasks".to_string(),
        ));
    }

    let mut name_to_id: HashMap<String, TaskId> = HashMap::new();
    for spec in &body.tasks {
        let id = TaskId::new_v4();
        name_to_id.insert(spec.name.clone(), id);
    }

    let mut tasks = Vec::new();
    for spec in body.tasks {
        let id = *name_to_id
            .get(&spec.name)
            .ok_or_else(|| HatchError::Planner("internal id map failure".into()))?;

        let mut deps = Vec::new();
        for dep_name in spec.dependencies {
            let dep_id = name_to_id.get(&dep_name).ok_or_else(|| {
                HatchError::Planner(format!(
                    "unknown dependency '{dep_name}' for task '{}'",
                    spec.name
                ))
            })?;
            deps.push(*dep_id);
        }

        tasks.push(Task {
            id,
            name: spec.name,
            description: spec.description,
            agent_type: spec.agent_type,
            dependencies: deps,
        });
    }

    Ok(ExecutionPlan {
        run_id,
        intent: intent.to_string(),
        tasks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_plan_json() {
        let run = RunId::new_v4();
        let json = r#"{"tasks":[{"name":"A","description":"do a","agent_type":"frontend","dependencies":[]},{"name":"B","description":"do b","agent_type":"backend","dependencies":["A"]}]}"#;
        let plan = parse_execution_plan_from_llm_json(run, "test intent", json)
            .expect("parse");
        assert_eq!(plan.intent, "test intent");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[1].dependencies.len(), 1);
        assert_eq!(plan.tasks[1].dependencies[0], plan.tasks[0].id);
    }

    #[test]
    fn strips_markdown_fence() {
        let run = RunId::new_v4();
        let json = "```json\n{\"tasks\":[{\"name\":\"A\",\"description\":\"d\",\"agent_type\":\"generic\",\"dependencies\":[]}]}\n```";
        let plan = parse_execution_plan_from_llm_json(run, "x", json).expect("parse");
        assert_eq!(plan.tasks.len(), 1);
    }
}
