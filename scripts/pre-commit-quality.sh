#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTRACTS_DIR="$ROOT_DIR/contracts"
DASHBOARD_DIR="$ROOT_DIR/usage-dashboard"
RUN_ALL=false
FILES=()

for arg in "$@"; do
  case "$arg" in
    --all) RUN_ALL=true ;;
    *) FILES+=("$arg") ;;
  esac
done

run_step() {
  local label="$1"
  shift

  printf '\n🔍 %s\n' "$label"
  "$@"
}

has_changed_file() {
  local pattern="$1"
  local file

  "$RUN_ALL" && return 0
  for file in "${FILES[@]}"; do
    [[ "$file" == $pattern ]] && return 0
  done
  return 1
}

if [[ ! -d "$CONTRACTS_DIR" ]]; then
  echo "❌ contracts workspace not found at $CONTRACTS_DIR" >&2
  exit 1
fi

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-always}"

if has_changed_file "contracts/*.rs" || has_changed_file "contracts/*/src/*.rs" || has_changed_file "contracts/*/tests/*.rs" || has_changed_file "contracts/*/Cargo.toml" || has_changed_file "contracts/Cargo.toml"; then
  run_step "Rust formatting" \
    cargo fmt --manifest-path "$CONTRACTS_DIR/Cargo.toml" --all -- --check

  run_step "Rust clippy" \
    cargo clippy --manifest-path "$CONTRACTS_DIR/Cargo.toml" --all-targets --all-features -- -D warnings

  run_step "Rust tests" \
    cargo test --manifest-path "$CONTRACTS_DIR/Cargo.toml" --all-features
else
  printf '\nℹ️  Skipping Rust checks because this commit does not change Rust workspace files.\n'
fi

if has_changed_file "usage-dashboard/*.js" || has_changed_file "usage-dashboard/*.jsx" || has_changed_file "usage-dashboard/*.ts" || has_changed_file "usage-dashboard/*.tsx" || has_changed_file "usage-dashboard/package.json"; then
  if [[ -d "$DASHBOARD_DIR/node_modules" ]]; then
    run_step "Usage dashboard lint" \
      npm --prefix "$DASHBOARD_DIR" run lint
  else
    printf '\n⚠️  Skipping usage dashboard lint because %s is missing. Run npm install in usage-dashboard to enable it.\n' \
      "$DASHBOARD_DIR/node_modules"
  fi
else
  printf '\nℹ️  Skipping usage dashboard lint because this commit does not change dashboard files.\n'
fi

printf '\n✅ Pre-commit quality suite passed.\n'
