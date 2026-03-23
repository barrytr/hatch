//! Rich, step-by-step terminal output for HATCH (similar to an agentic IDE session).
//!
//! Subscribe to the [`MessageBus`](hatch_bus::MessageBus) **before** spawning work so events are not dropped.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

use std::sync::Arc;

use colored::Colorize;
use hatch_bus::{HatchMessage, MessageBus};
use hatch_core::{AgentId, ExecutionPlan};
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

fn clock() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn short_id(id: AgentId) -> String {
    let s = id.to_string();
    if s.len() > 8 {
        format!("{}…", &s[..8])
    } else {
        s
    }
}

/// Prints a session header (intent + model) in a Claude Code–like frame.
pub fn print_session_header(intent: &str, model: &str) {
    println!();
    println!("{}", "╭──────────────────────────────────────────────".dimmed());
    println!(
        "{} {} {}",
        "│".dimmed(),
        "HATCH".white().bold(),
        "— multi-agent session".dimmed()
    );
    println!("{} {}", "│".dimmed(), format!("model: {model}").dimmed());
    println!(
        "{} {}",
        "│".dimmed(),
        format!("intent: {intent}").white()
    );
    println!("{}", "╰──────────────────────────────────────────────".dimmed());
    println!();
}

/// Announces the planning phase before the LLM is called.
pub fn print_planning_start(model: &str) {
    println!(
        "{} {} {}",
        clock().dimmed(),
        "●".yellow().bold(),
        format!("Planner (model: {model}) is decomposing your intent…")
            .yellow()
            .bold()
    );
}

/// Confirms planning finished (call after the planner returns an execution plan).
pub fn print_planning_done(task_count: usize) {
    println!(
        "{} {} {}",
        clock().dimmed(),
        "✓".green().bold(),
        format!("Plan ready — {task_count} task(s)")
            .green()
            .bold()
    );
    println!();
}

/// Publishes [`HatchMessage::PlanReady`] so live reporters can render the DAG.
pub fn emit_plan_ready(bus: &MessageBus, plan: &ExecutionPlan) -> hatch_core::Result<()> {
    bus.publish(HatchMessage::PlanReady(plan.clone()))
}

/// Spawns a background task that prints bus traffic until [`HatchMessage::RunComplete`].
///
/// `plan` is used to resolve human-readable task titles for [`HatchMessage::AgentStarted`].
pub fn spawn_live_reporter(bus: Arc<MessageBus>, plan: Arc<ExecutionPlan>) -> JoinHandle<()> {
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        let task_title = |tid: hatch_core::TaskId| -> String {
            plan.tasks
                .iter()
                .find(|t| t.id == tid)
                .map(|t| format!("{} ({})", t.name, t.agent_type))
                .unwrap_or_else(|| format!("task {tid}"))
        };

        loop {
            match rx.recv().await {
                Ok(HatchMessage::PlanReady(p)) => {
                    println!("{}", "── Execution graph ──".dimmed().bold());
                    println!(
                        "  {} {}",
                        "run".dimmed(),
                        format!("{}", p.run_id).dimmed()
                    );
                    for (i, t) in p.tasks.iter().enumerate() {
                        println!(
                            "  {} {} {}",
                            format!("[{}]", i + 1).cyan().bold(),
                            t.agent_type.cyan(),
                            t.name.white().bold()
                        );
                        println!("      {}", t.description.dimmed());
                        if !t.dependencies.is_empty() {
                            println!(
                                "      {} {}",
                                "depends on:".dimmed(),
                                format!("{:?}", t.dependencies).dimmed()
                            );
                        }
                    }
                    println!();
                }
                Ok(HatchMessage::AgentStarted { agent_id, task_id }) => {
                    let title = task_title(task_id);
                    println!(
                        "{} {} {} {}",
                        clock().dimmed(),
                        "▸".yellow().bold(),
                        "Agent".yellow().bold(),
                        short_id(agent_id).dimmed()
                    );
                    println!(
                        "    {} {}",
                        "task:".dimmed(),
                        title.white()
                    );
                }
                Ok(HatchMessage::AgentProgress { agent_id, message }) => {
                    println!(
                        "    {} {} {}",
                        "│".dimmed(),
                        format!("[{}]", short_id(agent_id)).dimmed(),
                        message.white()
                    );
                }
                Ok(HatchMessage::AgentDone(out)) => {
                    let preview: String = out.content.chars().take(160).collect();
                    let suffix = if out.content.chars().count() > 160 {
                        "…"
                    } else {
                        ""
                    };
                    println!(
                        "{} {} {}",
                        clock().dimmed(),
                        "✓".green().bold(),
                        format!("Agent {} finished task {}", short_id(out.agent_id), out.task_id)
                            .green()
                    );
                    println!("    {}", "preview:".dimmed());
                    println!("    {}{}", preview.dimmed(), suffix.dimmed());
                    if !out.artifacts.is_empty() {
                        let names: Vec<_> = out.artifacts.iter().map(|a| a.name.as_str()).collect();
                        println!(
                            "    {} {}",
                            "artifacts:".dimmed(),
                            names.join(", ").white()
                        );
                    }
                    println!();
                }
                Ok(HatchMessage::AgentFailed { agent_id, error }) => {
                    println!(
                        "{} {} {} — {}",
                        clock().dimmed(),
                        "✗".red().bold(),
                        "Agent failed".red().bold(),
                        short_id(agent_id).dimmed()
                    );
                    println!("    {}", error.red());
                    println!();
                }
                Ok(HatchMessage::RunComplete { .. }) => {
                    // Caller prints the final “run complete” block to keep stdout ordered.
                    break;
                }
                Err(RecvError::Lagged(n)) => {
                    eprintln!(
                        "{}",
                        format!("(console: lagged {n} messages — increase bus capacity)")
                            .yellow()
                    );
                }
                Err(RecvError::Closed) => break,
            }
        }
    })
}
