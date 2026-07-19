#!/usr/bin/env python3
"""CI gate for critical-path P99 performance regressions.

The gate compares a current benchmark/monitoring snapshot against a committed
baseline and fails when any critical path exceeds either the hard SLO or the
allowed regression budget. Input format is intentionally small JSON so Rust,
JavaScript, load-test, or observability exporters can all feed the same gate.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

DEFAULT_MAX_P99_MS = 100.0
DEFAULT_REGRESSION_PERCENT = 10.0


@dataclass(frozen=True)
class PathResult:
    name: str
    baseline_p99_ms: float
    current_p99_ms: float
    regression_percent: float
    allowed_p99_ms: float
    passed: bool
    reason: str


def _load_json(path: Path) -> dict[str, Any]:
    try:
        with path.open(encoding="utf-8") as handle:
            payload = json.load(handle)
    except FileNotFoundError as exc:
        raise ValueError(f"{path} does not exist") from exc
    except json.JSONDecodeError as exc:
        raise ValueError(f"{path} is not valid JSON: {exc}") from exc

    if not isinstance(payload, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return payload


def _extract_critical_paths(payload: dict[str, Any], label: str) -> dict[str, float]:
    raw_paths = payload.get("critical_paths")
    if not isinstance(raw_paths, dict) or not raw_paths:
        raise ValueError(f"{label} must define a non-empty critical_paths object")

    paths: dict[str, float] = {}
    for name, metrics in raw_paths.items():
        if not isinstance(name, str) or not name:
            raise ValueError(f"{label} contains an invalid critical path name")
        if not isinstance(metrics, dict) or "p99_ms" not in metrics:
            raise ValueError(f"{label}.{name} must define p99_ms")
        value = metrics["p99_ms"]
        if not isinstance(value, (int, float)) or isinstance(value, bool) or value < 0:
            raise ValueError(f"{label}.{name}.p99_ms must be a non-negative number")
        paths[name] = float(value)
    return paths


def evaluate_regressions(
    baseline: dict[str, Any],
    current: dict[str, Any],
    max_p99_ms: float = DEFAULT_MAX_P99_MS,
    regression_percent: float = DEFAULT_REGRESSION_PERCENT,
) -> list[PathResult]:
    """Return per-critical-path pass/fail results for the performance gate."""
    if max_p99_ms <= 0:
        raise ValueError("max_p99_ms must be greater than zero")
    if regression_percent < 0:
        raise ValueError("regression_percent must be zero or greater")

    baseline_paths = _extract_critical_paths(baseline, "baseline")
    current_paths = _extract_critical_paths(current, "current")

    missing = sorted(set(baseline_paths) - set(current_paths))
    if missing:
        raise ValueError(f"current snapshot is missing critical paths: {', '.join(missing)}")

    results: list[PathResult] = []
    for name in sorted(baseline_paths):
        base = baseline_paths[name]
        curr = current_paths[name]
        allowed = min(max_p99_ms, base * (1 + regression_percent / 100.0))
        delta = 0.0 if base == 0 and curr == 0 else 100.0 if base == 0 else ((curr - base) / base) * 100.0
        if curr > max_p99_ms:
            passed = False
            reason = f"P99 {curr:.2f}ms exceeds hard SLO {max_p99_ms:.2f}ms"
        elif curr > allowed:
            passed = False
            reason = f"P99 regression {delta:.2f}% exceeds budget {regression_percent:.2f}%"
        else:
            passed = True
            reason = "within performance budget"
        results.append(PathResult(name, base, curr, delta, allowed, passed, reason))
    return results


def _write_markdown(results: list[PathResult], output: Path) -> None:
    lines = [
        "# Performance Regression Report",
        "",
        "| Critical path | Baseline P99 | Current P99 | Delta | Allowed P99 | Status |",
        "| --- | ---: | ---: | ---: | ---: | --- |",
    ]
    for result in results:
        status = "PASS" if result.passed else f"FAIL: {result.reason}"
        lines.append(
            f"| {result.name} | {result.baseline_p99_ms:.2f}ms | {result.current_p99_ms:.2f}ms | "
            f"{result.regression_percent:.2f}% | {result.allowed_p99_ms:.2f}ms | {status} |"
        )
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Detect critical-path P99 performance regressions")
    parser.add_argument("--baseline", type=Path, required=True, help="Baseline JSON snapshot")
    parser.add_argument("--current", type=Path, required=True, help="Current JSON snapshot")
    parser.add_argument("--max-p99-ms", type=float, default=DEFAULT_MAX_P99_MS)
    parser.add_argument("--regression-percent", type=float, default=DEFAULT_REGRESSION_PERCENT)
    parser.add_argument("--report", type=Path, help="Optional markdown report path")
    args = parser.parse_args(argv)

    try:
        results = evaluate_regressions(
            _load_json(args.baseline),
            _load_json(args.current),
            args.max_p99_ms,
            args.regression_percent,
        )
    except ValueError as exc:
        print(f"performance gate configuration error: {exc}", file=sys.stderr)
        return 2

    if args.report:
        _write_markdown(results, args.report)

    for result in results:
        marker = "✅" if result.passed else "❌"
        print(f"{marker} {result.name}: {result.current_p99_ms:.2f}ms P99 ({result.reason})")
    return 0 if all(result.passed for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
