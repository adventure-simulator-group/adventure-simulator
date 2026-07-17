#!/usr/bin/env python3
"""Normalise Viabundus v2 CSV data and optionally load it into SpacetimeDB."""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable


WORLD_YEAR = 1544
ROAD_TYPES = {"land", "ferry"}
REQUIRED_FILES = ("nodes.csv", "edges.csv", "population.csv")
SOURCE_DOI = "https://doi.org/10.5281/zenodo.16611998"


def optional_int(value: str | None) -> int | None:
    return int(value) if value not in (None, "", "null") else None


def optional_float(value: str | None) -> float | None:
    return float(value) if value not in (None, "", "null") else None


def active_in_year(row: dict[str, str], from_key: str, to_key: str, year: int) -> bool:
    start = optional_int(row.get(from_key))
    end = optional_int(row.get(to_key))
    return (start is None or start <= year) and (end is None or year < end)


def population_level(thousands: int | None) -> int:
    """Map Viabundus' approximate population (in thousands) to current UI bands."""
    if thousands is None or thousands <= 1:
        return 1
    if thousands <= 3:
        return 2
    if thousands <= 10:
        return 3
    if thousands <= 50:
        return 4
    return 5


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8-sig", newline="") as source:
        return list(csv.DictReader(source))


def normalise(raw_dir: Path, year: int) -> dict[str, Any]:
    missing = [name for name in REQUIRED_FILES if not (raw_dir / name).is_file()]
    if missing:
        raise RuntimeError(
            f"Viabundus data is missing from {raw_dir}: {', '.join(missing)}. "
            "Run `just init-viabundus` first."
        )

    nodes_by_id = {int(row["id"]): row for row in read_csv(raw_dir / "nodes.csv")}
    population_by_node: dict[int, tuple[int, int]] = {}
    for row in read_csv(raw_dir / "population.csv"):
        population_year = optional_int(row.get("year"))
        inhabitants = optional_int(row.get("inhabitants"))
        node_id = optional_int(row.get("nodesid"))
        if node_id is None or inhabitants is None or population_year is None or population_year > year:
            continue
        # Prefer the most recent estimate available at the game's start year.
        previous = population_by_node.get(node_id)
        if previous is None or population_year >= previous[0]:
            population_by_node[node_id] = (population_year, inhabitants)

    edges: list[dict[str, Any]] = []
    endpoint_ids: set[int] = set()
    excluded_by_reason: dict[str, int] = defaultdict(int)
    for row in read_csv(raw_dir / "edges.csv"):
        kind = row["type"]
        if kind not in ROAD_TYPES:
            excluded_by_reason[f"type:{kind}"] += 1
            continue
        if not active_in_year(row, "fromyear", "toyear", year):
            excluded_by_reason["inactive"] += 1
            continue
        from_node = int(row["fromnode"])
        to_node = int(row["tonode"])
        if from_node not in nodes_by_id or to_node not in nodes_by_id:
            excluded_by_reason["missing-node"] += 1
            continue
        endpoint_ids.update((from_node, to_node))
        edges.append(
            {
                "id": int(row["id"]),
                "from_node_id": from_node,
                "to_node_id": to_node,
                "kind": kind,
                "length_m": int(row["length"]),
                "slope_multiplier": float(row["slopemultiplier"] or 1),
                "certainty": int(row["certainty"]),
                "section": row["section"] or "",
            }
        )

    settlements: list[dict[str, Any]] = []
    settlement_node_ids: set[int] = set()
    for node_id, row in nodes_by_id.items():
        if row.get("Is_Settlement") != "y" or not active_in_year(
            row, "Settlement_From", "Settlement_To", year
        ):
            continue
        settlement_node_ids.add(node_id)
        estimate = population_by_node.get(node_id)
        settlements.append(
            {
                "id": f"viabundus-{node_id}",
                "source_node_id": node_id,
                "name": row["name"],
                "longitude": float(row["longitude"]),
                "latitude": float(row["latitude"]),
                "population_level": population_level(estimate[1] if estimate else None),
                "population_estimate": estimate[1] * 1_000 if estimate else 0,
                # Terrain-scene selection remains a tactical content decision.
                "scene_key": "hills",
                # Stable placeholder distribution until individual 1544 church
                # affiliations are curated from historical sources.
                "religion_id": (
                    "western_church",
                    "reformed",
                    "old_faith",
                )[node_id % 3],
            }
        )

    required_nodes = endpoint_ids | settlement_node_ids
    nodes: list[dict[str, Any]] = []
    for node_id in sorted(required_nodes):
        row = nodes_by_id[node_id]
        nodes.append(
            {
                "id": node_id,
                "parent_node_id": optional_int(row.get("parentid")),
                "latitude": float(row["latitude"]),
                "longitude": float(row["longitude"]),
                "is_settlement": row.get("Is_Settlement") == "y",
                "is_town": row.get("Is_Town") == "y",
                "is_ferry": row.get("Is_Ferry") == "y",
                "is_harbour": row.get("Is_Harbour") == "y",
            }
        )

    settlement_node_set = {settlement["source_node_id"] for settlement in settlements}
    connected_settlements = {
        endpoint
        for edge in edges
        for endpoint in (edge["from_node_id"], edge["to_node_id"])
        if endpoint in settlement_node_set
    }
    return {
        "metadata": {
            "source": "Viabundus Pre-modern Street Map 2",
            "source_doi": SOURCE_DOI,
            "license": "CC-BY-SA-4.0",
            "world_year": year,
            "road_types": sorted(ROAD_TYPES),
        },
        "nodes": nodes,
        "edges": edges,
        "settlements": sorted(settlements, key=lambda settlement: settlement["id"]),
        "report": {
            "nodes": len(nodes),
            "edges": len(edges),
            "settlements": len(settlements),
            "settlements_connected_to_road_network": len(connected_settlements),
            "excluded_edges": dict(sorted(excluded_by_reason.items())),
        },
    }


