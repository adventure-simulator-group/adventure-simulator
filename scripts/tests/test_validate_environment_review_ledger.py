import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "validate_environment_review_ledger.py"
SPEC = importlib.util.spec_from_file_location("validate_environment_review_ledger", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def ledger(item):
    return {"items": [item], "stop_decision": {"ready_to_stop": False, "reason": "work remains"}}


class LedgerValidationTests(unittest.TestCase):
    def test_unassessable_requires_coverage_block(self):
        item = {"id": "gap", "severity_before": "UNASSESSABLE", "evidence": [], "coverage_gap": None, "candidates": [], "triage": "queued"}
        self.assertTrue(MODULE.validate_ledger(ledger(item)))

    def test_severity_two_requires_candidate_and_exact_cost_math(self):
        item = {"id": "rock", "severity_before": 3, "evidence": ["rock.png"], "coverage_gap": None, "triage": "queued", "candidates": []}
        self.assertTrue(MODULE.validate_ledger(ledger(item)))
        item["candidates"] = [{"implementation_complexity": 1, "performance_cost": 2, "expected_severity_reduction": 2, "confidence": .5, "cost": 4, "benefit": 3, "benefit_cost_ratio": .75}]
        self.assertEqual(MODULE.validate_ledger(ledger(item)), [])

    def test_stop_rejects_open_or_coverage_blocked_items(self):
        item = {"id": "gap", "severity_before": "UNASSESSABLE", "evidence": [], "coverage_gap": "missing", "candidates": [], "triage": "coverage-blocked"}
        value = ledger(item)
        value["stop_decision"]["ready_to_stop"] = True
        self.assertTrue(MODULE.validate_ledger(value))


if __name__ == "__main__":
    unittest.main()
