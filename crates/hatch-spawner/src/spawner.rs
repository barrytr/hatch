use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use hatch_agent::{Agent, AgentContext, GenericAgent};
use hatch_bus::{HatchMessage, MessageBus};
use hatch_core::{AgentId, ExecutionPlan, HatchError, Result};
use hatch_llm::SharedLlm;
use tokio::task::JoinHandle;
use tracing::{error, info, instrument};

use crate::template::{load_templates_from_dir, AgentTemplate};

/// Spawns [`GenericAgent`] tasks for each node in a plan.
pub struct Spawner {
    bus: Arc<MessageBus>,
    llm: SharedLlm,
    templates: HashMap<String, AgentTemplate>,
    default_model: String,
}

impl Spawner {
    /// Builds a spawner with in-memory templates and default model fallback.
    pub fn new(
        bus: Arc<MessageBus>,
        llm: SharedLlm,
        templates: HashMap<String, AgentTemplate>,
        default_model: impl Into<String>,
    ) -> Self {
        Self {
            bus,
            llm,
            templates,
            default_model: default_model.into(),
        }
    }

    /// Convenience: loads templates from disk then constructs a [`Spawner`].
    pub fn from_agents_dir(
        bus: Arc<MessageBus>,
        llm: SharedLlm,
        dir: impl AsRef<Path>,
        default_model: impl Into<String>,
    ) -> Result<Self> {
        let templates = load_templates_from_dir(dir)?;
        Ok(Self::new(bus, llm, templates, default_model))
    }

    fn resolve_template(&self, agent_type: &str) -> Result<&AgentTemplate> {
        self.templates
            .get(agent_type)
            .ok_or_else(|| HatchError::Template(format!("no template for agent_type '{agent_type}'")))
    }

    /// Spawns one Tokio task per plan task; publishes [`HatchMessage::AgentStarted`] and agent output events.
    #[instrument(skip(self, plan))]
    pub async fn spawn_plan(
        &self,
        plan: ExecutionPlan,
    ) -> Result<Vec<JoinHandle<Result<hatch_core::AgentOutput>>>> {
        let run_id = plan.run_id;
        let mut handles = Vec::new();
        for task in plan.tasks {
            let agent_id = AgentId::new_v4();
            let tpl = self.resolve_template(&task.agent_type)?.clone();
            let model = tpl
                .model
                .clone()
                .unwrap_or_else(|| self.default_model.clone());
            let bus = Arc::clone(&self.bus);
            let llm = Arc::clone(&self.llm);
            let system_prompt = tpl.system_prompt.clone();
            let agent_name = tpl.name.clone();
            let agent_type = tpl.agent_type.clone();

            let _ = bus.publish(HatchMessage::AgentStarted {
                agent_id,
                task_id: task.id,
            });

            let handle = tokio::spawn(async move {
                let gen = GenericAgent::new(agent_id, agent_name, agent_type);
                let ctx = AgentContext {
                    task,
                    run_id,
                    llm,
                    bus: Arc::clone(&bus),
                    system_prompt,
                    model,
                };

                match gen.run(ctx).await {
                    Ok(out) => {
                        let _ = bus.publish(HatchMessage::AgentDone(out.clone()));
                        Ok(out)
                    }
                    Err(e) => {
                        error!(target: "hatch_spawner", %e, "agent task failed");
                        let _ = bus.publish(HatchMessage::AgentFailed {
                            agent_id,
                            error: e.to_string(),
                        });
                        Err(e)
                    }
                }
            });

            handles.push(handle);
        }

        info!(target: "hatch_spawner", count = handles.len(), "spawned agent tasks");
        Ok(handles)
    }
}
