# HATCH workspace — common Cargo tasks.
# Run from repo root: cd .../hatch && make <target>
#
# Examples:
#   make chat
#   make run INTENT="build a REST API for todos"
#   make plan INTENT="small CLI calculator"

CARGO       ?= cargo
CLIPPY_FLAGS ?= --workspace --all-targets -- -D warnings

.PHONY: help build test clippy fmt check clean \
	start chat run plan agents example install-hatch

help:
	@echo "HATCH — make targets"
	@echo ""
	@echo "  make build          cargo build --workspace"
	@echo "  make test           cargo test --workspace"
	@echo "  make clippy         cargo clippy (deny warnings)"
	@echo "  make fmt            cargo fmt --all"
	@echo "  make check          build + clippy"
	@echo "  make clean          cargo clean"
	@echo ""
	@echo "  make start          planner chat REPL (hatch chat)"
	@echo "  make chat           alias of make start"
	@echo "  make run INTENT=\"…\"   hatch run (needs OPENAI_API_KEY or Ollama)"
	@echo "  make plan INTENT=\"…\"  hatch plan (JSON only)"
	@echo "  make agents         list agent templates"
	@echo "  make example        cargo run -p build_todo_app"
	@echo "  make install-hatch  cargo install --path crates/hatch-cli"
	@echo ""
	@echo "Optional: HATCH_DEFAULT_MODEL=… HATCH_AGENTS_DIR=… RUST_LOG=hatch=debug"

build:
	$(CARGO) build --workspace

test:
	$(CARGO) test --workspace

clippy:
	$(CARGO) clippy $(CLIPPY_FLAGS)

fmt:
	$(CARGO) fmt --all

check: build clippy

clean:
	$(CARGO) clean

# --- Binaries (run from repo root so ./agents resolves) ---

start:
	$(CARGO) run -p hatch-cli -- chat

chat: start

run:
	@test -n "$(INTENT)" || (echo 'Set INTENT, e.g. make run INTENT="your goal"' >&2; exit 1)
	$(CARGO) run -p hatch-cli -- run "$(INTENT)"

plan:
	@test -n "$(INTENT)" || (echo 'Set INTENT, e.g. make plan INTENT="your goal"' >&2; exit 1)
	$(CARGO) run -p hatch-cli -- plan "$(INTENT)"

agents:
	$(CARGO) run -p hatch-cli -- agents

example:
	$(CARGO) run -p build_todo_app

install-hatch:
	$(CARGO) install --path crates/hatch-cli
