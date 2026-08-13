#!/usr/bin/env python3
"""Validate semantic invariants beyond the environment-review JSON schema."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


SCHEMA_PATH = Path(__file__).resolve().parents[1] / "assets/tactical-scenes/environment-review-ledger.schema.json"
SUPPORTED_SCHEMA_KEYS = {"$schema", "$id", "$defs", "$ref", "title", "type", "required", "properties", "additionalProperties", "const", "enum", "oneOf", "minimum", "maximum", "minLength", "minItems", "uniqueItems", "items", "allOf", "contains", "pattern"}


def audit_schema(
    schema: object, path: str = "$schema", *, inside_supported_all_of_rule: bool = False
) -> list[str]:
    if not isinstance(schema, dict):
        return [f"{path}: schema must be an object"]
    errors = [f"{path}: unsupported JSON Schema keyword {key}" for key in schema if key not in SUPPORTED_SCHEMA_KEYS]
    if "$ref" in schema and set(schema) != {"$ref"}:
        errors.append(f"{path}: reference siblings are not evaluated")
    if "oneOf" in schema and set(schema) != {"oneOf"}:
        errors.append(f"{path}: oneOf siblings are not evaluated")
    for key in ("properties", "$defs"):
        if key in schema and not isinstance(schema[key], dict):
            errors.append(f"{path}.{key}: must be an object")
    for key in ("oneOf", "allOf", "required"):
        if key in schema and not isinstance(schema[key], list):
            errors.append(f"{path}.{key}: must be an array")
    if "contains" in schema and not inside_supported_all_of_rule:
        errors.append(f"{path}.contains: direct contains is not evaluated")
    if "allOf" in schema and isinstance(schema["allOf"], list):
        if schema.get("type") != "array":
            errors.append(f"{path}.allOf: only array allOf rules are evaluated")
        for index, rule in enumerate(schema["allOf"]):
            if not isinstance(rule, dict) or set(rule) != {"contains"} or not isinstance(rule["contains"], dict) or set(rule["contains"]) != {"const"}:
                errors.append(f"{path}.allOf[{index}]: only contains/const rules are evaluated")
    for key, value in schema.items():
        if key in {"properties", "$defs"} and isinstance(value, dict):
            for name, child in value.items():
                errors.extend(audit_schema(child, f"{path}.{key}.{name}"))
        elif key in {"items", "contains"}:
            errors.extend(audit_schema(value, f"{path}.{key}"))
        elif key in {"oneOf", "allOf"} and isinstance(value, list):
            for index, child in enumerate(value):
                errors.extend(
                    audit_schema(
                        child,
                        f"{path}.{key}[{index}]",
                        inside_supported_all_of_rule=key == "allOf",
                    )
                )
    return errors


def validate_schema(instance: object, schema: dict, root: dict | None = None, path: str = "$") -> list[str]:
    root = schema if root is None else root
    if "$ref" in schema:
        target = schema["$ref"]
        if not target.startswith("#/$defs/"):
            return [f"{path}: unsupported reference {target}"]
        return validate_schema(instance, root["$defs"][target.removeprefix("#/$defs/")], root, path)
    if "oneOf" in schema:
        matches = sum(not validate_schema(instance, option, root, path) for option in schema["oneOf"])
        return [] if matches == 1 else [f"{path}: must match exactly one allowed shape"]
    errors = []
    if "const" in schema and instance != schema["const"]:
        errors.append(f"{path}: value differs from required constant")
    if "enum" in schema and instance not in schema["enum"]:
        errors.append(f"{path}: value is not allowed")
    expected = schema.get("type")
    types = expected if isinstance(expected, list) else [expected]
    if expected is not None and not any(_matches_type(instance, value) for value in types):
        return [f"{path}: has the wrong type"]
    if isinstance(instance, dict):
        properties = schema.get("properties", {})
        errors.extend(f"{path}: missing required property {name}" for name in schema.get("required", []) if name not in instance)
        if schema.get("additionalProperties") is False:
            errors.extend(f"{path}: unexpected property {name}" for name in instance.keys() - properties.keys())
        for name in instance.keys() & properties.keys():
            errors.extend(validate_schema(instance[name], properties[name], root, f"{path}.{name}"))
    elif isinstance(instance, list):
        if len(instance) < schema.get("minItems", 0): errors.append(f"{path}: has too few items")
        if schema.get("uniqueItems") and len({json.dumps(value, sort_keys=True) for value in instance}) != len(instance): errors.append(f"{path}: items must be unique")
        for index, value in enumerate(instance): errors.extend(validate_schema(value, schema.get("items", {}), root, f"{path}[{index}]"))
        for rule in schema.get("allOf", []):
            required = rule.get("contains", {}).get("const")
            if required is not None and required not in instance: errors.append(f"{path}: must contain {required!r}")
    elif isinstance(instance, str):
        if len(instance) < schema.get("minLength", 0): errors.append(f"{path}: string is too short")
        if "pattern" in schema and __import__("re").fullmatch(schema["pattern"], instance) is None: errors.append(f"{path}: string pattern differs")
    elif isinstance(instance, (int, float)) and not isinstance(instance, bool):
        if instance < schema.get("minimum", instance): errors.append(f"{path}: below minimum")
        if instance > schema.get("maximum", instance): errors.append(f"{path}: above maximum")
    return errors


def _matches_type(value: object, expected: str) -> bool:
    return {"object": isinstance(value, dict), "array": isinstance(value, list), "string": isinstance(value, str), "integer": isinstance(value, int) and not isinstance(value, bool), "number": isinstance(value, (int, float)) and not isinstance(value, bool), "boolean": isinstance(value, bool), "null": value is None}[expected]


def validate_ledger(ledger: dict) -> list[str]:
    errors: list[str] = []
    items = ledger.get("items", [])
    item_ids = [item.get("id") for item in items]
    if len(set(item_ids)) != len(item_ids):
        errors.append("items: ids must be unique")
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
    schema = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
    ledger = json.loads(args.ledger.read_text(encoding="utf-8"))
    errors = audit_schema(schema)
    if not errors:
        errors = validate_schema(ledger, schema)
    if not errors:
        errors = validate_ledger(ledger)
    if errors:
        raise SystemExit("Invalid environment review ledger:\n" + "\n".join(errors))
    print(f"ENVIRONMENT_REVIEW_LEDGER_VALID={args.ledger}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
