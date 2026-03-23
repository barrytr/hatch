use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;
use std::{fs, path::PathBuf};

use hatch_bus::{HatchMessage, MessageBus};
use hatch_core::{AgentOutput, ExecutionPlan, HatchError, Result, TaskId};
use tokio::sync::broadcast::{self, error::RecvError};
use tracing::{info, warn};

/// Aggregated outcome for a completed orchestration run.
pub struct RunResult {
    /// Run identifier from the plan.
    pub run_id: hatch_core::RunId,
    /// Original user intent text.
    pub intent: String,
    /// Successful agent outputs in plan task order.
    pub outputs: Vec<AgentOutput>,
    /// Short textual merge summary for operators.
    pub summary: String,
    /// Filesystem location where agent artifacts were materialized.
    pub output_dir: PathBuf,
    /// Full paths of files written.
    pub written_files: Vec<PathBuf>,
}

/// Subscribes to [`MessageBus`] traffic and waits for all plan tasks to finish.
pub struct Supervisor {
    bus: Arc<MessageBus>,
}

impl Supervisor {
    /// Creates a supervisor bound to the given bus.
    pub fn new(bus: Arc<MessageBus>) -> Self {
        Self { bus }
    }

    /// Waits until each task ID in `plan` has a matching [`HatchMessage::AgentDone`] or any failure.
    ///
    /// Call [`MessageBus::subscribe`](hatch_bus::MessageBus::subscribe) **before** spawning agents
    /// so early [`HatchMessage::AgentDone`] events are not dropped.
    pub async fn supervise(
        &self,
        plan: &ExecutionPlan,
        mut rx: broadcast::Receiver<HatchMessage>,
        output_base_dir: PathBuf,
    ) -> Result<RunResult> {
        let expected: HashSet<TaskId> = plan.tasks.iter().map(|t| t.id).collect();
        let mut pending: HashSet<TaskId> = expected.clone();
        let mut by_task: HashMap<TaskId, AgentOutput> = HashMap::new();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3600);

        while !pending.is_empty() {
            let timeout = tokio::time::sleep_until(deadline);
            tokio::pin!(timeout);

            tokio::select! {
                _ = &mut timeout => {
                    return Err(HatchError::Supervisor("supervisor timed out waiting for agents".into()));
                }
                recv = rx.recv() => {
                    match recv {
                        Ok(HatchMessage::AgentDone(out)) => {
                            if pending.remove(&out.task_id) {
                                by_task.insert(out.task_id, out);
                            }
                        }
                        Ok(HatchMessage::AgentFailed { error, .. }) => {
                            return Err(HatchError::Supervisor(format!("agent failed: {error}")));
                        }
                        Ok(HatchMessage::RunComplete { .. }) => {
                            // Ignore nested run complete signals for simplicity.
                        }
                        Ok(_) => {}
                        Err(RecvError::Lagged(n)) => {
                            warn!(target: "hatch_supervisor", lagged = n, "supervisor lagged; increase bus capacity");
                        }
                        Err(RecvError::Closed) => {
                            return Err(HatchError::Supervisor("bus closed while supervising".into()));
                        }
                    }
                }
            }
        }

        let mut outputs = Vec::new();
        for t in &plan.tasks {
            if let Some(o) = by_task.remove(&t.id) {
                outputs.push(o);
            }
        }

        let (output_dir, written_files) =
            materialize_artifacts(output_base_dir, plan.run_id, &outputs)?;
        let summary = merge_summary(plan, &outputs, &output_dir);

        let run_result = RunResult {
            run_id: plan.run_id,
            intent: plan.intent.clone(),
            outputs: outputs.clone(),
            summary: summary.clone(),
            output_dir: output_dir.clone(),
            written_files,
        };

        let _ = self.bus.publish(HatchMessage::RunComplete {
            run_id: plan.run_id,
            outputs,
        });

        info!(target: "hatch_supervisor", run_id = %plan.run_id, "run complete");
        Ok(run_result)
    }
}

