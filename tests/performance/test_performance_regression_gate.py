import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "scripts"))

from performance_regression_gate import evaluate_regressions


class PerformanceRegressionGateTests(unittest.TestCase):
    def test_passes_when_current_snapshot_is_within_budget(self):
        baseline = {"critical_paths": {"settlement": {"p99_ms": 80}}}
        current = {"critical_paths": {"settlement": {"p99_ms": 86}}}

        results = evaluate_regressions(baseline, current, max_p99_ms=100, regression_percent=10)

        self.assertTrue(results[0].passed)
        self.assertEqual(results[0].reason, "within performance budget")

    def test_fails_when_hard_p99_slo_is_exceeded(self):
        baseline = {"critical_paths": {"settlement": {"p99_ms": 80}}}
        current = {"critical_paths": {"settlement": {"p99_ms": 101}}}

        results = evaluate_regressions(baseline, current, max_p99_ms=100, regression_percent=50)

        self.assertFalse(results[0].passed)
        self.assertIn("hard SLO", results[0].reason)

    def test_fails_when_regression_budget_is_exceeded(self):
        baseline = {"critical_paths": {"settlement": {"p99_ms": 80}}}
        current = {"critical_paths": {"settlement": {"p99_ms": 90}}}

        results = evaluate_regressions(baseline, current, max_p99_ms=100, regression_percent=10)

        self.assertFalse(results[0].passed)
        self.assertIn("regression", results[0].reason)

    def test_requires_all_baseline_critical_paths_in_current_snapshot(self):
        baseline = {"critical_paths": {"settlement": {"p99_ms": 80}}}
        current = {"critical_paths": {"oracle": {"p99_ms": 30}}}

        with self.assertRaisesRegex(ValueError, "missing critical paths"):
            evaluate_regressions(baseline, current)

    def test_cli_writes_markdown_report_and_returns_failure(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            baseline = temp_path / "baseline.json"
            current = temp_path / "current.json"
            report = temp_path / "report.md"
            baseline.write_text(json.dumps({"critical_paths": {"settlement": {"p99_ms": 80}}}))
            current.write_text(json.dumps({"critical_paths": {"settlement": {"p99_ms": 101}}}))

            completed = subprocess.run(
                [
                    sys.executable,
                    str(REPO_ROOT / "scripts" / "performance_regression_gate.py"),
                    "--baseline",
                    str(baseline),
                    "--current",
                    str(current),
                    "--report",
                    str(report),
                ],
                check=False,
                text=True,
                capture_output=True,
            )

            self.assertEqual(completed.returncode, 1)
            self.assertIn("settlement", report.read_text())


if __name__ == "__main__":
    unittest.main()
