//! Interactive REPL for planner ideation plus `/plan` and `/run` orchestration hooks.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use colored::Colorize;
use futures_util::StreamExt;
use hatch_llm::{CompletionRequest, Message, MessageRole, SharedLlm};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde::Deserialize;

use crate::pipeline;

const CHAT_SYSTEM: &str = r#"You are Hatch — an interactive terminal planner assistant.
Help the user shape an idea into an implementable software plan.

Rules:
- Mirror the user language (Vietnamese or English).
- Ask concise clarifying questions when needed.
- Propose a small MVP slice first.
- Keep answers short and actionable.

You may recommend that the user runs:
  /plan [goal]  or  /run [goal]
But do NOT output JSON plans yourself."#;

const AUTO_RUN_SYSTEM: &str = r#"You are the readiness checker for HATCH.
Given the conversation, decide if the user has provided enough detail to run a multi-agent implementation pass.

Return ONLY valid JSON (no markdown) exactly matching:
{
  "ready": true|false,
  "intent": "<concise implementation goal to feed into /run>",
  "missing": ["<what is still needed>"]
}

- Set ready=true only when you can form a sensible MVP intent.
- intent must be non-empty."#;

const MAX_HISTORY_CHARS: usize = 24_000;

/// Runs the interactive session until `/quit` or EOF.
pub async fn chat_loop(
    llm: SharedLlm,
    model: String,
    agents_dir: PathBuf,
) -> anyhow::Result<()> {
    println!();
    println!("{}", "╭──────────────────────────────────────────────".dimmed());
    println!(
        "{} {} {}",
        "│".dimmed(),
        "HATCH planner chat".white().bold(),
        "— brainstorm, then /plan or /run".dimmed()
    );
    println!(
        "{} {}",
        "│".dimmed(),
        format!("model: {model}").dimmed()
    );
    println!("{}", "╰──────────────────────────────────────────────".dimmed());
    println!();
    print_help();
    println!();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let reader = std::thread::spawn(move || {
        let mut rl = match DefaultEditor::new() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("readline init failed: {e}");
                return;
            }
        };

        let hist_path = std::env::current_dir()
            .unwrap_or_default()
            .join(".hatch_repl_history");
        let _ = rl.load_history(&hist_path);

        loop {
            match rl.readline("hatch> ") {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let _ = rl.add_history_entry(trimmed);
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("{}", "(use /quit to exit)".dimmed());
                    continue;
                }
                Err(ReadlineError::Eof) => break,
                Err(e) => {
                    eprintln!("readline: {e}");
                    break;
                }
            }
        }

        let _ = rl.save_history(&hist_path);
    });

    let mut history: Vec<Message> = Vec::new();
    let mut idea_notes: Vec<String> = Vec::new();

    let mut auto_run_enabled = true;
    let mut auto_run_done = false;
    let mut awaiting_output_base_dir = false;
    let mut pending_run_intent: Option<String> = None;

    while let Some(line) = rx.recv().await {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }

        if awaiting_output_base_dir {
            let intent = match pending_run_intent.take() {
                Some(v) => v,
                None => {
                    awaiting_output_base_dir = false;
                    println!("{}", "Internal error: pending run intent missing".red());
                    continue;
                }
            };

            if t.eq_ignore_ascii_case("/cancel") {
                awaiting_output_base_dir = false;
                auto_run_done = false;
                println!("{}", "Cancelled run.".yellow());
                continue;
            }

            println!(
                "{} {}",
                "Writing project to:".cyan().bold(),
                if t.eq_ignore_ascii_case("/default") {
                    "default output base dir".dimmed().to_string()
                } else {
                    t.dimmed().to_string()
                }
            );

            let output_base_dir = if t.eq_ignore_ascii_case("/default") {
                None
            } else {
                Some(PathBuf::from(t))
            };

            if let Err(e) = pipeline::run_full_pipeline(
                Arc::clone(&llm),
                model.clone(),
                intent,
                false,
                false,
                agents_dir.clone(),
                output_base_dir,
            )
            .await
            {
                println!("{} {e:#}", "run failed:".red().bold());
            }

            awaiting_output_base_dir = false;
            println!();
            continue;
        }

        // --- meta commands ---
        if t.eq_ignore_ascii_case("/quit") || t.eq_ignore_ascii_case("/exit") || t == "/q" {
            println!("{}", "Bye.".dimmed());
            break;
        }

        if t.eq_ignore_ascii_case("/help") || t == "/?" {
            print_help();
            continue;
        }

        if t.eq_ignore_ascii_case("/clear") {
            history.clear();
            idea_notes.clear();
            auto_run_done = false;
            println!("{}", "Conversation and idea notes cleared.".green());
            continue;
        }

        if t.eq_ignore_ascii_case("/idea") {
            if let Some(intent) = compose_intent(&idea_notes) {
                println!("{}", "── Current idea brief ──".cyan().bold());
                println!("{}", intent);
            } else {
                println!("{}", "No idea yet. Chat first, then /idea.".yellow());
            }
            println!();
            continue;
        }

        if t.starts_with("/autorun") {
            let rest = t.trim_start_matches("/autorun").trim();
            match rest.to_ascii_lowercase().as_str() {
                "" => {
                    println!(
                        "{} {} (done={})",
                        "auto-run".cyan().bold(),
                        if auto_run_enabled { "ON".green() } else { "OFF".red() },
                        auto_run_done
                    );
                }
                "on" => {
                    auto_run_enabled = true;
                    auto_run_done = false;
                    println!("{}", "Auto-run enabled.".green().bold());
                }
                "off" => {
                    auto_run_enabled = false;
                    println!("{}", "Auto-run disabled.".yellow().bold());
                }
                "reset" => {
                    auto_run_done = false;
                    println!("{}", "Auto-run reset.".green().bold());
                }
                other => {
                    println!(
                        "{} {}",
                        "Unknown /autorun argument:".red().bold(),
                        other
                    );
                    println!("{}", "Use: /autorun on|off|reset".dimmed());
                }
            }
            println!();
            continue;
        }

        // --- planning / running ---
        if t.starts_with("/plan") {
            let rest = t.trim_start_matches("/plan").trim();
            let intent = if rest.is_empty() {
                match compose_intent(&idea_notes) {
                    Some(v) => v,
                    None => {
                        println!("{}", "Chưa có idea. Hãy chat trước hoặc dùng /plan <goal>.".yellow());
                        continue;
                    }
                }
            } else {
                rest.to_string()
            };

            println!("{}", "── Planning (meta-agent) ──".cyan().bold());
            match pipeline::plan_intent(Arc::clone(&llm), model.clone(), &intent, false).await {
                Ok(plan) => {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                }
                Err(e) => {
                    println!("{} {e}", "plan failed:".red().bold());
                }
            }
            println!();
            continue;
        }

        if t.starts_with("/run") {
            let rest = t.trim_start_matches("/run").trim();
            let intent = if rest.is_empty() {
                match compose_intent(&idea_notes) {
                    Some(v) => v,
                    None => {
                        println!("{}", "Chưa có idea. Hãy chat trước hoặc dùng /run <goal>.".yellow());
                        continue;
                    }
                }
            } else {
                rest.to_string()
            };

            auto_run_done = true;

            pending_run_intent = Some(intent);
            awaiting_output_base_dir = true;
            println!();
            println!(
                "{}",
                "Choose where to create the generated project (base directory).".cyan().bold()
            );
            println!(
                "{}",
                "Type an absolute/relative path, or `/default` for `HATCH_OUTPUT_DIR` (or hatch_runs)."
                    .dimmed()
            );
            println!("{} {}", "Or".dimmed(), "`/cancel`".yellow().bold());
            continue;
        }

        // --- normal chat path: user message -> assistant reply -> maybe auto-run ---
        idea_notes.push(t.to_string());
        history.push(Message {
            role: MessageRole::User,
            content: t.to_string(),
        });
        trim_history(&mut history);

        println!("{}", "assistant".cyan().bold());
        let reply = match stream_chat(&llm, &model, &history).await {
            Ok(s) => s,
            Err(e) => {
                println!("{} {e:#}", "chat error:".red());
                history.pop();
                idea_notes.pop();
                continue;
            }
        };

        history.push(Message {
            role: MessageRole::Assistant,
            content: reply,
        });
        trim_history(&mut history);
        println!();

        if auto_run_enabled && !auto_run_done {
            match check_ready_and_intent(&llm, &model, &history, &idea_notes).await {
                Ok(Some(decision)) => {
                    auto_run_done = true;
                    println!(
                        "{} {}",
                        "auto-run:".cyan().bold(),
                        "readiness met; starting multi-agent run…".green().bold()
                    );
                    println!();

                    pending_run_intent = Some(decision.intent);
                    awaiting_output_base_dir = true;
                    println!(
                        "{}",
                        "Before writing files, choose output base directory (see /run prompt)."
                            .yellow()
                            .bold()
                    );
                    println!(
                        "{}",
                        "Type a path, or `/default`, or `/cancel`."
                            .dimmed()
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    // If readiness check fails, do not interrupt the chat.
                    println!(
                        "{} {e:#}",
                        "auto-run check error (ignored):".yellow().bold()
                    );
                    println!();
                }
            }
        }
    }

    // Do not join the readline thread: it may block on `readline()` until EOF.
    std::mem::drop(rx);
    std::mem::drop(reader);
    Ok(())
}

