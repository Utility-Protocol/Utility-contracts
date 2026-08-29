#!/usr/bin/env bash
#
# Automated Performance Regression Gate
#
# Runs the webhook-ingestion benchmark and compares the fresh results against the committed
# baseline (.perf/perf-baselines.json). Exits non-zero when a metric regresses beyond the
# configured tolerance or breaches an absolute SLO (p99 < 100ms).
#
# Usage:
#   ./scripts/perf-regression-gate.sh              # run + compare (CI gate)
#   ./scripts/perf-regression-gate.sh --update-baseline   # refresh baseline after a verified improvement
#
# Environment:
#   PERF_SAMPLES   measured requests per run        (default: 800)
#   PERF_WARMUP    discarded warmup requests        (default: 200)
#   PERF_TOLERANCE regression tolerance percent     (default: 30)
#   PERF_STRICT_WARN fail on warnings too           (default: off)
#
# Exit codes:
#   0  all metrics within baseline / SLO
#   1  regression or SLO breach detected
#   2  warnings only and PERF_STRICT_WARN=1
#   3  harness error (missing deps, build failure, ...)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SERVICE_DIR="${REPO_ROOT}/webhook-delivery-service"
BASELINE="${REPO_ROOT}/.perf/perf-baselines.json"
RESULTS_DIR="${REPO_ROOT}/.perf/results"
RESULTS_FILE="${RESULTS_DIR}/latest.json"

PERF_SAMPLES="${PERF_SAMPLES:-800}"
PERF_WARMUP="${PERF_WARMUP:-200}"
PERF_TOLERANCE="${PERF_TOLERANCE:-30}"

UPDATE_BASELINE=0
STRICT_WARN=0
for arg in "$@"; do
  case "${arg}" in
    --update-baseline) UPDATE_BASELINE=1 ;;
    --strict-warn) STRICT_WARN=1 ;;
    --help|-h)
      sed -n '2,20p' "${BASH_SOURCE[0]}"
      exit 0
      ;;
    *) echo "Unknown argument: ${arg}" >&2; exit 3 ;;
  esac
done

echo "::group::Performance regression gate"
echo "Suite:       webhook-ingestion"
echo "Samples:     ${PERF_SAMPLES} (warmup ${PERF_WARMUP})"
echo "Tolerance:   ${PERF_TOLERANCE}%"
echo "Baseline:    ${BASELINE}"
echo

# 1. Dependencies ---------------------------------------------------------------
if ! command -v node >/dev/null 2>&1; then
  echo "error: node is required to run the performance gate" >&2
  exit 3
fi
if [ ! -d "${SERVICE_DIR}/node_modules" ]; then
  echo "Installing webhook-delivery-service dependencies..."
  (cd "${SERVICE_DIR}" && npm ci)
fi
if [ ! -f "${BASELINE}" ]; then
  echo "error: no baseline found at ${BASELINE}" >&2
  exit 3
fi

# 2. Build the service (compiles the benchmark into dist/perf/) ------------------
echo "Building webhook-delivery-service..."
(cd "${SERVICE_DIR}" && npm run build) || {
  echo "error: build failed" >&2
  exit 3
}

# 3. Run the benchmark -----------------------------------------------------------
echo "Running benchmark (${PERF_SAMPLES} samples, ${PERF_WARMUP} warmup)..."
mkdir -p "${RESULTS_DIR}"
(cd "${SERVICE_DIR}" && node dist/perf/benchmark.js \
  --samples "${PERF_SAMPLES}" \
  --warmup "${PERF_WARMUP}" \
  --out "${RESULTS_FILE}")
echo

# 4. Compare against baseline ----------------------------------------------------
COMPARE_ARGS=(--current "${RESULTS_FILE}" --baseline "${BASELINE}" --tolerance "${PERF_TOLERANCE}")
if [ "${UPDATE_BASELINE}" = "1" ]; then
  COMPARE_ARGS+=(--update-baseline)
fi
if [ "${STRICT_WARN}" = "1" ]; then
  COMPARE_ARGS+=(--strict-warn)
fi

set +e
(cd "${SERVICE_DIR}" && node dist/perf/compare-results.js "${COMPARE_ARGS[@]}")
COMPARE_EXIT=$?
set -e

if [ "${UPDATE_BASELINE}" = "1" ] && [ "${COMPARE_EXIT}" = "0" ]; then
  echo "✅ Baseline refreshed at ${BASELINE}"
fi

echo "::endgroup::"
exit "${COMPARE_EXIT}"
