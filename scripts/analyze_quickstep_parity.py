#!/usr/bin/env python3
"""Compare an authored quickstep against a real neutral-root gameplay trace."""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np

from prepare_animation_motion import ROOT_PATH, accessor_view, read_glb, scene_paths


KEY_PHASES = (0.0, 0.25, 0.5, 0.75, 1.0)
TRACKED_BONES = (
    "root",
    "l_upleg",
    "r_upleg",
    "l_lowleg",
    "r_lowleg",
    "l_foot",
    "r_foot",
)


def quaternion_matrix(value: np.ndarray) -> np.ndarray:
    x, y, z, w = (float(component) for component in value)
    length = math.sqrt(x * x + y * y + z * z + w * w)
    if not math.isfinite(length) or length <= 1.0e-9:
        raise ValueError("animation contains a non-finite or zero quaternion")
    x, y, z, w = x / length, y / length, z / length, w / length
    return np.asarray(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ],
        dtype=np.float64,
    )


def slerp(left: np.ndarray, right: np.ndarray, progress: float) -> np.ndarray:
    left = np.asarray(left, dtype=np.float64)
    right = np.asarray(right, dtype=np.float64)
    left /= np.linalg.norm(left)
    right /= np.linalg.norm(right)
    dot = float(np.dot(left, right))
    if dot < 0.0:
        right = -right
        dot = -dot
    if dot > 0.9995:
        result = left + (right - left) * progress
        return result / np.linalg.norm(result)
    angle = math.acos(max(-1.0, min(1.0, dot)))
    scale = math.sin(angle)
    return (
        math.sin((1.0 - progress) * angle) / scale * left
        + math.sin(progress * angle) / scale * right
    )


class MotionSampler:
    def __init__(self, path: Path):
        self.path = path
        self.document, self.binary = read_glb(path)
        self.paths = scene_paths(self.document)
        self.nodes_by_name = {
            path[-1]: index
            for index, path in self.paths.items()
            if path and path[:1] == ("Skeleton",)
        }
        missing = set(TRACKED_BONES) - self.nodes_by_name.keys()
        if missing:
            raise ValueError(f"{path} is missing tracked bone {sorted(missing)[0]}")
        self.parents: dict[int, int] = {}
        for parent, node in enumerate(self.document["nodes"]):
            for child in node.get("children", ()):
                self.parents[child] = parent
        animation = self.document["animations"][0]
        self.channels: dict[tuple[int, str], tuple[np.ndarray, np.ndarray, str]] = {}
        for channel in animation["channels"]:
            sampler = animation["samplers"][channel["sampler"]]
            interpolation = sampler.get("interpolation", "LINEAR")
            if interpolation not in {"LINEAR", "STEP"}:
                raise ValueError(f"{path} uses unsupported {interpolation} interpolation")
            self.channels[(channel["target"]["node"], channel["target"]["path"])] = (
                accessor_view(self.document, self.binary, sampler["input"])[:, 0].copy(),
                accessor_view(self.document, self.binary, sampler["output"]).copy(),
                interpolation,
            )
        root_node = next(
            index for index, node_path in self.paths.items() if node_path == ROOT_PATH
        )
        root_channel = self.channels.get((root_node, "translation"))
        if root_channel is None:
            raise ValueError(f"{path} has no animated root translation")
        self.duration = float(root_channel[0][-1])

    def channel_value(self, node: int, path: str, time: float) -> np.ndarray:
        defaults = {
            "translation": (0.0, 0.0, 0.0),
            "rotation": (0.0, 0.0, 0.0, 1.0),
            "scale": (1.0, 1.0, 1.0),
        }
        channel = self.channels.get((node, path))
        if channel is None:
            return np.asarray(self.document["nodes"][node].get(path, defaults[path]), dtype=np.float64)
        times, values, interpolation = channel
        if time <= times[0]:
            return values[0].astype(np.float64)
        if time >= times[-1]:
            return values[-1].astype(np.float64)
        right = int(np.searchsorted(times, time, side="right"))
        left = right - 1
        if interpolation == "STEP":
            return values[left].astype(np.float64)
        progress = float((time - times[left]) / (times[right] - times[left]))
        if path == "rotation":
            return slerp(values[left], values[right], progress)
        return values[left] + (values[right] - values[left]) * progress

    def local_matrix(self, node: int, time: float) -> np.ndarray:
        raw = self.document["nodes"][node]
        if "matrix" in raw and not any((node, path) in self.channels for path in ("translation", "rotation", "scale")):
            return np.asarray(raw["matrix"], dtype=np.float64).reshape((4, 4), order="F")
        translation = self.channel_value(node, "translation", time)
        rotation = self.channel_value(node, "rotation", time)
        scale = self.channel_value(node, "scale", time)
        matrix = np.identity(4, dtype=np.float64)
        matrix[:3, :3] = quaternion_matrix(rotation) @ np.diag(scale)
        matrix[:3, 3] = translation
        return matrix

    def positions(self, phase: float) -> dict[str, np.ndarray]:
        time = min(1.0, max(0.0, phase)) * self.duration
        cache: dict[int, np.ndarray] = {}

        def global_matrix(node: int) -> np.ndarray:
            if node not in cache:
                local = self.local_matrix(node, time)
                parent = self.parents.get(node)
                cache[node] = local if parent is None else global_matrix(parent) @ local
            return cache[node]

        return {
            name: global_matrix(self.nodes_by_name[name])[:3, 3].copy()
            for name in TRACKED_BONES
        }