fn print_help() {
    println!("{}", "Commands:".white().bold());
    println!("  {}          {}", "/idea".cyan(), "— show current idea brief from your chat");
    println!("  {}  {}", "/plan [goal]".cyan(), "— if omitted, use current idea brief");
    println!("  {}   {}", "/run [goal]".cyan(), "— if omitted, run using current idea brief");
    println!("  {}         {}", "/autorun".cyan(), "— auto-run when ready (on|off|reset)");
    println!("  {}         {}", "/clear".cyan(), "— reset chat + idea notes");
    println!("  {}          {}", "/help".cyan(), "— this list");
    println!("  {}          {}", "/quit".cyan(), "— exit");
    println!(
        "  {} {}",
        "(plain text)".dimmed(),
        "— talk with the planner assistant (streamed)".dimmed()
    );
}

fn compose_intent(notes: &[String]) -> Option<String> {
    if notes.is_empty() {
        return None;
    }
    Some(notes.join("\n"))
}

fn trim_history(msgs: &mut Vec<Message>) {
    loop {
        let total: usize = msgs.iter().map(|m| m.content.len()).sum();
        if total <= MAX_HISTORY_CHARS || msgs.is_empty() {
            break;
        }
        msgs.remove(0);
    }
}

async fn stream_chat(
    llm: &SharedLlm,
    model: &str,
    history: &[Message],
) -> anyhow::Result<String> {
    let req = CompletionRequest {
        model: model.to_string(),
        messages: history.to_vec(),
        max_tokens: Some(4096),
        temperature: Some(0.35),
        system: Some(CHAT_SYSTEM.to_string()),
    };

    let mut stream = llm
        .complete_stream(req)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let mut acc = String::new();
    while let Some(chunk) = stream.next().await {
        let piece = chunk.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        acc.push_str(&piece);
        print!("{piece}");
        use std::io::Write;
        std::io::stdout()
            .flush()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }
    println!();
    Ok(acc)
}

