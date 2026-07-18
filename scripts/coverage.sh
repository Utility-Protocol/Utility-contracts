#!/usr/bin/env bash
set -euo pipefail

COVERAGE_THRESHOLD="${COVERAGE_THRESHOLD:-80}"
CARGO_LLVM_COV_FLAGS=(--locked --all-features --workspace --fail-under-lines "${COVERAGE_THRESHOLD}")

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "cargo-llvm-cov is required. Install it with: cargo install cargo-llvm-cov --locked"
  exit 127
fi

echo "Enforcing line coverage threshold: ${COVERAGE_THRESHOLD}%"

echo "::group::Root package coverage"
cargo llvm-cov "${CARGO_LLVM_COV_FLAGS[@]}"
echo "::endgroup::"

echo "::group::Contracts workspace coverage"
cargo llvm-cov --manifest-path contracts/Cargo.toml "${CARGO_LLVM_COV_FLAGS[@]}"
echo "::endgroup::"
