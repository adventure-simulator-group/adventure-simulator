#!/usr/bin/env python3
"""Validate semantic invariants beyond the environment-review JSON schema."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


def validate_ledger(ledger: dict) -> list[str]:
    errors: list[str] = []
    items = ledger.get("items", [])
    for index, item in enumerate(items):
        prefix = f"items[{index}]"
        severity = item.get("severity_before")
        evidence = item.get("evidence", [])
        coverage_gap = item.get("coverage_gap")
        triage = item.get("triage")
        candidates = item.get("candidates", [])
        if severity == "UNASSESSABLE":
            if not coverage_gap or triage != "coverage-blocked":
                errors.append(f"{prefix}: UNASSESSABLE requires a coverage gap and coverage-blocked triage")
        elif not evidence:
            errors.append(f"{prefix}: assessable critique requires evidence")
        if isinstance(severity, int) and severity >= 2 and triage not in {"coverage-blocked", "closed-resolved"}:
            if not candidates:
                errors.append(f"{prefix}: severity 2+ requires candidate alternatives")
        for candidate_index, candidate in enumerate(candidates):
            candidate_prefix = f"{prefix}.candidates[{candidate_index}]"
            expected_cost = 1 + candidate.get("implementation_complexity", 0) + candidate.get("performance_cost", 0)
            expected_benefit = (severity if isinstance(severity, int) else 0) * candidate.get("expected_severity_reduction", 0) * candidate.get("confidence", 0)
            expected_ratio = expected_benefit / expected_cost
            for field, expected in (("cost", expected_cost), ("benefit", expected_benefit), ("benefit_cost_ratio", expected_ratio)):
                if not math.isclose(candidate.get(field, -1), expected, rel_tol=1e-6, abs_tol=1e-6):
                    errors.append(f"{candidate_prefix}: {field} must equal {expected:.6g}")
    stop = ledger.get("stop_decision", {})
    if stop.get("ready_to_stop"):
        open_items = [item.get("id", "?") for item in items if item.get("triage") in {"queued", "implemented", "reassess", "coverage-blocked"}]
        if open_items:
            errors.append(f"stop_decision: cannot stop with open items: {', '.join(open_items)}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ledger", type=Path)
    args = parser.parse_args()
    errors = validate_ledger(json.loads(args.ledger.read_text(encoding="utf-8")))
    if errors:
        raise SystemExit("Invalid environment review ledger:\n" + "\n".join(errors))
    print(f"ENVIRONMENT_REVIEW_LEDGER_VALID={args.ledger}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
