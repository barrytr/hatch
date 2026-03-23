# Contributing to HATCH

Thanks for your interest in contributing to HATCH.

This project aims to make local-first, multi-agent software generation reliable and developer-friendly. We welcome bug reports, ideas, documentation fixes, and code contributions.

## Quick Start

```bash
cd /Users/cuongtran/Documents/hatch
cargo build --workspace
make test
make clippy
```

If you are working on runtime behavior, also run:

```bash
make start
```

Then test the `/plan` and `/run` flow in `hatch chat`.

## Ways to Contribute

- Report bugs with clear repro steps
- Propose architecture improvements
- Improve docs and examples
- Add tests for planner, spawner, supervisor, and CLI flows
- Improve build/fix reliability for generated fullstack projects

## Development Workflow

1. Fork the repository
2. Create a feature branch
3. Make focused changes
4. Run checks locally:
   - `cargo fmt --all`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
5. Open a pull request with:
   - problem statement
   - approach
   - test evidence

## Contribution Guidelines

- Keep changes scoped and reviewable
- Avoid broad refactors in feature PRs
- Prefer explicit, typed interfaces across crates
- Preserve local-first behavior (no hidden remote side effects)
- Add or update tests when behavior changes

## Coding Standards

- Rust 2021 edition
- No `unwrap()` in library crates
- Use `tracing` for runtime visibility
- Return structured errors instead of panicking
- Validate generated file paths before writing to disk

## Pull Request Checklist

- [ ] Code compiles (`cargo build --workspace`)
- [ ] Formatting applied (`cargo fmt --all`)
- [ ] Lints pass (`cargo clippy ... -D warnings`)
- [ ] Tests pass (`cargo test --workspace`)
- [ ] README/docs updated when behavior changed
- [ ] PR description explains the why, not just the what

## Good First Issues

If you are new, look for tasks in README under **Good First Issues**.

If none are open yet, create an issue with title prefix:

`good-first-issue: <short topic>`

We will help scope it.

## Security

If you discover a security issue, please avoid posting details publicly first.
Open a private report via GitHub Security Advisories or contact maintainers directly.

## License

By contributing, you agree your contributions are licensed under the same terms as this repository (MIT OR Apache-2.0).