fn merge_summary(plan: &ExecutionPlan, outputs: &[AgentOutput], output_dir: &PathBuf) -> String {
    let mut parts = vec![format!("Run {} — {}", plan.run_id, plan.intent)];
    for o in outputs {
        let preview: String = o.content.chars().take(200).collect();
        parts.push(format!("- Task {}: {}", o.task_id, preview));
    }
    parts.push(format!("Artifacts written to {}", output_dir.display()));
    parts.join("\n")
}

fn materialize_artifacts(
    output_base_dir: PathBuf,
    run_id: hatch_core::RunId,
    outputs: &[AgentOutput],
) -> Result<(PathBuf, Vec<PathBuf>)> {
    let output_dir = output_base_dir.join(run_id.to_string());
    fs::create_dir_all(&output_dir)?;

    let approvals = if approval_enabled() {
        collect_top_level_approvals(outputs)?
    } else {
        HashMap::new()
    };

    let mut written = Vec::new();
    for out in outputs {
        for artifact in &out.artifacts {
            let rel = artifact.name.trim();
            if rel.is_empty() {
                continue;
            }
            if rel.starts_with('/') {
                return Err(HatchError::Config(format!(
                    "refusing to write absolute path: {}",
                    rel
                )));
            }
            if rel.split('/').any(|p| p == "..") {
                return Err(HatchError::Config(format!(
                    "refusing to write path with parent traversal: {}",
                    rel
                )));
            }
            if !is_top_level_approved(rel, &approvals) {
                continue;
            }

            let path = output_dir.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, artifact.content.as_bytes())?;
            written.push(path);
        }
    }

    Ok((output_dir, written))
}

fn approval_enabled() -> bool {
    // Default OFF for non-interactive "generate/build" runs.
    match std::env::var("HATCH_APPROVAL_TOP_LEVEL") {
        Ok(v) => {
            let v = v.trim();
            !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("no"))
        }
        Err(_) => false,
    }
}

fn top_level_of(path: &str) -> String {
    path.split('/').next().unwrap_or(path).to_string()
}

fn collect_top_level_approvals(outputs: &[AgentOutput]) -> Result<HashMap<String, bool>> {
    let mut top_levels = HashSet::new();
    for out in outputs {
        for artifact in &out.artifacts {
            let rel = artifact.name.trim();
            if rel.is_empty() {
                continue;
            }
            top_levels.insert(top_level_of(rel));
        }
    }

    if top_levels.is_empty() {
        return Ok(HashMap::new());
    }

    let mut sorted = top_levels.into_iter().collect::<Vec<_>>();
    sorted.sort();

    println!();
    println!("Top-level folder approval before writing files:");
    println!("(y = allow, n = skip, a = allow all remaining, q = cancel run)");

    let mut approvals = HashMap::new();
    let mut allow_all_remaining = false;
    for top in sorted {
        if allow_all_remaining {
            approvals.insert(top, true);
            continue;
        }
        let approved = ask_approval(&top)?;
        match approved {
            ApprovalAnswer::Yes => {
                approvals.insert(top, true);
            }
            ApprovalAnswer::No => {
                approvals.insert(top, false);
            }
            ApprovalAnswer::All => {
                approvals.insert(top, true);
                allow_all_remaining = true;
            }
            ApprovalAnswer::Quit => {
                return Err(HatchError::Supervisor(
                    "run cancelled by user during folder approval".to_string(),
                ));
            }
        }
    }
    Ok(approvals)
}

fn is_top_level_approved(path: &str, approvals: &HashMap<String, bool>) -> bool {
    if approvals.is_empty() {
        return true;
    }
    let top = top_level_of(path);
    approvals.get(&top).copied().unwrap_or(true)
}

enum ApprovalAnswer {
    Yes,
    No,
    All,
    Quit,
}

fn ask_approval(top_level: &str) -> Result<ApprovalAnswer> {
    loop {
        print!("Create `{}` ? [y/n/a/q]: ", top_level);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let answer = input.trim().to_ascii_lowercase();
        match answer.as_str() {
            "y" | "yes" => return Ok(ApprovalAnswer::Yes),
            "n" | "no" => return Ok(ApprovalAnswer::No),
            "a" | "all" => return Ok(ApprovalAnswer::All),
            "q" | "quit" => return Ok(ApprovalAnswer::Quit),
            _ => {
                println!("Please type y, n, a, or q.");
            }
        }
    }
}