def load_dodge_trace(path: Path) -> list[dict[str, object]]:
    frames = []
    with path.open("r", encoding="utf-8") as source:
        for line_number, line in enumerate(source, 1):
            if not line.strip():
                continue
            try:
                frame = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"invalid JSON on line {line_number}: {error}") from error
            if frame.get("action") == "Dodge":
                frames.append(frame)
    if len(frames) < 2:
        raise ValueError("trace must contain at least two active Dodge frames")
    return frames


def controller_curve(frames: list[dict[str, object]]) -> tuple[list[tuple[float, float]], float]:
    start = np.asarray(frames[0]["controller_transform"]["translation"], dtype=np.float64)
    end = np.asarray(frames[-1]["controller_transform"]["translation"], dtype=np.float64)
    direction = end[[0, 2]] - start[[0, 2]]
    distance = float(np.linalg.norm(direction))
    if distance <= 1.0e-6:
        raise ValueError("quickstep controller did not travel")
    direction /= distance
    by_phase: dict[float, float] = {}
    for frame in frames:
        phase = float(frame["action_phase"])
        position = np.asarray(frame["controller_transform"]["translation"], dtype=np.float64)
        displacement = float(np.dot(position[[0, 2]] - start[[0, 2]], direction))
        by_phase[phase] = max(by_phase.get(phase, -math.inf), displacement / distance)
    curve = sorted(by_phase.items())
    return curve, distance


def interpolate_curve(curve: list[tuple[float, float]], phase: float) -> float:
    if phase <= curve[0][0]:
        return curve[0][1]
    if phase >= curve[-1][0]:
        return curve[-1][1]
    for (left_phase, left), (right_phase, right) in zip(curve, curve[1:], strict=False):
        if left_phase <= phase <= right_phase:
            progress = (phase - left_phase) / max(right_phase - left_phase, 1.0e-9)
            return left + (right - left) * progress
    raise AssertionError("phase interpolation interval was not found")


