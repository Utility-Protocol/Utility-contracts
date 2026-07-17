# Pre-Commit Quality Suite

This repository includes a pre-commit hook suite that runs the same quality gates developers are expected to satisfy before opening a pull request.

## Architecture

The suite is intentionally local and deterministic:

1. `.pre-commit-config.yaml` registers a single repository-local hook.
2. `scripts/pre-commit-quality.sh` executes the quality gates from the repository root.
3. The hook receives changed files from pre-commit and runs only the checks relevant to those paths.
4. Rust checks target the `contracts/Cargo.toml` workspace so the hook works even when invoked from another directory.
5. Usage dashboard linting runs only when `usage-dashboard/node_modules` is present, avoiding network installs inside commit hooks.

## Quality Gates

For changed Rust workspace files, the hook enforces:

- `cargo fmt --all -- --check` for canonical Rust formatting.
- `cargo clippy --all-targets --all-features -- -D warnings` for warning-free Rust linting.
- `cargo test --all-features` for workspace tests.

For changed usage dashboard files, the hook enforces `npm run lint` when dependencies are installed.

## Installation

Install pre-commit once, then enable the repository hooks:

```bash
pipx install pre-commit
pre-commit install
```

Developers who already manage Python tools with another package manager can install `pre-commit` through that workflow instead.

## Manual Execution

Run the suite without creating a commit:

```bash
pre-commit run --all-files
```

Or run the underlying script directly:

```bash
scripts/pre-commit-quality.sh --all
```

## Operational Notes

- The hook performs local validation only; production availability, monitoring dashboards, alerting, blue-green deployment, and canary analysis remain CI/CD and operations concerns.
- Security review is supported by making formatting, linting, and test failures visible before code reaches review.
- The hook does not install dependencies or modify source files, keeping commit latency predictable and avoiding hidden network work.
