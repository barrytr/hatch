//! One-shot planning and full multi-agent runs shared by `run` and `chat`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use colored::Colorize;
use hatch_bus::MessageBus;
use hatch_core::ExecutionPlan;
use hatch_console::{
    emit_plan_ready, print_planning_done, print_planning_start, print_session_header,
    spawn_live_reporter,
};
use hatch_llm::{CompletionRequest, LlmProvider, Message, MessageRole, SharedLlm};
use hatch_planner::Planner;
use hatch_spawner::Spawner;
use hatch_supervisor::Supervisor;
use futures_util::StreamExt;
use tokio::process::Command;
use tracing::{info, warn};

/// Runs planner only and returns the execution plan.
pub async fn plan_intent(
    llm: SharedLlm,
    model: String,
    intent: &str,
    stream: bool,
) -> anyhow::Result<ExecutionPlan> {
    if stream {
        planner_plan_streaming(intent, &model, &llm).await
    } else {
        let planner = Planner::new(Arc::clone(&llm), model.clone());
        planner.plan(intent).await.map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}

/// Full plan → spawn → supervise → artifacts (same as `hatch run`).
pub async fn run_full_pipeline(
    llm: SharedLlm,
    model: String,
    intent: String,
    stream: bool,
    show_plan_json: bool,
    agents_dir: PathBuf,
    output_base_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let bus = Arc::new(MessageBus::new(1024));
    let output_base_dir = output_base_dir.unwrap_or_else(|| {
        std::env::var("HATCH_OUTPUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("hatch_runs"))
    });

    print_session_header(&intent, &model);
    print_planning_start(&model);

    let plan = if stream {
        planner_plan_streaming(&intent, &model, &llm).await?
    } else {
        let planner = Planner::new(Arc::clone(&llm), model.clone());
        planner.plan(&intent).await.map_err(|e| anyhow::anyhow!(e.to_string()))?
    };
    print_planning_done(plan.tasks.len());

    if show_plan_json {
        println!("{}", "── Plan (JSON) ──".dimmed().bold());
        println!("{}", serde_json::to_string_pretty(&plan)?);
        println!();
    }

    let plan_arc = Arc::new(plan.clone());
    let reporter = spawn_live_reporter(Arc::clone(&bus), Arc::clone(&plan_arc));
    emit_plan_ready(&bus, &plan).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let spawner = Spawner::from_agents_dir(
        Arc::clone(&bus),
        Arc::clone(&llm),
        &agents_dir,
        model.clone(),
    )
    .context("load spawner templates")?;

    println!(
        "{} {}",
        "●".cyan().bold(),
        "Spawning agents and streaming progress…"
            .cyan()
            .bold()
    );
    println!();

    let supervisor = Supervisor::new(Arc::clone(&bus));
    let rx = bus.subscribe();
    let plan_sup = plan.clone();
    let sup_task = tokio::spawn(async move {
        supervisor
            .supervise(&plan_sup, rx, output_base_dir)
            .await
    });

    let handles = spawner
        .spawn_plan(plan.clone())
        .await
        .context("spawn agents")?;

    let result = sup_task
        .await
        .context("supervisor join")?
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    for h in handles {
        let _ = h.await;
    }

    reporter
        .await
        .context("live reporter task panicked or was cancelled")?;

    maybe_local_build_fix_and_serve(&llm, &model, &intent, &result.output_dir).await?;

    println!("{}", "── Run complete ──".dimmed().bold());
    println!(
        "  {} {}",
        "outputs:".dimmed(),
        format!("{}", result.outputs.len()).white().bold()
    );
    println!();

    println!("{}", "── Final summary ──".green().bold());
    println!("{}", result.summary);
    println!();
    println!("{}", "── Artifacts (full) ──".green().bold());
    for out in &result.outputs {
        for a in &out.artifacts {
            println!("- {} ({:?})\n{}", a.name, a.kind, a.content);
        }
    }
    Ok(())
}

fn truthy_env(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn local_fix_max_attempts() -> usize {
    std::env::var("HATCH_FIX_MAX_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
}

fn backend_port() -> u16 {
    std::env::var("HATCH_BACKEND_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3001)
}

fn frontend_port() -> u16 {
    std::env::var("HATCH_DEV_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5173)
}

fn npm_script_exists(dir: &PathBuf, script: &str) -> bool {
    let pkg = dir.join("package.json");
    if !pkg.exists() {
        return false;
    }
    let raw = match fs::read_to_string(&pkg) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return false,
    };
    parsed
        .get("scripts")
        .and_then(|s| s.get(script))
        .and_then(|v| v.as_str())
        .is_some()
}

fn frontend_envs_for_api(backend_url: &str) -> Vec<(&'static str, String)> {
    vec![
        ("VITE_API_URL", backend_url.to_string()),
        ("REACT_APP_API_URL", backend_url.to_string()),
        ("NEXT_PUBLIC_API_URL", backend_url.to_string()),
    ]
}

fn frontend_dir(output_dir: &PathBuf) -> PathBuf {
    let p = output_dir.join("frontend/package.json");
    if p.exists() {
        output_dir.join("frontend")
    } else {
        output_dir.clone()
    }
}

fn backend_dir(output_dir: &PathBuf) -> Option<PathBuf> {
    let pkg = output_dir.join("backend/package.json");
    if pkg.exists() {
        Some(output_dir.join("backend"))
    } else {
        let cargo = output_dir.join("backend/Cargo.toml");
        if cargo.exists() {
            Some(output_dir.join("backend"))
        } else {
            None
        }
    }
}

async fn run_fullstack_build_capture(
    output_dir: &PathBuf,
    intent: &str,
) -> anyhow::Result<CmdResult> {
    let backend_port = backend_port();
    let backend_url = format!("http://127.0.0.1:{backend_port}");
    let mut combined = String::new();
    let mut ok = true;
    let mut ran_any = false;

    // Prefer root build if present.
    if npm_script_exists(output_dir, "build") {
        ran_any = true;
        let res = run_capture(output_dir, "npm", &["run", "build"]).await?;
        if !res.success {
            ok = false;
        }
        combined.push_str(&format!("--- root build ---\n{}\n", res.combined));
    } else {
        // Frontend build if present.
        let fd = frontend_dir(output_dir);
        if npm_script_exists(&fd, "build") {
            ran_any = true;
            let envs = frontend_envs_for_api(&backend_url);
            let res =
                run_capture_with_env(&fd, "npm", &["run", "build"], &envs).await?;
            if !res.success {
                ok = false;
            }
            combined.push_str(&format!("--- frontend build ({}) ---\n{}\n", fd.display(), res.combined));
        }

        // Backend build if present.
        if let Some(bd) = backend_dir(output_dir) {
            let npm_pkg = bd.join("package.json");
            if npm_pkg.exists() {
                if npm_script_exists(&bd, "build") {
                    ran_any = true;
                    let res = run_capture(&bd, "npm", &["run", "build"]).await?;
                    if !res.success {
                        ok = false;
                    }
                    combined.push_str(&format!(
                        "--- backend build ({}) ---\n{}\n",
                        bd.display(),
                        res.combined
                    ));
                }
            } else if bd.join("Cargo.toml").exists() {
                ran_any = true;
                let res = run_capture(&bd, "cargo", &["build"]).await?;
                if !res.success {
                    ok = false;
                }
                combined.push_str(&format!(
                    "--- cargo build ({}) ---\n{}\n",
                    bd.display(),
                    res.combined
                ));
            }
        }
    }

    if !ran_any {
        combined.push_str(&format!(
            "no build scripts found for fullstack. intent hint: {}\n",
            intent
        ));
        ok = true;
    }

    Ok(CmdResult {
        success: ok,
        combined,
    })
}

async fn maybe_serve_fullstack(output_dir: &PathBuf) -> anyhow::Result<()> {
    if !truthy_env("HATCH_SERVE") {
        return Ok(());
    }

    let backend_port = backend_port();
    let frontend_port = frontend_port();
    let backend_url = format!("http://127.0.0.1:{backend_port}");

    // If root has a dev script, prefer that (monorepo-friendly).
    if npm_script_exists(output_dir, "dev") {
        let envs = frontend_envs_for_api(&backend_url);
        let mut child = {
            let mut c = Command::new("npm");
            c.args(["run", "dev"])
                .arg("--")
                .arg("--host")
                .arg("127.0.0.1")
                .arg("--port")
                .arg(frontend_port.to_string())
                .current_dir(output_dir);
            for (k, v) in envs {
                c.env(k, v);
            }
            c.spawn()?
        };
        let _ = child.wait().await;
        return Ok(());
    }

    let fd = frontend_dir(output_dir);

    // Spawn backend dev if available.
    let mut backend_child: Option<tokio::process::Child> = None;
    if let Some(bd) = backend_dir(output_dir) {
        let mut cmd = Command::new("npm");
        cmd.current_dir(&bd);
        if npm_script_exists(&bd, "dev") {
            cmd.args(["run", "dev"]);
        } else if npm_script_exists(&bd, "start") {
            cmd.args(["run", "start"]);
        } else {
            cmd.args(["run", "dev"]);
        }
        cmd.env("PORT", backend_port.to_string());
        backend_child = Some(cmd.spawn()?);
    }

    // Small delay to let backend boot.
    tokio::time::sleep(Duration::from_millis(800)).await;

    let envs = frontend_envs_for_api(&backend_url);
    let mut front = Command::new("npm");
    front.current_dir(&fd);
    if npm_script_exists(&fd, "dev") {
        front.args(["run", "dev"]);
        front.args([
            "--",
            "--host",
            "127.0.0.1",
            "--port",
            &frontend_port.to_string(),
        ]);
    } else if npm_script_exists(&fd, "start") {
        front.args(["run", "start"]);
        front.env("PORT", frontend_port.to_string());
    } else {
        front.args(["run", "dev"]);
        front.args([
            "--",
            "--host",
            "127.0.0.1",
            "--port",
            &frontend_port.to_string(),
        ]);
    }
    for (k, v) in envs {
        front.env(k, v);
    }
    let mut front_child = front.spawn()?;
    let _ = front_child.wait().await;

    if let Some(mut b) = backend_child {
        let _ = b.kill();
        let _ = b.wait().await;
    }

    Ok(())
}

async fn maybe_local_build_fix_and_serve(
    llm: &SharedLlm,
    model: &str,
    intent: &str,
    output_dir: &PathBuf,
) -> anyhow::Result<()> {
    if !truthy_env("HATCH_LOCAL_LOOP") {
        return Ok(());
    }

    let root_pkg = output_dir.join("package.json");
    let frontend_pkg = output_dir.join("frontend/package.json");
    let backend_pkg = output_dir.join("backend/package.json");
    let has_any_node_pkg = root_pkg.exists() || frontend_pkg.exists() || backend_pkg.exists();
    if !has_any_node_pkg {
        warn!(
            target: "hatch_pipeline",
            "HATCH_LOCAL_LOOP=1 but no frontend/backend/root package.json in {}; skipping local loop",
            output_dir.display()
        );
        return Ok(());
    }

    println!("{}", "── Local build/fix loop ──".cyan().bold());
    let force_install = truthy_env("HATCH_NPM_FORCE_INSTALL");
    let mut install_dirs: Vec<PathBuf> = Vec::new();
    if output_dir.join("package.json").exists() {
        install_dirs.push(output_dir.clone());
    }
    let frontend_pkg = output_dir.join("frontend/package.json");
    if frontend_pkg.exists() {
        install_dirs.push(output_dir.join("frontend"));
    }
    let backend_pkg = output_dir.join("backend/package.json");
    if backend_pkg.exists() {
        install_dirs.push(output_dir.join("backend"));
    }

    install_dirs.sort();
    install_dirs.dedup();

    for dir in install_dirs {
        let nm = dir.join("node_modules");
        if force_install || !nm.exists() {
            info!(target: "hatch_pipeline", "npm install in {}", dir.display());
            run_and_expect_success(&dir, "npm", &["install"]).await?;
        } else {
            info!(
                target: "hatch_pipeline",
                "skipping npm install in {} (node_modules exists)",
                dir.display()
            );
        }
    }

    let max_attempts = local_fix_max_attempts();
    for attempt in 1..=max_attempts {
        let build = run_fullstack_build_capture(output_dir, intent).await?;
        if build.success {
            println!(
                "{} {}",
                "build:".green().bold(),
                "ok".green()
            );
            break;
        }

        println!(
            "{} {} / {}",
            "build failed; auto-fixing attempt".yellow().bold(),
            attempt,
            max_attempts
        );

        let fixes = request_log_based_fix(llm, model, intent, output_dir, &build).await?;
        if fixes.is_empty() {
            return Err(anyhow::anyhow!(
                "auto-fix returned no file patches. logs:\n{}",
                build.combined
            ));
        }
        apply_fixes(output_dir, &fixes)?;

        if attempt == max_attempts {
            let final_build =
                run_fullstack_build_capture(output_dir, intent).await?;
            if !final_build.success {
                return Err(anyhow::anyhow!(
                    "build still failing after {} attempts.\n{}",
                    max_attempts,
                    final_build.combined
                ));
            }
        }
    }

    maybe_serve_fullstack(output_dir).await?;

    Ok(())
}

struct CmdResult {
    success: bool,
    combined: String,
}

async fn run_capture(dir: &PathBuf, cmd: &str, args: &[&str]) -> anyhow::Result<CmdResult> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .with_context(|| format!("failed to run {} {:?}", cmd, args))?;
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&out.stdout));
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(CmdResult {
        success: out.status.success(),
        combined,
    })
}

async fn run_capture_with_env(
    dir: &PathBuf,
    cmd: &str,
    args: &[&str],
    envs: &[(&str, String)],
) -> anyhow::Result<CmdResult> {
    let mut c = Command::new(cmd);
    c.args(args).current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    let out = c
        .output()
        .await
        .with_context(|| format!("failed to run {} {:?}", cmd, args))?;

    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&out.stdout));
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(CmdResult {
        success: out.status.success(),
        combined,
    })
}

