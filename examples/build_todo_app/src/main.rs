//! End-to-end example: plan, spawn agents, supervise, print artifacts.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use hatch_bus::MessageBus;
use hatch_console::{
    emit_plan_ready, print_planning_done, print_planning_start, print_session_header,
    spawn_live_reporter,
};
use hatch_llm::{OllamaProvider, OpenAiProvider, SharedLlm};
use hatch_planner::Planner;
use hatch_spawner::Spawner;
use hatch_supervisor::Supervisor;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("hatch=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();

    let llm: SharedLlm = match std::env::var("OPENAI_API_KEY") {
        Ok(_) => Arc::new(OpenAiProvider::from_env().context("OpenAI")?),
        Err(_) => Arc::new(OllamaProvider::new().context("Ollama")?),
    };

    let model = std::env::var("HATCH_DEFAULT_MODEL").unwrap_or_else(|_| {
        if std::env::var("OPENAI_API_KEY").is_ok() {
            "gpt-4o-mini".into()
        } else {
            "llama3.2".into()
        }
    });

    let bus = Arc::new(MessageBus::new(1024));
    let planner = Planner::new(Arc::clone(&llm), model.clone());

    let intent = "build a todo app with React frontend and REST API backend";
    print_session_header(intent, &model);
    print_planning_start(&model);
    let plan = planner.plan(intent).await?;
    print_planning_done(plan.tasks.len());

    let plan_arc = Arc::new(plan.clone());
    let reporter = spawn_live_reporter(Arc::clone(&bus), Arc::clone(&plan_arc));
    emit_plan_ready(&bus, &plan)?;
    println!();
    println!("Spawning agents and streaming progress…");
    println!();

    let agents_dir = std::env::var("HATCH_AGENTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("agents"));

    let spawner = Spawner::from_agents_dir(
        Arc::clone(&bus),
        Arc::clone(&llm),
        &agents_dir,
        model,
    )
    .context("spawner templates")?;

    let supervisor = Supervisor::new(Arc::clone(&bus));
    let rx = bus.subscribe();
    let plan_sup = plan.clone();
    let output_base_dir = std::env::var("HATCH_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("hatch_runs"));
    let sup_task = tokio::spawn(async move {
        supervisor
            .supervise(&plan_sup, rx, output_base_dir)
            .await
    });

    let handles = spawner.spawn_plan(plan).await?;

    let result = sup_task.await??;

    for h in handles {
        let _ = h.await;
    }

    reporter
        .await
        .context("live reporter task panicked or was cancelled")?;

    println!();
    println!("── Run complete ──");
    println!("  outputs: {}", result.outputs.len());
    println!();

    tracing::info!("run complete");
    for out in &result.outputs {
        tracing::info!(task_id = %out.task_id, "agent output");
        for a in &out.artifacts {
            println!("--- {} ---\n{}", a.name, a.content);
        }
    }

    Ok(())
}
