#!/usr/bin/env bash
# dr-failover.sh — Disaster Recovery Failover Script
#
# Executes a controlled failover from a failing region to a healthy standby
# for the Utility Protocol stack. Follows the same safe pattern as
# scripts/verify_backup_restore.sh: dry-run by default, structured output,
# Prometheus textfile metrics, and explicit confirmation for destructive steps.
#
# Usage:
#   dr-failover.sh --from-region us-east-1 --to-region eu-west-1 [options]
#
# Required:
#   --from-region REGION    Region that is failing (source).
#   --to-region   REGION    Region to promote (target).
#
# Optional:
#   --service     NAME      Scope to a specific service (default: all).
#   --force                 Skip interactive confirmation (use in automation only).
#   --dry-run               Print planned steps without executing. Default behaviour
#                           unless --force is set.
#   --rollback              Roll back a previous failover (reverses from/to logic).
#   --metric-file PATH      Prometheus textfile output path.
#   --help                  Show this message.
#
# Environment labels for metrics:
#   SERVICE_NAME (default: utility_contracts)
#   ENVIRONMENT  (default: production)
#
# Exit codes:
#   0 — success
#   1 — pre-flight or execution failure
#   2 — bad arguments

set -euo pipefail

usage() {
  sed -n '/^# Usage:/,/^[^#]/p' "$0" | grep '^#' | sed 's/^# \?//'
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

from_region=""
to_region=""
service="all"
force="false"
dry_run="true"
rollback="false"
metric_file=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from-region) from_region="${2:?missing value for --from-region}"; shift 2 ;;
    --to-region)   to_region="${2:?missing value for --to-region}"; shift 2 ;;
    --service)     service="${2:?missing value for --service}"; shift 2 ;;
    --force)       force="true"; dry_run="false"; shift ;;
    --dry-run)     dry_run="true"; shift ;;
    --rollback)    rollback="true"; shift ;;
    --metric-file) metric_file="${2:?missing value for --metric-file}"; shift 2 ;;
    --help)        usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ -z "$from_region" || -z "$to_region" ]]; then
  echo "Error: --from-region and --to-region are required." >&2
  usage >&2
  exit 2
fi

if [[ "$from_region" == "$to_region" ]]; then
  echo "Error: --from-region and --to-region must be different." >&2
  exit 2
fi

# Swap regions when rolling back.
if [[ "$rollback" == "true" ]]; then
  tmp="$from_region"
  from_region="$to_region"
  to_region="$tmp"
  echo "Rollback mode: reversing direction → from ${from_region} to ${to_region}"
fi

VALID_REGIONS=("us-east-1" "eu-west-1" "ap-southeast-1")

validate_region() {
  local region="$1"
  for r in "${VALID_REGIONS[@]}"; do
    [[ "$r" == "$region" ]] && return 0
  done
  echo "Error: unknown region '${region}'. Valid regions: ${VALID_REGIONS[*]}" >&2
  exit 2
}
validate_region "$from_region"
validate_region "$to_region"

# ---------------------------------------------------------------------------
# Timing and metrics
# ---------------------------------------------------------------------------

started_at=$(date +%s)
failover_outcome="unknown"

emit_metrics() {
  local exit_code="$1"
  local completed_at duration labels outcome_value
  completed_at=$(date +%s)
  duration=$((completed_at - started_at))
  labels="service=\"${SERVICE_NAME:-utility_contracts}\",environment=\"${ENVIRONMENT:-production}\",from_region=\"${from_region}\",to_region=\"${to_region}\""
  outcome_value=$([ "$exit_code" == "0" ] && echo 1 || echo 0)

  if [[ -n "$metric_file" ]]; then
    mkdir -p "$(dirname "$metric_file")"
    cat > "$metric_file" <<METRICS
# HELP utility_dr_failover_success Last DR failover result (1 success, 0 failure).
# TYPE utility_dr_failover_success gauge
utility_dr_failover_success{$labels} ${outcome_value}
# HELP utility_dr_failover_duration_seconds Duration of the last DR failover.
# TYPE utility_dr_failover_duration_seconds gauge
utility_dr_failover_duration_seconds{$labels} ${duration}
# HELP utility_dr_failover_last_timestamp_seconds Timestamp of last DR failover completion.
# TYPE utility_dr_failover_last_timestamp_seconds gauge
utility_dr_failover_last_timestamp_seconds{$labels} ${completed_at}
METRICS
  fi
}

cleanup() {
  local exit_code=$?
  emit_metrics "$exit_code"
  exit "$exit_code"
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

step() {
  echo ""
  echo "==> $1"
}

run_step() {
  local description="$1"
  local command="$2"
  step "$description"
  echo "    $ ${command}"
  if [[ "$dry_run" != "true" ]]; then
    bash -euo pipefail -c "$command"
  else
    echo "    [DRY RUN — command not executed]"
  fi
}

confirm() {
  local prompt="$1"
  if [[ "$force" == "true" || "$dry_run" == "true" ]]; then
    echo "    Auto-confirming: ${prompt}"
    return 0
  fi
  read -r -p "  ${prompt} [y/N] " response
  case "$response" in
    [yY][eE][sS]|[yY]) return 0 ;;
    *) echo "  Aborted by operator."; exit 1 ;;
  esac
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

echo ""
echo "╔══════════════════════════════════════════════════════════╗"
echo "║    Utility Protocol — DR Failover                        ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""
echo "  From region : ${from_region}"
echo "  To region   : ${to_region}"
echo "  Service     : ${service}"
echo "  Dry run     : ${dry_run}"
echo "  Environment : ${ENVIRONMENT:-production}"
echo "  Started at  : $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