async fn run_and_expect_success(dir: &PathBuf, cmd: &str, args: &[&str]) -> anyhow::Result<()> {
    let res = run_capture(dir, cmd, args).await?;
    if res.success {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "command failed: {} {:?}\n{}",
            cmd,
            args,
            res.combined
        ))
    }
}

#[derive(Debug, serde::Deserialize)]
struct FixResponse {
    artifacts: Vec<FixArtifact>,
}

#[derive(Debug, serde::Deserialize)]
struct FixArtifact {
    name: String,
    content: String,
}

async fn request_log_based_fix(
    llm: &SharedLlm,
    model: &str,
    intent: &str,
    output_dir: &PathBuf,
    build: &CmdResult,
) -> anyhow::Result<Vec<FixArtifact>> {
    let candidate_files = collect_candidate_files(output_dir, &build.combined)?;
    const MAX_FILE_CHARS: usize = 25_000;
    const MAX_BUNDLE_CHARS: usize = 90_000;

    let mut file_bundle = String::new();
    let mut used = 0usize;
    for p in &candidate_files {
        let rel = p.strip_prefix(output_dir).unwrap_or(p.as_path());
        let mut content = fs::read_to_string(p).unwrap_or_default();
        if content.len() > MAX_FILE_CHARS {
            content.truncate(MAX_FILE_CHARS);
            content.push_str("\n/* truncated */\n");
        }

        let chunk = format!("FILE: {}\n{}\n", rel.display(), content);
        // Keep prompt size bounded.
        if used + chunk.len() > MAX_BUNDLE_CHARS && !file_bundle.is_empty() {
            break;
        }
        file_bundle.push_str(&chunk);
        used += chunk.len();
        if !file_bundle.ends_with("\n---\n") && used < MAX_BUNDLE_CHARS {
            file_bundle.push_str("\n---\n");
        }
    }

    let system = r#"You are a senior software build fixer.
Return ONLY JSON:
{
  "artifacts": [
    { "name": "relative/path/to/file", "content": "full new file content" }
  ]
}
Rules:
- Patch only files that are necessary to fix build errors.
- No markdown or explanation.
- Keep paths relative."#;

    let user = format!(
        "Project intent:\n{intent}\n\nBuild errors:\n{}\n\nCandidate files (may be truncated):\n{}",
        build.combined, file_bundle
    );

    let req = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: MessageRole::User,
            content: user,
        }],
        max_tokens: Some(2048),
        temperature: Some(0.0),
        system: Some(system.to_string()),
    };

    let resp = llm.complete(req).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let raw = resp.content.trim();
    let json = strip_json_fences(raw);
    let parsed: FixResponse = serde_json::from_str(json)
        .with_context(|| format!("invalid fix JSON: {}", raw))?;
    Ok(parsed.artifacts)
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

