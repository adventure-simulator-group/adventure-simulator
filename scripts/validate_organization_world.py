#!/usr/bin/env python3
"""Cross-check organization chapters, recognition, and policies against world data."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--world",
        type=Path,
        required=True,
        help="Compiled Viabundus world JSON containing a settlements array.",
    )
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]

    world = json.loads(args.world.read_text(encoding="utf-8"))
    records = world.get("settlements")
    if not isinstance(records, list):
        raise SystemExit(f"{args.world}: expected a settlements array")
    settlement_ids = {
        row["id"]
        for row in records
        if isinstance(row, dict) and isinstance(row.get("id"), str)
    }
    source = str(args.world)

    errors: list[str] = []
    catalog_dir = root / "content" / "organizations"
    for path in sorted(catalog_dir.glob("*.yaml")):
        parsed = json.loads(path.read_text(encoding="utf-8"))
        documents = parsed if isinstance(parsed, list) else [parsed]
        for definition in documents:
            organization_id = definition.get("id", "<missing>")
            references = list(definition.get("chapters", []))
            recognition = definition.get("recognition", {})
            if recognition.get("kind") == "settlements":
                references.extend(recognition.get("settlement_ids", []))
            for settlement_id in references:
                if settlement_id not in settlement_ids:
                    errors.append(
                        f"{path.relative_to(root)}: {organization_id} references "
                        f"unknown settlement {settlement_id!r}"
                    )

    policy_path = root / "content" / "settlement-policies.yaml"
    for policy in json.loads(policy_path.read_text(encoding="utf-8")):
        if policy.get("settlement_id") not in settlement_ids:
            errors.append(
                f"{policy_path.relative_to(root)}: unknown settlement "
                f"{policy.get('settlement_id')!r}"
            )

    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Organization world references valid against {source} ({len(settlement_ids)} settlements).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
