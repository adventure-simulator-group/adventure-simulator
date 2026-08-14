#!/usr/bin/env python3
"""Benchmark equal-area tactical terrain families and normalize their density."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys


PROFILES = {
    "bare": {
        "surface": "road",
        "canopy_bps": 0,
        "wetland_bps": 0,
        "cultivation_bps": 0,
        "water_bps": 0,
        "hilly_bps": 0,
        "crossing_bps": 10_000,
    },
    "grassland": {
        "surface": "open",
        "canopy_bps": 0,
        "wetland_bps": 0,
        "cultivation_bps": 0,
        "water_bps": 0,
        "hilly_bps": 0,
        "crossing_bps": 0,
    },
    "sparse_woodland": {
        "surface": "sparse_woods",
        "canopy_bps": 3_500,
        "wetland_bps": 300,
        "cultivation_bps": 0,
        "water_bps": 0,
        "hilly_bps": 0,
        "crossing_bps": 0,
    },
    "dense_woodland": {
        "surface": "deep_woods",
        "canopy_bps": 9_000,
        "wetland_bps": 500,
        "cultivation_bps": 0,
        "water_bps": 0,
        "hilly_bps": 0,
        "crossing_bps": 0,
    },
    "wetland": {
        "surface": "wetland",
        "canopy_bps": 1_000,
        "wetland_bps": 9_500,
        "cultivation_bps": 0,
        "water_bps": 3_000,
        "hilly_bps": 0,
        "crossing_bps": 0,
    },
    "rocky_ground": {
        "surface": "open",
        "canopy_bps": 0,
        "wetland_bps": 0,
        "cultivation_bps": 0,
        "water_bps": 0,
        "hilly_bps": 9_000,
        "crossing_bps": 0,
    },
}

GRASS_BLADES_PER_MACRO_PATCH = 96 * 96


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run equal-area QHD terrain-family performance comparisons."
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("target/tactical-benchmarks/terrain-density"),
    )
    parser.add_argument("--frames", type=int, default=600)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument(
        "--summarize-existing",
        action="store_true",
        help="regenerate aggregate reports from completed profile directories",
    )
    return parser.parse_args()


def uniform_environment(profile: dict[str, int | str], count: int) -> list[dict]:
    return [dict(profile) for _ in range(count)]


def write_inputs(repository: Path, output: Path) -> list[tuple[str, Path]]:
    template_path = repository / "assets/tactical-scenes/flat-dry-grassland.json"
    template = json.loads(template_path.read_text(encoding="utf-8"))
    inputs = output / "inputs"
    inputs.mkdir()
    written = []
    bare_vista = PROFILES["bare"]
    for index, (name, profile) in enumerate(PROFILES.items()):
        scene = json.loads(json.dumps(template))
        scene["seed"] = 71_000 + index
        scene["scene_key"] = f"terrain-density-{name}"
        scene["source"] = {"kind": "synthetic_fixture", "id": f"terrain-density-{name}"}
        scene["playable"]["environment"] = uniform_environment(
            profile, len(scene["playable"]["environment"])
        )
        for lod in scene["vista"]["lods"]:
            lod["environment"] = uniform_environment(
                bare_vista, len(lod["environment"])
            )
        path = inputs / f"{name}.json"
        path.write_text(json.dumps(scene, indent=2) + "\n", encoding="utf-8")
        written.append((name, path))
    return written


def gpu_mean(result: dict) -> float:
    return sum(
        metric["mean"]
        for path, metric in result["render_diagnostics"].items()
        if path.endswith("/elapsed_gpu")
    )


def grass_density(profile: dict[str, int | str]) -> float:
    canopy = int(profile["canopy_bps"]) / 10_000
    water = int(profile["water_bps"]) / 10_000
    cultivation = int(profile["cultivation_bps"]) / 10_000
    return max(0.25, min(0.98, 0.98 - canopy * 0.95 - water * 0.88 + cultivation * 0.04))


def report_row(name: str, report: dict) -> dict:
    result = report["results"][0]
    area = report["playable_area_square_km"]
    counts = dict(report["scene_entity_counts"])
    counts["playable_trees"] = report["playable_tree_count"]
    physical_counts = {
        "playable_trees": counts.get("playable_trees", 0),
        "grass_macro_patches": counts.get("grass_patches", 0) // 2,
        "grass_blades": round(
            (counts.get("grass_patches", 0) // 2)
            * GRASS_BLADES_PER_MACRO_PATCH
            * grass_density(PROFILES[name])
        ),
        "understory_plants": counts.get("understory_patches", 0) // 3,
        "dry_leaf_patches": counts.get("dry_leaf_patches", 0),
        "twig_patches": counts.get("twig_patches", 0),
        "loose_stones": counts.get(
            "loose_stone_pebbles", counts.get("loose_stone_patches", 0)
        ),
        "procedural_rocks": counts.get("procedural_rocks", 0),
    }
    return {
        "profile": name,
        "playable_area_square_km": area,
        "entity_counts": counts,
        "entity_density_per_square_km": {
            key: value / area for key, value in counts.items()
        },
        "physical_instance_counts": physical_counts,
        "physical_density_per_square_km": {
            key: value / area for key, value in physical_counts.items()
        },
        "wall_median_ms": result["median_ms"],
        "wall_p95_ms": result["p95_ms"],
        "gpu_mean_ms": gpu_mean(result),
        "gpu_p95_ms": result["gpu_elapsed_p95_ms"],
    }


def run_profiles(repository: Path, output: Path, frames: int, skip_build: bool) -> list[dict]:
    if frames < 30:
        raise SystemExit("--frames must be at least 30")
    executable = repository / "target/release/tactical-scene-viewer"
    if os.name == "nt":
        executable = executable.with_suffix(".exe")
    if not skip_build:
        subprocess.run(
            [
                "cargo",
                "build",
                "--release",
                "-p",
                "adventuresim-tactical-client",
                "--bin",
                "tactical-scene-viewer",
            ],
            cwd=repository,
            check=True,
        )
    if not executable.is_file():
        raise SystemExit(f"missing benchmark executable: {executable}")
    rows = []
    for name, input_path in write_inputs(repository, output):
        profile_output = output / name
        environment = os.environ.copy()
        environment["TACTICAL_BENCH_ONLY_MODE"] = "Natural production LODs"
        subprocess.run(
            [
                str(executable),
                "--scene-input",
                str(input_path),
                "--scene-performance-benchmark-frames",
                str(frames),
                "--scene-performance-render-diagnostics",
                "--output",
                str(profile_output),
            ],
            cwd=repository,
            env=environment,
            check=True,
        )
        report = json.loads(
            (profile_output / "scene-performance-benchmark.json").read_text(
                encoding="utf-8"
            )
        )
        rows.append(report_row(name, report))
    return rows


def load_existing_rows(output: Path) -> tuple[list[dict], int]:
    rows = []
    frames = None
    for name in PROFILES:
        report_path = output / name / "scene-performance-benchmark.json"
        report = json.loads(report_path.read_text(encoding="utf-8"))
        frames = report["sample_frames_per_mode"] if frames is None else frames
        if report["sample_frames_per_mode"] != frames:
            raise SystemExit("existing profile reports use different sample counts")
        rows.append(report_row(name, report))
    return rows, frames


def summarize(output: Path, rows: list[dict], frames: int) -> None:
    baseline = rows[0]["gpu_mean_ms"]
    positive_marginals = [
        row["gpu_mean_ms"] - baseline
        for row in rows[1:]
        if row["gpu_mean_ms"] - baseline > 0.02
    ]
    target = statistics.median(positive_marginals) if positive_marginals else 0.0
    for row in rows:
        marginal = row["gpu_mean_ms"] - baseline
        row["marginal_gpu_ms_vs_bare"] = marginal
        row["cost_balance_ratio"] = marginal / target if target > 0.0 else None
        row["linear_density_scale_to_balance"] = (
            target / marginal if target > 0.0 and marginal > 0.02 else None
        )
        ratio = row["cost_balance_ratio"]
        row["balance"] = (
            "baseline"
            if row["profile"] == "bare"
            else "below_noise"
            if marginal <= 0.02
            else "balanced"
            if 0.75 <= ratio <= 1.25
            else "too_expensive"
            if ratio > 1.25
            else "budget_available"
        )
    aggregate = {
        "pipeline": "tactical_terrain_density_benchmark_v1",
        "rendered_area_square_km": rows[0]["playable_area_square_km"],
        "density_normalization": "entity counts are normalized to one square kilometre; frame cost is measured for the fixed one-hectare viewport and is not multiplied linearly",
        "sample_frames_per_profile": frames,
        "balance_target_marginal_gpu_ms": target,
        "balance_band": [0.75, 1.25],
        "results": rows,
    }
    (output / "terrain-density-benchmark.json").write_text(
        json.dumps(aggregate, indent=2) + "\n", encoding="utf-8"
    )
    lines = [
        "# Equal-area tactical terrain benchmark",
        "",
        f"Each profile renders the same {rows[0]['playable_area_square_km']:.3f} km^2 "
        f"production plot at QHD. Physical instance counts are normalized to density/km^2; GPU cost "
        "is the measured viewport cost and is not multiplied by 100.",
        "",
        "| Terrain | GPU mean ms | GPU P95 ms | Marginal vs bare ms | Linear density scale | Balance | Trees/km^2 | Grass blades/m^2 | Understory plants/km^2 | Pebbles/km^2 |",
        "|---|---:|---:|---:|---:|:---:|---:|---:|---:|---:|",
    ]
    for row in rows:
        density = row["physical_density_per_square_km"]
        scale = row["linear_density_scale_to_balance"]
        lines.append(
            f"| {row['profile']} | {row['gpu_mean_ms']:.3f} | "
            f"{row['gpu_p95_ms']:.3f} | {row['marginal_gpu_ms_vs_bare']:+.3f} | "
            f"{'n/a' if scale is None else f'{scale:.2f}x'} | {row['balance']} | "
            f"{density.get('playable_trees', 0):.0f} | "
            f"{density.get('grass_blades', 0) / 1_000_000:.0f} | "
            f"{density.get('understory_plants', 0):.0f} | "
            f"{density.get('loose_stones', 0):.0f} |"
        )
    lines.extend(
        [
            "",
            "The balance label compares positive marginal GPU cost with the median terrain-family marginal cost. Values within 0.75x-1.25x are balanced; small or negative deltas are below the measurement noise floor. The density scale is a first-order diagnostic, not an automatic tuning instruction: culling, occlusion, and overdraw make actual scaling non-linear.",
            "",
        ]
    )
    (output / "comparison.md").write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    args = parse_args()
    repository = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if args.summarize_existing:
        rows, frames = load_existing_rows(output)
        summarize(output, rows, frames)
        print(f"TERRAIN_DENSITY_BENCHMARK={output}")
        return 0
    if output.exists():
        raise SystemExit(f"output must be a fresh path: {output}")
    output.mkdir(parents=True)
    rows = run_profiles(repository, output, args.frames, args.skip_build)
    summarize(output, rows, args.frames)
    print(f"TERRAIN_DENSITY_BENCHMARK={output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
