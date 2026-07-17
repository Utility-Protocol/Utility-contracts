from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_DIR = ROOT / "deploy" / "service-mesh"


def manifest(name):
    return (MANIFEST_DIR / name).read_text(encoding="utf-8")


def test_strict_mtls_peer_authentication():
    text = manifest("mtls-policy.yaml")
    assert "kind: PeerAuthentication" in text
    assert "name: utility-contracts-strict-mtls" in text
    assert "mode: STRICT" in text


def test_all_destination_rules_use_istio_mutual_tls():
    text = manifest("destination-rules.yaml") + manifest("traffic-policy.yaml")
    assert text.count("kind: DestinationRule") >= 2
    assert text.count("mode: ISTIO_MUTUAL") >= 2


def test_virtual_service_preserves_latency_budget_and_blue_green_subsets():
    text = manifest("traffic-policy.yaml")
    assert "kind: VirtualService" in text
    assert "name: canary" in text
    assert "name: primary" in text
    assert "timeout: 100ms" in text
    assert "name: blue" in text
    assert "name: green" in text


def test_monitoring_covers_latency_availability_and_mtls_drift():
    text = manifest("monitoring.yaml")
    assert "alert: UtilityContractsMeshP99LatencyHigh" in text
    assert "alert: UtilityContractsMeshAvailabilityLow" in text
    assert "alert: UtilityContractsMTLSPolicyDrift" in text
