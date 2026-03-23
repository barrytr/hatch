# HATCH

[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)
[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-blue.svg)](#license)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](./CONTRIBUTING.md)

**Turn a product idea into a running fullstack app on localhost with one command.**

HATCH plans, generates, builds, auto-fixes from logs, and serves locally.

---

## 10-Second Demo

> Add your GIF/video here to maximize conversion from visitors to users.
>
> - `docs/demo.gif`
> - [Asciinema](https://asciinema.org/)
> - [YouTube short](https://www.youtube.com/)

```text
$ make run INTENT="Build a fullstack todo app"
→ plans frontend + backend tasks
→ generates files
→ runs local build
→ if build fails: reads logs and patches files
→ serves app at http://127.0.0.1:5173
```

---

## Before vs After

### Before

- manually scaffold frontend and backend
- wire API URLs
- debug build breakage by hand
- run multiple commands across folders

### After

```bash
make run INTENT="Build a fullstack todo app with auth"
```

HATCH handles the rest locally.

---

## Why This Matters

Developers do not need another "AI chat that outputs code snippets".
They need a tool that **actually ships runnable output**.

HATCH focuses on that last mile:

- intent -> execution plan
- multi-agent generation
- local build loop
- auto-fix from real logs
- runnable app for immediate review

---

## Quick Start

```bash
cd /Users/cuongtran/Documents/hatch
cargo build --workspace
export OPENAI_API_KEY="sk-..."
export HATCH_LOCAL_LOOP=1
export HATCH_SERVE=1
make run INTENT="Build a fullstack todo app with React frontend and REST API backend"
```

Open `http://127.0.0.1:5173`.

### Interactive Planner Mode

```bash
make start
```

Inside chat:

- describe what you want
- run `/plan` to inspect decomposition
- run `/run` to generate + build + serve

---

## Core Capabilities

- **Planner-first workflow** (intent -> task DAG)
- **Concurrent role agents** (frontend/backend/devops/data)
- **Typed message bus** for observability
- **Filesystem materialization** with optional top-level approval
- **Local build/fix loop** that patches files using build logs
- **Fullstack dev serving** with frontend API env injection

---

## CLI

```bash
hatch run "<intent>" [--provider openai|ollama] [--model <model>] [--show-plan-json] [--output-dir <base_dir>]
hatch plan "<intent>" [--provider openai|ollama] [--model <model>] [--stream]
hatch chat [--provider openai|ollama] [--model <model>]
hatch agents
```

---

## Environment Variables

### Model / Provider

- `OPENAI_API_KEY`
- `HATCH_DEFAULT_PROVIDER` (`openai` or `ollama`)
- `HATCH_DEFAULT_MODEL`
- `HATCH_AGENTS_DIR`

### Generation / Output

- `HATCH_OUTPUT_DIR` (default: `hatch_runs`)
- `HATCH_NO_PROMPT=1`
- `HATCH_APPROVAL_TOP_LEVEL=1` (off by default)

### Build / Fix / Serve

- `HATCH_LOCAL_LOOP=1`
- `HATCH_FIX_MAX_ATTEMPTS` (default: `2`)
- `HATCH_NPM_FORCE_INSTALL=1`
- `HATCH_SERVE=1`
- `HATCH_DEV_PORT` (default: `5173`)
- `HATCH_BACKEND_PORT` (default: `3001`)

### Logging

- `RUST_LOG` (example: `hatch=debug,hatch_llm=debug`)

---

## Launch Assets

- [Launch Playbook](./docs/LAUNCH_PLAYBOOK.md)
- [Tweet Templates](./docs/TWEETS.md)
- [Post Templates (Reddit/HN/DEV)](./docs/POST_TEMPLATES.md)

---

## Roadmap

- stronger backend build/test auto-fix coverage
- tool-capability runtime (command/file/mcp)
- run traces and replay tooling
- plugin ecosystem for custom agents

---

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

## License

Dual-licensed under **MIT** or **Apache-2.0**.