fn collect_candidate_files(output_dir: &PathBuf, logs: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut selected = BTreeSet::new();
    // Include both frontend (web) and backend (common) file types.
    let re = regex::Regex::new(
        r#"([A-Za-z0-9_\-./]+?\.(ts|tsx|js|jsx|json|css|html|rs|toml|py|go))"#,
    )?;
    for cap in re.captures_iter(logs) {
        if let Some(m) = cap.get(1) {
            let p = output_dir.join(m.as_str());
            if p.exists() {
                selected.insert(p);
            }
        }
    }

    let package_json = output_dir.join("package.json");
    if package_json.exists() {
        selected.insert(package_json);
    }
    let tsconfig = output_dir.join("tsconfig.json");
    if tsconfig.exists() {
        selected.insert(tsconfig);
    }

    if selected.is_empty() {
        // fallback: first-level src files (only common web extensions)
        let src = output_dir.join("src");
        if src.exists() {
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        let ext = ext.to_ascii_lowercase();
                        if matches!(
                            ext.as_str(),
                            "ts" | "tsx" | "js" | "jsx" | "json" | "css" | "html"
                        ) {
                            selected.insert(p);
                        }
                    }
                }
            }
        }
    }

    Ok(selected.into_iter().take(16).collect())
}

fn apply_fixes(output_dir: &PathBuf, fixes: &[FixArtifact]) -> anyhow::Result<()> {
    for fix in fixes {
        let rel = fix.name.trim();
        if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|s| s == "..") {
            continue;
        }
        let path = output_dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, fix.content.as_bytes())?;
        info!(target: "hatch_pipeline", "applied auto-fix to {}", path.display());
    }
    Ok(())
}

async fn planner_plan_streaming(
    intent: &str,
    model: &str,
    llm: &SharedLlm,
) -> anyhow::Result<ExecutionPlan> {
    let run_id = hatch_core::RunId::new_v4();
    let user = format!(
        "User intent:\n{intent}\n\nReturn ONLY valid JSON with this shape (no markdown):\n{{\"tasks\":[{{\"name\":\"...\",\"description\":\"...\",\"agent_type\":\"frontend|backend|devops|data|generic\",\"dependencies\":[]}}]}}"
    );
    let req = CompletionRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: MessageRole::User,
            content: user,
        }],
        max_tokens: Some(2048),
        temperature: Some(0.0),
        system: Some(hatch_planner::PLANNER_SYSTEM.to_string()),
    };

    let mut stream = llm.complete_stream(req).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let piece = chunk.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        buf.push_str(&piece);
        print!("{piece}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
    println!();
    hatch_planner::parse_execution_plan_from_llm_json(run_id, intent, &buf)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}
