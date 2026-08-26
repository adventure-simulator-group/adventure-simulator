#!/usr/bin/env python3
"""Measure hand excursion from viewer or real-client animation JSONL."""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path


def subtract(left: list[float], right: list[float]) -> tuple[float, float, float]:
    return (left[0] - right[0], left[1] - right[1], left[2] - right[2])


def rotate_by_inverse(
    rotation_xyzw: list[float], vector: tuple[float, float, float]
) -> tuple[float, float, float]:
    x, y, z, w = rotation_xyzw
    length = math.sqrt(x * x + y * y + z * z + w * w)
    if not math.isfinite(length) or length <= 1.0e-9:
        raise ValueError("subject rotation is not a finite non-zero quaternion")
    # Inverse of a unit quaternion, followed by q * v * conjugate(q).
    qx, qy, qz, qw = -x / length, -y / length, -z / length, w / length
    vx, vy, vz = vector
    tx = 2.0 * (qy * vz - qz * vy)
    ty = 2.0 * (qz * vx - qx * vz)
    tz = 2.0 * (qx * vy - qy * vx)
    return (
        vx + qw * tx + (qy * tz - qz * ty),
        vy + qw * ty + (qz * tx - qx * tz),
        vz + qw * tz + (qx * ty - qy * tx),
    )


def distance(left: tuple[float, float, float], right: tuple[float, float, float]) -> float:
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(left, right, strict=True)))


def is_hand_bone(name: str) -> bool:
    return name.casefold() in {"l_wrist", "r_wrist"}


def analyze_trace(path: Path, scenario: str | None = None) -> dict[str, object]:
    samples: dict[str, list[tuple[int, tuple[float, float, float]]]] = {}
    active_frames = 0
    bone_counts: list[int] = []
    selected_scenario: str | None = None

    with path.open("r", encoding="utf-8") as trace:
        for line_number, line in enumerate(trace, 1):
            if not line.strip():
                continue
            try:
                frame = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"invalid JSON on line {line_number}: {error}") from error
            if scenario is not None and frame.get("scenario") != scenario:
                continue
            if frame.get("action") != "Attack":
                continue
            selected_scenario = str(frame.get("scenario"))
            active_frames += 1
            bones = frame.get("bones", [])
            bone_counts.append(len(bones))
            subject_translation = frame["subject_translation"]
            subject_rotation = frame["subject_rotation_xyzw"]
            scenario_frame = int(frame["scenario_frame"])
            for bone in bones:
                name = str(bone.get("name", ""))
                if not is_hand_bone(name):
                    continue
                local = rotate_by_inverse(
                    subject_rotation,
                    subtract(bone["translation"], subject_translation),
                )
                samples.setdefault(name, []).append((scenario_frame, local))

    if active_frames == 0:
        raise ValueError("trace contains no matching active attack frames")
    if not samples:
        raise ValueError("trace contains no l_wrist or r_wrist animation targets")

    hands: dict[str, object] = {}
    for name, positions in sorted(samples.items()):
        start_frame, start = positions[0]
        excursions = [(frame, distance(position, start)) for frame, position in positions]
        forward_excursions = [
            (frame, abs(position[2] - start[2])) for frame, position in positions
        ]
        peak_frame, maximum = max(excursions, key=lambda sample: sample[1])
        forward_peak_frame, maximum_forward = max(
            forward_excursions, key=lambda sample: sample[1]
        )
        hands[name] = {
            "start_frame": start_frame,
            "last_frame": positions[-1][0],
            "peak_frame": peak_frame,
            "maximum_excursion_metres": maximum,
            "maximum_forward_excursion_metres": maximum_forward,
            "forward_peak_frame": forward_peak_frame,
            "final_excursion_metres": excursions[-1][1],
            "sample_count": len(positions),
        }

    return {
        "scenario": selected_scenario,
        "active_attack_frames": active_frames,
        "minimum_bones_per_frame": min(bone_counts),
        "maximum_bones_per_frame": max(bone_counts),
        "hands": hands,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--scenario")
    parser.add_argument("--minimum-excursion-metres", type=float, default=0.05)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not math.isfinite(args.minimum_excursion_metres) or args.minimum_excursion_metres <= 0.0:
        raise SystemExit("--minimum-excursion-metres must be finite and positive")
    try:
        result = analyze_trace(args.trace, args.scenario)
    except (OSError, KeyError, TypeError, ValueError) as error:
        print(f"bone trace analysis failed: {error}", file=sys.stderr)
        return 2

    hands = result["hands"]
    maximum = max(
        hand["maximum_forward_excursion_metres"] for hand in hands.values()
    )
    result["required_minimum_excursion_metres"] = args.minimum_excursion_metres
    result["attack_excursion_valid"] = maximum >= args.minimum_excursion_metres
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["attack_excursion_valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
