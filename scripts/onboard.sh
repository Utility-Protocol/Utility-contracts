#!/usr/bin/env bash
# Developer onboarding script for Utility-contracts local development.
# It verifies required tools, installs safe local dependencies, and prints next steps.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECK_ONLY=false
SKIP_NPM=false
SKIP_RUST_TARGET=false

usage() {
  cat <<USAGE
Usage: scripts/onboard.sh [options]

Options:
  --check-only        Verify prerequisites without installing dependencies.
  --skip-npm         Do not run npm install in JavaScript workspaces.
  --skip-rust-target Do not install the wasm32-unknown-unknown Rust target.
  -h, --help         Show this help message.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check-only) CHECK_ONLY=true ;;
    --skip-npm) SKIP_NPM=true ;;
    --skip-rust-target) SKIP_RUST_TARGET=true ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 2 ;;
  esac
  shift
done

info() { printf 'ℹ️  %s\n' "$*"; }
success() { printf '✅ %s\n' "$*"; }
warn() { printf '⚠️  %s\n' "$*"; }
fail() { printf '❌ %s\n' "$*" >&2; }

require_command() {
  local cmd="$1"
  local hint="$2"
  if command -v "$cmd" >/dev/null 2>&1; then
    success "Found $cmd ($(command -v "$cmd"))"
  else
    fail "Missing $cmd. $hint"
    return 1
  fi
}

version_ge() {
  local current="$1"
  local required="$2"
  [[ "$(printf '%s\n' "$required" "$current" | sort -V | head -n1)" == "$required" ]]
}

check_node_version() {
  require_command node "Install Node.js 16 or newer." || return 1
  local node_version
  node_version="$(node --version | sed 's/^v//')"
  if version_ge "$node_version" "16.0.0"; then
    success "Node.js $node_version satisfies >=16.0.0"
  else
    fail "Node.js $node_version is too old; install Node.js 16 or newer."
    return 1
  fi
}

check_rust_version() {
  require_command rustc "Install Rust via https://rustup.rs/." || return 1
  require_command cargo "Install Cargo via https://rustup.rs/." || return 1
  local rust_version
  rust_version="$(rustc --version | awk '{print $2}')"
  if version_ge "$rust_version" "1.70.0"; then
    success "Rust $rust_version satisfies >=1.70.0"
  else
    fail "Rust $rust_version is too old; install Rust 1.70 or newer."
    return 1
  fi
}

install_rust_target() {
  if rustup target list --installed | rg -q '^wasm32-unknown-unknown$'; then
    success "Rust WASM target is installed"
  elif [[ "$CHECK_ONLY" == true || "$SKIP_RUST_TARGET" == true ]]; then
    warn "Rust WASM target wasm32-unknown-unknown is not installed"
  else
    info "Installing Rust WASM target wasm32-unknown-unknown"
    rustup target add wasm32-unknown-unknown
  fi
}

install_npm_workspace() {
  local workspace="$1"
  local package_json="$ROOT_DIR/$workspace/package.json"
  [[ -f "$package_json" ]] || return 0

  if [[ "$CHECK_ONLY" == true || "$SKIP_NPM" == true ]]; then
    if [[ -d "$ROOT_DIR/$workspace/node_modules" ]]; then
      success "$workspace dependencies are installed"
    else
      warn "$workspace dependencies are not installed"
    fi
    return 0
  fi

  info "Installing npm dependencies in $workspace"
  (cd "$ROOT_DIR/$workspace" && npm install)
}

create_env_file() {
  local workspace="$1"
  local example="$ROOT_DIR/$workspace/.env.example"
  local env_file="$ROOT_DIR/$workspace/.env"
  [[ -f "$example" ]] || return 0
  if [[ -f "$env_file" ]]; then
    success "$workspace/.env already exists"
  elif [[ "$CHECK_ONLY" == true ]]; then
    warn "$workspace/.env is missing; copy from .env.example before running services"
  else
    cp "$example" "$env_file"
    success "Created $workspace/.env from .env.example"
  fi
}

main() {
  cd "$ROOT_DIR"
  info "Starting Utility-contracts local onboarding from $ROOT_DIR"

  require_command git "Install Git from https://git-scm.com/."
  require_command rg "Install ripgrep for fast repository searches."
  check_rust_version
  require_command rustup "Install rustup from https://rustup.rs/."
  install_rust_target
  check_node_version
  require_command npm "Install npm with Node.js."

  install_npm_workspace meter-simulator
  install_npm_workspace usage-dashboard
  create_env_file meter-simulator
  chmod +x scripts/onboard.sh meter-simulator/scripts/setup.sh scripts/deploy.sh 2>/dev/null || true

  info "Recommended validation commands:"
  printf '  cargo fmt --all -- --check\n'
  printf '  cargo test\n'
  printf '  (cd meter-simulator && npm test)\n'

  success "Local onboarding completed"
}

main "$@"