#[derive(Debug, Deserialize)]
struct AutoRunDecision {
    ready: bool,
    intent: String,
    #[serde(default)]
    missing: Vec<String>,
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

async fn check_ready_and_intent(
    llm: &SharedLlm,
    model: &str,
    history: &[Message],
    idea_notes: &[String],
) -> anyhow::Result<Option<AutoRunDecision>> {
    let idea_brief = compose_intent(idea_notes).unwrap_or_default();

    let transcript = history
        .iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
            };
            format!("{role}: {}", m.content)
        })
        .collect::<Vec<_>>()
        .join("
");

    let prompt = format!(
        "Conversation transcript:
{transcript}

Idea brief (user notes):
{idea_brief}

Decide readiness now." 
    );

    let req = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: MessageRole::User,
            content: prompt,
        }],
        max_tokens: Some(512),
        temperature: Some(0.0),
        system: Some(AUTO_RUN_SYSTEM.to_string()),
    };

    let resp = llm
        .complete(req)
        .await
        .context("auto-run readiness check request failed")?;

    let json = strip_json_fences(&resp.content);
    let decision: AutoRunDecision = serde_json::from_str(json)
        .context("failed to parse auto-run readiness JSON")?;

    if decision.ready {
        Ok(Some(decision))
    } else {
        Ok(None)
    }
}