def analyze_parity(
    source_motion: Path,
    runtime_motion: Path,
    trace: Path,
    *,
    maximum_bone_error_metres: float = 0.08,
    maximum_planted_foot_excess_drift_metres: float = 0.03,
    minimum_distance_metres: float = 0.90,
    maximum_distance_metres: float = 1.10,
    maximum_terrain_height_range_metres: float = 0.02,
) -> dict[str, object]:
    source = MotionSampler(source_motion)
    runtime = MotionSampler(runtime_motion)
    frames = load_dodge_trace(trace)
    terrain_heights = [float(frame["terrain_height"]) for frame in frames]
    terrain_height_range = max(terrain_heights) - min(terrain_heights)
    curve, gameplay_distance = controller_curve(frames)
    phases = sorted(set(KEY_PHASES) | {float(frame["action_phase"]) for frame in frames})
    source_start = source.positions(0.0)
    runtime_start = runtime.positions(0.0)
    source_end = source.positions(1.0)
    authored_root_delta = source_end["root"] - source_start["root"]
    authored_root_delta[1] = 0.0
    if np.linalg.norm(authored_root_delta) <= 1.0e-6:
        raise ValueError("source quickstep has no authored lateral root displacement")

    phase_reports = []
    bone_errors: dict[str, list[float]] = {name: [] for name in TRACKED_BONES}

    def displaced_positions(phase: float) -> tuple[dict[str, np.ndarray], dict[str, np.ndarray]]:
        source_pose = source.positions(phase)
        runtime_pose = runtime.positions(phase)
        controller_progress = interpolate_curve(curve, phase)
        reference = {name: source_pose[name] - source_start[name] for name in TRACKED_BONES}
        candidate = {
            name: runtime_pose[name] - runtime_start[name] + authored_root_delta * controller_progress
            for name in TRACKED_BONES
        }
        return reference, candidate

    for phase in phases:
        reference, candidate = displaced_positions(phase)
        errors = {
            name: float(np.linalg.norm(candidate[name] - reference[name]))
            for name in TRACKED_BONES
        }
        for name, error in errors.items():
            bone_errors[name].append(error)
        phase_reports.append(
            {
                "phase": phase,
                "source_frame": phase * 12.0,
                "controller_progress": interpolate_curve(curve, phase),
                "maximum_bone_error_metres": max(errors.values()),
                "bone_errors_metres": errors,
            }
        )

    frame_nine_reference, frame_nine_candidate = displaced_positions(0.75)
    frame_twelve_reference, frame_twelve_candidate = displaced_positions(1.0)
    late_foot_drift = {}
    for name in ("l_foot", "r_foot"):
        reference_drift = float(
            np.linalg.norm(frame_twelve_reference[name] - frame_nine_reference[name])
        )
        candidate_drift = float(
            np.linalg.norm(frame_twelve_candidate[name] - frame_nine_candidate[name])
        )
        late_foot_drift[name] = {
            "authored_metres": reference_drift,
            "gameplay_metres": candidate_drift,
            "excess_metres": candidate_drift - reference_drift,
        }
    planted_name = min(late_foot_drift, key=lambda name: late_foot_drift[name]["authored_metres"])
    maximum_error = max(max(errors) for errors in bone_errors.values())
    rms_error = math.sqrt(
        sum(error * error for errors in bone_errors.values() for error in errors)
        / sum(len(errors) for errors in bone_errors.values())
    )
    planted_excess = late_foot_drift[planted_name]["excess_metres"]
    distance_valid = minimum_distance_metres <= gameplay_distance <= maximum_distance_metres
    bone_parity_valid = maximum_error <= maximum_bone_error_metres
    planted_foot_valid = planted_excess <= maximum_planted_foot_excess_drift_metres
    flat_terrain_valid = terrain_height_range <= maximum_terrain_height_range_metres
    return {
        "trace": str(trace),
        "source_motion": str(source_motion),
        "runtime_motion": str(runtime_motion),
        "gameplay_distance_metres": gameplay_distance,
        "required_gameplay_distance_metres": [minimum_distance_metres, maximum_distance_metres],
        "maximum_bone_error_metres": maximum_error,
        "rms_bone_error_metres": rms_error,
        "allowed_maximum_bone_error_metres": maximum_bone_error_metres,
        "late_authored_frames": [9, 12],
        "authored_planted_foot": planted_name,
        "late_foot_drift": late_foot_drift,
        "allowed_planted_foot_excess_drift_metres": maximum_planted_foot_excess_drift_metres,
        "terrain_height_range_metres": terrain_height_range,
        "allowed_terrain_height_range_metres": maximum_terrain_height_range_metres,
        "distance_valid": distance_valid,
        "bone_parity_valid": bone_parity_valid,
        "planted_foot_valid": planted_foot_valid,
        "flat_terrain_valid": flat_terrain_valid,
        "parity_valid": distance_valid
        and bone_parity_valid
        and planted_foot_valid
        and flat_terrain_valid,
        "phases": phase_reports,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument(
        "--source-motion",
        type=Path,
        default=Path("assets_src/biped/unarmed/quickstep_right.glb"),
    )
    parser.add_argument(
        "--runtime-motion",
        type=Path,
        default=Path("assets/animations/biped/unarmed/quickstep_right.glb"),
    )
    parser.add_argument("--report", type=Path)
    parser.add_argument("--maximum-bone-error-metres", type=float, default=0.08)
    parser.add_argument(
        "--maximum-planted-foot-excess-drift-metres", type=float, default=0.03
    )
    parser.add_argument(
        "--maximum-terrain-height-range-metres", type=float, default=0.02
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        result = analyze_parity(
            args.source_motion,
            args.runtime_motion,
            args.trace,
            maximum_bone_error_metres=args.maximum_bone_error_metres,
            maximum_planted_foot_excess_drift_metres=args.maximum_planted_foot_excess_drift_metres,
            maximum_terrain_height_range_metres=args.maximum_terrain_height_range_metres,
        )
    except (OSError, KeyError, IndexError, TypeError, ValueError) as error:
        print(f"quickstep parity analysis failed: {error}", file=sys.stderr)
        return 2
    rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.report is not None:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
    return 0 if result["parity_valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
