use std::collections::HashMap;
use std::path::Path;

use hatch_core::{HatchError, Result};
use serde::Deserialize;
use tracing::{info, warn};

/// Configuration loaded from an agent template TOML file.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentTemplate {
    /// Display name for logging.
    pub name: String,
    /// Template key matched against [`hatch_core::Task::agent_type`].
    pub agent_type: String,
    /// System prompt injected into the LLM.
    pub system_prompt: String,
    /// Optional per-template default model override.
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TomlFile {
    name: String,
    agent_type: String,
    system_prompt: String,
    model: Option<String>,
}

/// Loads all `*.toml` files in `dir` keyed by [`AgentTemplate::agent_type`].
pub fn load_templates_from_dir(dir: impl AsRef<Path>) -> Result<HashMap<String, AgentTemplate>> {
    let dir = dir.as_ref();
    let mut map = HashMap::new();
    let rd = std::fs::read_dir(dir).map_err(|e| {
        HatchError::Config(format!("failed to read agents dir {}: {e}", dir.display()))
    })?;

    for ent in rd {
        let ent = ent.map_err(HatchError::Io)?;
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).map_err(HatchError::Io)?;
        let parsed: TomlFile = toml::from_str(&raw).map_err(HatchError::TomlDeserialize)?;
        let tpl = AgentTemplate {
            name: parsed.name,
            agent_type: parsed.agent_type.clone(),
            system_prompt: parsed.system_prompt,
            model: parsed.model,
        };
        info!(target: "hatch_spawner", path = %path.display(), agent_type = %tpl.agent_type, "loaded agent template");
        if map.insert(tpl.agent_type.clone(), tpl).is_some() {
            warn!(target: "hatch_spawner", path = %path.display(), "duplicate agent_type, last wins");
        }
    }

    Ok(map)
}