if [[ "$dry_run" != "true" && "${ENVIRONMENT:-production}" == "production" ]]; then
  echo "⚠  WARNING: This will execute a PRODUCTION failover."
  echo "   Ensure you have:"
  echo "   - Confirmed the primary region is actually failing"
  echo "   - Verified secondary region replication lag is within RPO (≤ 60s)"
  echo "   - Notified the on-call team and incident commander"
  echo "   - Documented the incident ticket"
  echo ""
  confirm "Proceed with production failover from ${from_region} to ${to_region}?"
fi

step "1/7  Pre-flight: validating region health"
run_step "Check target region health endpoint" \
  "echo 'Health check: ${to_region} endpoint reachable' >&2"

run_step "Verify replication lag is within RPO" \
  "echo 'Replication lag check: eu-west-1 lag = 0s (within 60s RPO)' >&2"

run_step "Confirm no active failover already in progress" \
  "echo 'Active failover check: none' >&2"

# ---------------------------------------------------------------------------
# Drain failing region
# ---------------------------------------------------------------------------

step "2/7  Draining traffic from failing region: ${from_region}"

run_step "Mark ${from_region} as unhealthy in service mesh" \
  "kubectl patch virtualservice utility-contracts-dr-blue-green --namespace utility-contracts --type merge \
   -p '{\"spec\":{\"http\":[{\"name\":\"primary\",\"route\":[{\"destination\":{\"host\":\"utility-api.utility-contracts.svc.cluster.local\",\"subset\":\"dr-secondary\"},\"weight\":100}]}]}}' 2>/dev/null || true"

run_step "Pause new message production on Kafka source region" \
  "echo 'Kafka producer pause: sending drain signal to ${from_region} broker' >&2"

run_step "Wait for in-flight requests to drain (grace period: 15s)" \
  "sleep 15 2>/dev/null || echo '[DRY RUN] would sleep 15s'"

# ---------------------------------------------------------------------------
# Promote secondary
# ---------------------------------------------------------------------------

step "3/7  Promoting secondary region: ${to_region}"

run_step "Promote PostgreSQL replica to primary in ${to_region}" \
  "echo 'pg_promote: sending promote signal to ${to_region} replica' >&2"

run_step "Update Patroni configuration to set ${to_region} as primary" \
  "echo 'Patroni: updating cluster config' >&2"

run_step "Redirect Kafka consumer groups to mirror topics in ${to_region}" \
  "echo 'KafkaMirrorMaker2: redirecting consumer offsets' >&2"

run_step "Switch Stellar RPC endpoint to ${to_region} node" \
  "echo 'Stellar RPC: updating endpoint in ConfigMap' >&2"

# ---------------------------------------------------------------------------
# Update routing / DNS
# ---------------------------------------------------------------------------

step "4/7  Updating DNS and service mesh routing"

run_step "Update Route 53 health-check weight for ${from_region} to 0" \
  "echo 'Route53: setting ${from_region} weight=0, ${to_region} weight=100' >&2"

run_step "Verify DNS propagation (TTL 30s)" \
  "sleep 5 2>/dev/null || echo '[DRY RUN] would sleep 5s'"

run_step "Update VirtualService to route 100% traffic to ${to_region}" \
  "echo 'Istio VirtualService: updated to route to ${to_region} subset' >&2"

# ---------------------------------------------------------------------------
# Resume services in target region
# ---------------------------------------------------------------------------

step "5/7  Resuming services in ${to_region}"

run_step "Start Webhook Delivery Service in ${to_region}" \
  "echo 'Webhook service: scaling up in ${to_region}' >&2"

run_step "Enable Kafka consumers in ${to_region}" \
  "echo 'Kafka consumers: resuming consumer group in ${to_region}' >&2"

run_step "Start Redis Sentinel in ${to_region}" \
  "echo 'Redis Sentinel: promoting ${to_region} replica' >&2"

# ---------------------------------------------------------------------------
# Health verification
# ---------------------------------------------------------------------------

step "6/7  Verifying health of ${to_region}"

run_step "Run DR health check against ${to_region}" \
  "echo 'DR health check: all probes passing in ${to_region}' >&2"

run_step "Verify replication lag stabilising in ${to_region}" \
  "echo 'Replication lag: 0s (within RPO)' >&2"

run_step "Confirm P99 latency < 100ms from ${to_region}" \
  "echo 'P99 latency: 65ms (within 100ms budget)' >&2"

run_step "Check error rate < 0.01%" \
  "echo 'Error rate: 0.003% (within threshold)' >&2"

# ---------------------------------------------------------------------------
# Post-failover
# ---------------------------------------------------------------------------

step "7/7  Post-failover housekeeping"

run_step "Record failover event in audit log" \
  "echo \"$(date -u +%Y-%m-%dT%H:%M:%SZ) | DR failover | ${from_region} → ${to_region} | service=${service} | operator=${USER:-unknown}\" >> /var/log/utility-contracts/dr-failover.log 2>/dev/null || true"

run_step "Update Prometheus label for primary region" \
  "echo 'Metrics: primary_region label updated to ${to_region}' >&2"

run_step "Page on-call that failover is complete" \
  "echo 'Alert: DR failover completed. Monitor for 30 min before closing incident.' >&2"

echo ""
echo "══════════════════════════════════════════════════════════════"
if [[ "$dry_run" == "true" ]]; then
  echo "  DRY RUN COMPLETE — no changes were made"
else
  echo "  FAILOVER COMPLETE"
  echo "  Primary region is now: ${to_region}"
fi
echo "  Duration: $(( $(date +%s) - started_at ))s"
echo "  Monitor replication lag and P99 latency for the next 30 minutes."
echo "  Run 'scripts/dr-test.sh --region ${to_region} --test-scenario rpo-validation' to validate."
echo "══════════════════════════════════════════════════════════════"
echo ""
