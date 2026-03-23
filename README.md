# HATCH

[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)
[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-blue.svg)](#license)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](./CONTRIBUTING.md)

HATCH is a Rust multi-agent orchestration framework that turns natural language into a runnable software project.

It is built for a practical loop:

1. Understand user intent
2. Plan tasks (frontend/backend/devops/etc.)
3. Generate project files
4. Build locally
5. Auto-fix from logs when build fails
6. Run locally so users can see a real product

---

## Demo

### Terminal Flow (Planner Chat -> Run -> Build -> Serve)

> Replace with your own recording URL:
>
> - [Asciinema Demo](https://asciinema.org/)
> - [YouTube Walkthrough](https://www.youtube.com/)

```text
$ make start
hatch> build a SaaS landing page + API
hatch> /run
Choose where to create the generated project (base directory).
...
build failed; auto-fixing attempt 1 / 3
build: ok
serving local app at http://127.0.0.1:5173
```

### Generated Project Example

> Add screenshots/GIF here once you publish:
>
> - `docs/demo-dashboard.png`
> - `docs/demo-mobile.png`

---

## Why HATCH

- **Planner-driven architecture**: intent -> execution DAG
- **Multi-agent execution**: concurrent Tokio tasks per role
- **Typed event bus**: all major lifecycle signals are observable
- **Interactive terminal UX**: chat with the planner and run from the same CLI
- **Local-first delivery**: generate code, build, fix, and serve on your machine

---

## Core Capabilities

- **`hatch chat`** interactive planner session
  - Free-form conversation to refine idea
  - `/plan` to inspect decomposition
  - `/run` to execute and generate project
  - optional auto-run flow
- **Top-level folder approval (optional)** before writing artifacts
- **Local build/fix loop** for generated projects
  - runs local build
  - reads build logs
  - asks LLM for file-level patches
  - retries automatically
- **Fullstack-friendly serve mode**
  - supports frontend + backend layout
  - injects API env vars for frontend (`VITE_API_URL`, `REACT_APP_API_URL`, `NEXT_PUBLIC_API_URL`)

---

## Workspace Layout

- `crates/hatch-core` - shared types and errors
- `crates/hatch-llm` - LLM abstraction + OpenAI/Ollama providers
- `crates/hatch-bus` - async typed message bus
- `crates/hatch-agent` - `Agent` trait + generic agent runtime
- `crates/hatch-planner` - intent -> execution plan
- `crates/hatch-spawner` - plan -> Tokio task spawning
- `crates/hatch-supervisor` - collects outputs + materializes artifacts
- `crates/hatch-console` - rich terminal session output
- `crates/hatch-cli` - `hatch` binary (`run`, `plan`, `chat`, `agents`)
- `agents/*.toml` - agent templates
- `examples/build_todo_app` - end-to-end sample

---

## Prerequisites

- Rust (stable, edition 2021)
- One model backend:
  - **OpenAI** (`OPENAI_API_KEY`)
  - **Ollama** running locally (default: `http://127.0.0.1:11434`)

---

## Quick Start

```bash
cd /Users/cuongtran/Documents/hatch
cargo build --workspace
```

### Option A: Interactive planner chat

```bash
export OPENAI_API_KEY="sk-..."
make start
```

Inside chat:

- Talk normally to refine requirements
- `/plan` to view the generated plan
- `/run` to create and execute project generation

### Option B: One-shot run

```bash
export OPENAI_API_KEY="sk-..."
make run INTENT="Build a fullstack todo app with React frontend and REST API backend"
```

### Option C: Plan only

```bash
make plan INTENT="Build a personal finance tracker"
```

---

## CLI Reference

```bash
hatch run "<intent>" [--provider openai|ollama] [--model <model>] [--stream] [--show-plan-json] [--output-dir <base_dir>]
hatch plan "<intent>" [--provider openai|ollama] [--model <model>] [--stream]
hatch chat [--provider openai|ollama] [--model <model>]
hatch agents
```

> `--output-dir` is the base directory. HATCH writes generated files under `<output-dir>/<run_id>/...`.

---

## Environment Variables

### Model / Provider

- `OPENAI_API_KEY` - OpenAI API key
- `HATCH_DEFAULT_PROVIDER` - `openai` or `ollama` (default: `openai`)
- `HATCH_DEFAULT_MODEL` - default model name
- `HATCH_AGENTS_DIR` - path to agent templates (default: `./agents`)

### Generation / Output

- `HATCH_OUTPUT_DIR` - base directory for generated runs (default: `hatch_runs`)
- `HATCH_NO_PROMPT=1` - disable interactive prompt for output directory
- `HATCH_APPROVAL_TOP_LEVEL=1` - enable top-level folder approval before write (default: off)

### Local Build / Fix / Serve

- `HATCH_LOCAL_LOOP=1` - enable build + auto-fix loop
- `HATCH_FIX_MAX_ATTEMPTS` - max auto-fix retries (default: `2`)
- `HATCH_NPM_FORCE_INSTALL=1` - always run `npm install` even when `node_modules` exists
- `HATCH_SERVE=1` - run local dev server after successful build loop
- `HATCH_DEV_PORT` - frontend dev port (default: `5173`)
- `HATCH_BACKEND_PORT` - backend dev port (default: `3001`)

### Logging

- `RUST_LOG` - standard tracing filter (example: `hatch=debug,hatch_llm=debug`)

---

## Makefile Targets

```bash
make help
make build
make test
make clippy
make fmt
make check
make start        # planner chat
make run INTENT="..."
make plan INTENT="..."
make agents
make example
```

---

## Roadmap

### Near-Term (v0.1 -> v0.2)

- [ ] Fullstack reliability improvements (better backend build/test loop)
- [ ] More robust patch strategy for large repos
- [ ] Better plan visualization in terminal
- [ ] Config file support (`hatch.toml`) in addition to env vars
- [ ] CI pipeline + release workflow

### Mid-Term (v0.3)

- [ ] Capability-based agents (frontend/backend/devops/data/security profiles)
- [ ] Tool runtime for command execution and file ops
- [ ] Multi-provider model policy (cost/perf fallback)
- [ ] Optional remote execution workers

### Long-Term (v1.0 vision)

- [ ] Plugin ecosystem for custom agents/tools
- [ ] Hosted run traces and observability dashboard
- [ ] Team collaboration flow (share plan, approve, replay)
- [ ] Framework-level benchmarks against common app templates

---

## Notes for Contributors

- Prefer small, composable crates and explicit interfaces between planner/spawner/supervisor.
- Keep runtime behavior observable via structured logs and bus events.
- Avoid hidden side effects in library crates.
- Validate generated file paths defensively before writing to disk.

## Good First Issues

- Add better error classification in `hatch-core` (recoverable vs fatal)
- Add richer bus events for build/fix loop visibility
- Add integration tests for `hatch chat` and `/run` flow
- Improve template loading UX with validation hints
- Add benchmark script for generation + build time

If you are new to the project, open a GitHub issue with prefix **`good-first-issue:`** and we can help scope it.

---

## License

Dual-licensed under **MIT** or **Apache-2.0**.
