//! `hatch` binary — plan, run, chat, and inspect agent templates.

mod chat;
mod pipeline;

use std::path::PathBuf;
use std::sync::Arc;
use std::io::{self, IsTerminal};

use anyhow::Context;
use clap::{Parser, Subcommand};
use colored::Colorize;
use hatch_llm::{llm_from_env, OllamaProvider, OpenAiProvider};
use hatch_llm::SharedLlm;
use tracing_subscriber::EnvFilter;

/// Top-level CLI entry.
#[derive(Parser)]
#[command(name = "hatch", about = "Dynamic multi-agent orchestration")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute intent end-to-end (plan, spawn, supervise).
    Run {
        /// Natural language goal.
        intent: String,
        /// LLM backend.
        #[arg(long, value_enum)]
        provider: Option<ProviderArg>,
        /// Model id (overrides `HATCH_DEFAULT_MODEL` when set).
        #[arg(long)]
        model: Option<String>,
        /// Stream LLM output for the planner step only (demo flag).
        #[arg(long, default_value_t = false)]
        stream: bool,
        /// Dump the execution plan as JSON after planning (in addition to the live view).
        #[arg(long, default_value_t = false)]
        show_plan_json: bool,
        /// Output base directory for generated project (where we create `<base>/<run_id>/...`).
        #[arg(long)]
        output_dir: Option<String>,
    },
    /// Show execution plan JSON without running agents.
    Plan {
        intent: String,
        #[arg(long, value_enum)]
        provider: Option<ProviderArg>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value_t = false)]
        stream: bool,
    },
    /// Interactive terminal: chat with the model; `/plan` and `/run` invoke the orchestrator.
    Chat {
        #[arg(long, value_enum)]
        provider: Option<ProviderArg>,
        #[arg(long)]
        model: Option<String>,
    },
    /// List available agent templates discovered on disk.
    Agents,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ProviderArg {
    /// OpenAI HTTP API.
    Openai,
    /// Local Ollama HTTP API.
    Ollama,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Commands::Run {
            intent,
            provider,
            model,
            stream,
            show_plan_json,
            output_dir,
        } => {
            let llm = build_llm(provider)?;
            let model = resolve_model(model);
            let output_base_dir = resolve_output_base_dir(output_dir)?;
            pipeline::run_full_pipeline(
                llm,
                model,
                intent,
                stream,
                show_plan_json,
                agents_dir(),
                output_base_dir,
            )
            .await?;
        }
        Commands::Plan {
            intent,
            provider,
            model,
            stream,
        } => {
            let llm = build_llm(provider)?;
            let model = resolve_model(model);
            let plan = pipeline::plan_intent(llm, model, &intent, stream).await?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        Commands::Chat { provider, model } => {
            let llm = build_llm(provider)?;
            let model = resolve_model(model);
            chat::chat_loop(llm, model, agents_dir()).await?;
        }
        Commands::Agents => agents_command()?,
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("hatch=info,hatch_core=info,hatch_llm=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn agents_dir() -> PathBuf {
    std::env::var("HATCH_AGENTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("agents"))
}

fn resolve_output_base_dir(output_dir: Option<String>) -> anyhow::Result<Option<PathBuf>> {
    if let Some(s) = output_dir {
        return Ok(Some(PathBuf::from(s)));
    }
    if let Ok(v) = std::env::var("HATCH_OUTPUT_DIR") {
        return Ok(Some(PathBuf::from(v)));
    }
    if std::env::var("HATCH_NO_PROMPT").is_ok() {
        return Ok(None);
    }

    let stdin_is_tty = io::stdin().is_terminal();
    if !stdin_is_tty {
        return Ok(None);
    }

    print!("Output base directory (default hatch_runs): ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let line = line.trim();
    let dir = if line.is_empty() {
        PathBuf::from("hatch_runs")
    } else {
        PathBuf::from(line)
    };
    Ok(Some(dir))
}

fn resolve_model(explicit: Option<String>) -> String {
    explicit
        .or_else(|| std::env::var("HATCH_DEFAULT_MODEL").ok())
        .unwrap_or_else(|| "gpt-4o-mini".to_string())
}

fn build_llm(provider: Option<ProviderArg>) -> anyhow::Result<SharedLlm> {
    match provider {
        Some(ProviderArg::Openai) => Ok(Arc::new(
            OpenAiProvider::from_env().context("OpenAI provider configuration")?,
        )),
        Some(ProviderArg::Ollama) => Ok(Arc::new(
            OllamaProvider::new().context("Ollama provider configuration")?,
        )),
        None => llm_from_env().context("LLM from HATCH_DEFAULT_PROVIDER"),
    }
}

fn agents_command() -> anyhow::Result<()> {
    let dir = agents_dir();
    let map = hatch_spawner::load_templates_from_dir(&dir)
        .with_context(|| format!("load templates from {}", dir.display()))?;
    if map.is_empty() {
        println!("No templates found under {}", dir.display());
        return Ok(());
    }
    for (k, v) in map {
        println!("{} — {} ({})", k.bold(), v.name, v.agent_type);
    }
    Ok(())
}
