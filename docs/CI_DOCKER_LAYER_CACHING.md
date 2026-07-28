# CI Docker Layer Caching Runbook

## Architecture

CI uses `Dockerfile.ci` as a BuildKit cache boundary for the repository's runnable services:

- root Rust payload generator (`rust-root-test` target),
- Soroban utility contracts (`contracts-test` and `wasm-build` targets),
- meter simulator (`meter-simulator-test` target), and
- usage dashboard (`usage-dashboard-build` target).

Each target copies dependency manifests before source files so dependency layers are reused when application code changes. BuildKit cache mounts persist Cargo registries, Cargo git checkouts, Rust `target` directories, and npm package downloads within the layer graph. GitHub Actions stores and restores those layers with `type=gha` caches scoped per service target.

## CI behavior

The `docker-layer-cache` job builds every service target with `docker/build-push-action` and `push: false`. The job is intentionally non-publishing: it validates image buildability and warms cache metadata only. Existing native Rust jobs continue to produce test output and WASM artifacts.

## Monitoring and alerting

Use the GitHub Actions run summary for these operational checks:

1. The `docker-layer-cache` job should stay green on all pull requests.
2. Repeated cache misses are visible as unusually long BuildKit dependency steps such as `cargo fetch` or `npm install`.
3. A failed `docker-layer-cache` job blocks merges through required status checks when branch protection is enabled.

## Blue-green and canary rollout

Roll out cache changes by first opening a pull request and validating the new cache scopes on that PR. Treat the pull request branch as the canary. After merge, `main` becomes the green environment while previous workflow runs remain the blue fallback reference. If build times regress or cache keys poison, revert the workflow and `Dockerfile.ci` changes; GitHub's `type=gha` cache scopes are isolated by target name, so removing a target stops consuming the affected cache.

## Security review notes

- The Docker workflow does not push images or export secrets.
- `.dockerignore` excludes VCS metadata, local dependency folders, build outputs, coverage data, and binary artifacts from Docker build contexts.
- BuildKit cache mounts are used only for package manager caches and compiler outputs.