def chunks(rows: list[dict[str, Any]], size: int) -> Iterable[list[dict[str, Any]]]:
    for index in range(0, len(rows), size):
        yield rows[index : index + size]


def call_reducer(
    spacetime: str, server: str, database: str, reducer: str, *arguments: Any
) -> None:
    subprocess.run(
        [
            spacetime,
            "call",
            "--server",
            server,
            database,
            reducer,
            *(json.dumps(argument, separators=(",", ":")) for argument in arguments),
        ],
        check=True,
    )


def sats_option(value: Any) -> dict[str, Any]:
    """Encode an Option value for SpacetimeDB's SATS JSON input format."""
    return {"none": []} if value is None else {"some": value}


def load_world(data: dict[str, Any], args: argparse.Namespace) -> None:
    call_reducer(args.spacetime, args.server, args.database, "begin_world_data_import")
    nodes = [
        {
            **node,
            "parent_node_id": sats_option(node["parent_node_id"]),
        }
        for node in data["nodes"]
    ]
    for label, reducer, rows in (
        ("nodes", "import_world_nodes", nodes),
        ("edges", "import_travel_edges", data["edges"]),
        ("settlements", "import_settlements", data["settlements"]),
    ):
        total = len(rows)
        for batch_number, batch in enumerate(chunks(rows, args.batch_size), start=1):
            print(f"Loading {label}: batch {batch_number} ({len(batch)} rows)")
            call_reducer(args.spacetime, args.server, args.database, reducer, batch)
        print(f"Loaded {total} {label}.")


def main() -> int:
    repository = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw-dir", type=Path, default=repository / "viabundus")
    parser.add_argument("--year", type=int, default=WORLD_YEAR)
    parser.add_argument(
        "--output", type=Path, default=repository / "target" / "viabundus-v2-1544.json"
    )
    parser.add_argument("--load", action="store_true", help="load normalised batches into SpacetimeDB")
    parser.add_argument("--spacetime", default="spacetime")
    parser.add_argument("--server", default="http://localhost:3000")
    parser.add_argument("--database", default="adventuresim-stdb-module")
    parser.add_argument("--batch-size", type=int, default=100)
    args = parser.parse_args()
    if args.batch_size < 1:
        parser.error("--batch-size must be positive")

    data = normalise(args.raw_dir, args.year)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf-8")
    print(json.dumps(data["report"], indent=2))
    print(f"Wrote normalised data to {args.output}")
    if args.load:
        load_world(data, args)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
