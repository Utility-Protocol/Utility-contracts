#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: verify_backup_restore.sh --backup PATH --restore-command COMMAND [options]

Verifies a scheduled database backup by restoring it into an isolated target and
running integrity probes. The script is database-agnostic: pass the restore and
probe commands used by your PostgreSQL, MySQL, SQLite, or managed-database tool.

Required:
  --backup PATH              Backup artifact to verify.
  --restore-command COMMAND  Command used to restore the artifact. The token
                             {backup} is replaced with the backup path and
                             {restore_dir} with a temporary restore directory.

Optional:
  --probe-command COMMAND    Command run after restore; may use {backup} and
                             {restore_dir}. Repeat for multiple probes.
  --metric-file PATH         Prometheus textfile output path.
  --manifest PATH            Expected SHA-256 manifest in "<sha>  <file>" form.
  --max-age-seconds N        Fail when the backup mtime is older than N seconds.
  --canary-percent N         Canary gate percentage (0-100). Defaults to 100.
  --dry-run                  Print planned actions without executing commands.
  --help                     Show this help.

Environment labels included in metrics:
  SERVICE_NAME (default: utility_contracts)
  ENVIRONMENT  (default: local)
USAGE
}

backup=""
restore_command=""
metric_file=""
manifest=""
max_age_seconds=""
canary_percent="100"
dry_run="false"
probe_commands=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --backup) backup="${2:?missing value for --backup}"; shift 2 ;;
    --restore-command) restore_command="${2:?missing value for --restore-command}"; shift 2 ;;
    --probe-command) probe_commands+=("${2:?missing value for --probe-command}"); shift 2 ;;
    --metric-file) metric_file="${2:?missing value for --metric-file}"; shift 2 ;;
    --manifest) manifest="${2:?missing value for --manifest}"; shift 2 ;;
    --max-age-seconds) max_age_seconds="${2:?missing value for --max-age-seconds}"; shift 2 ;;
    --canary-percent) canary_percent="${2:?missing value for --canary-percent}"; shift 2 ;;
    --dry-run) dry_run="true"; shift ;;
    --help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$backup" || -z "$restore_command" ]]; then
  usage >&2
  exit 2
fi

if [[ ! "$canary_percent" =~ ^[0-9]+$ ]] || (( canary_percent < 0 || canary_percent > 100 )); then
  echo "--canary-percent must be an integer from 0 to 100" >&2
  exit 2
fi

if [[ ! -f "$backup" ]]; then
  echo "Backup artifact not found: $backup" >&2
  exit 1
fi

started_at=$(date +%s)
restore_dir=""

emit_metrics() {
  local exit_code="$1"
  local completed_at duration labels
  completed_at=$(date +%s)
  duration=$((completed_at - started_at))
  labels="service=\"${SERVICE_NAME:-utility_contracts}\",environment=\"${ENVIRONMENT:-local}\""

  if [[ -n "$metric_file" ]]; then
    mkdir -p "$(dirname "$metric_file")"
    cat > "$metric_file" <<METRICS
# HELP utility_backup_restore_verification_success Backup restore verification result (1 success, 0 failure).
# TYPE utility_backup_restore_verification_success gauge
utility_backup_restore_verification_success{$labels} $([[ "$exit_code" == "0" ]] && echo 1 || echo 0)
# HELP utility_backup_restore_verification_duration_seconds Backup restore verification runtime.
# TYPE utility_backup_restore_verification_duration_seconds gauge
utility_backup_restore_verification_duration_seconds{$labels} $duration
# HELP utility_backup_restore_verification_last_timestamp_seconds Last verification completion timestamp.
# TYPE utility_backup_restore_verification_last_timestamp_seconds gauge
utility_backup_restore_verification_last_timestamp_seconds{$labels} $completed_at
METRICS
  fi
}

cleanup() {
  local exit_code=$?
  [[ -n "$restore_dir" && -d "$restore_dir" ]] && rm -rf "$restore_dir"
  emit_metrics "$exit_code"
  exit "$exit_code"
}
trap cleanup EXIT

render_command() {
  local template="$1"
  template="${template//\{backup\}/$backup}"
  template="${template//\{restore_dir\}/$restore_dir}"
  printf '%s' "$template"
}

run_step() {
  local description="$1"
  local command_template="$2"
  local rendered
  rendered=$(render_command "$command_template")
  echo "==> $description"
  echo "    $rendered"
  if [[ "$dry_run" != "true" ]]; then
    bash -euo pipefail -c "$rendered"
  fi
}

if [[ "$canary_percent" == "0" ]]; then
  echo "Canary percentage is 0; skipping restore verification by policy."
  exit 0
fi

if [[ -n "$max_age_seconds" ]]; then
  if [[ ! "$max_age_seconds" =~ ^[0-9]+$ ]]; then
    echo "--max-age-seconds must be a non-negative integer" >&2
    exit 2
  fi
  backup_mtime=$(stat -c %Y "$backup")
  backup_age=$((started_at - backup_mtime))
  if (( backup_age > max_age_seconds )); then
    echo "Backup age ${backup_age}s exceeds maximum ${max_age_seconds}s" >&2
    exit 1
  fi
fi

if [[ -n "$manifest" ]]; then
  if [[ ! -f "$manifest" ]]; then
    echo "Manifest not found: $manifest" >&2
    exit 1
  fi
  echo "==> Verifying SHA-256 manifest"
  sha256sum --check "$manifest"
fi

restore_dir=$(mktemp -d)
run_step "Restoring backup into isolated target" "$restore_command"

if [[ ${#probe_commands[@]} -eq 0 ]]; then
  echo "==> No probe commands supplied; restore command completion is the integrity check."
else
  for probe in "${probe_commands[@]}"; do
    run_step "Running integrity probe" "$probe"
  done
fi

echo "Backup restore verification completed successfully."
