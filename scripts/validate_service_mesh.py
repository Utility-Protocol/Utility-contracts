#!/usr/bin/env python3
"""Validate Utility Contracts service mesh manifests without cluster access."""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_DIR = ROOT / "deploy" / "service-mesh"
REQUIRED_FILES = (
    "mtls-policy.yaml",
    "destination-rules.yaml",
    "traffic-policy.yaml",
    "monitoring.yaml",
)


def read_manifest(name: str) -> str:
    return (MANIFEST_DIR / name).read_text(encoding="utf-8")


def require(pattern: str, text: str, message: str) -> None:
    if not re.search(pattern, text, re.MULTILINE):
        raise AssertionError(message)


def assert_strict_mtls(text: str) -> None:
    require(r"kind:\s*PeerAuthentication", text, "PeerAuthentication is required")
    require(r"name:\s*utility-contracts-strict-mtls", text, "strict mTLS policy must be named")
    require(r"mode:\s*STRICT", text, "PeerAuthentication must enforce STRICT mTLS")
    require(r"kind:\s*AuthorizationPolicy", text, "AuthorizationPolicy is required")
    require(r"action:\s*ALLOW", text, "authorization policy must be an explicit allowlist")


def assert_destination_rules(text: str) -> None:
    require(r"kind:\s*DestinationRule", text, "default DestinationRule is required")
    require(r"host:\s*\"\*\.utility-contracts\.svc\.cluster\.local\"", text, "default host wildcard is required")
    require(r"mode:\s*ISTIO_MUTUAL", text, "DestinationRule must originate ISTIO_MUTUAL TLS")
    require(r"connectTimeout:\s*100ms", text, "connect timeout must preserve the 100ms P99 budget")
    require(r"outlierDetection:", text, "outlier detection must be configured")


def assert_blue_green_and_canary(text: str) -> None:
    require(r"kind:\s*VirtualService", text, "VirtualService is required")
    require(r"name:\s*canary", text, "canary route is required")
    require(r"x-canary:", text, "canary header match is required")
    require(r"name:\s*primary", text, "primary route is required")
    require(r"timeout:\s*100ms", text, "routes must cap request timeout at 100ms")
    require(r"name:\s*blue", text, "blue subset is required")
    require(r"name:\s*green", text, "green subset is required")
    require(r"mode:\s*ISTIO_MUTUAL", text, "subset DestinationRule must use ISTIO_MUTUAL")


def assert_monitoring_rules(text: str) -> None:
    require(r"kind:\s*PrometheusRule", text, "PrometheusRule is required")
    for alert in (
        "UtilityContractsMeshP99LatencyHigh",
        "UtilityContractsMeshAvailabilityLow",
        "UtilityContractsMTLSPolicyDrift",
    ):
        require(rf"alert:\s*{alert}", text, f"{alert} alert is required")


def main() -> int:
    for name in REQUIRED_FILES:
        if not (MANIFEST_DIR / name).is_file():
            raise FileNotFoundError(MANIFEST_DIR / name)
    assert_strict_mtls(read_manifest("mtls-policy.yaml"))
    assert_destination_rules(read_manifest("destination-rules.yaml"))
    assert_blue_green_and_canary(read_manifest("traffic-policy.yaml"))
    assert_monitoring_rules(read_manifest("monitoring.yaml"))
    print("service mesh manifest validation passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, FileNotFoundError) as error:
        print(f"service mesh manifest validation failed: {error}", file=sys.stderr)
        raise SystemExit(1)
