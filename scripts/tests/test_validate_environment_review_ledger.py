import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "validate_environment_review_ledger.py"
TEMPLATE = SCRIPT.parents[1] / "assets" / "tactical-scenes" / "environment-review-ledger.template.json"
SCHEMA = SCRIPT.parents[1] / "assets" / "tactical-scenes" / "environment-review-ledger.schema.json"
SPEC = importlib.util.spec_from_file_location("validate_environment_review_ledger", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def ledger(item):
    return {"items": [item], "stop_decision": {"ready_to_stop": False, "reason": "work remains"}}


class LedgerValidationTests(unittest.TestCase):
    def test_template_passes_schema_and_semantic_validation(self):
        value = json.loads(TEMPLATE.read_text(encoding="utf-8"))
        schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
        self.assertEqual(MODULE.audit_schema(schema), [])
        self.assertEqual(MODULE.validate_schema(value, schema), [])
        self.assertEqual(MODULE.validate_ledger(value), [])

    def test_schema_rejects_missing_extra_and_wrong_items_type(self):
        value = json.loads(TEMPLATE.read_text(encoding="utf-8"))
        schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
        for mutate in (
            lambda ledger: ledger.pop("scope"),
            lambda ledger: ledger.__setitem__("unexpected", True),
            lambda ledger: ledger.__setitem__("items", {}),
        ):
            broken = copy.deepcopy(value)
            mutate(broken)
            self.assertTrue(MODULE.validate_schema(broken, schema))

    def test_schema_audit_rejects_unimplemented_keywords(self):
        self.assertTrue(MODULE.audit_schema({"type": "object", "format": "date"}))
        self.assertTrue(MODULE.audit_schema({"contains": {"const": "x"}}))
        self.assertTrue(MODULE.audit_schema({"allOf": {"contains": {"const": "x"}}}))
        self.assertTrue(MODULE.audit_schema({"allOf": [{"type": "string"}]}))
        self.assertTrue(
            MODULE.audit_schema(
                {"type": "string", "allOf": [{"contains": {"const": "x"}}]}
            )
        )
        self.assertTrue(
            MODULE.audit_schema(
                {"type": "object", "properties": {"allOf[0]": {"contains": {"const": "x"}}}}
            )
        )
        self.assertTrue(MODULE.audit_schema({"oneOf": [], "type": "string"}))

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

    def test_duplicate_item_ids_are_rejected(self):
        item = {"id": "same", "severity_before": 0, "evidence": ["view.png"], "coverage_gap": None, "candidates": [], "triage": "closed-resolved"}
        value = {"items": [item, copy.deepcopy(item)], "stop_decision": {"ready_to_stop": False, "reason": "work remains"}}
        self.assertIn("items: ids must be unique", MODULE.validate_ledger(value))


if __name__ == "__main__":
    unittest.main()
