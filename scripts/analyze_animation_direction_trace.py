#!/usr/bin/env python3
"""Score steady raised-guard locomotion from a real-client animation trace.

The score uses the same derivative classes and thresholds as the Rust animation
jitter validator. It is continuous so two directions can be compared even when
both are on the same side of the validator's binary pass/fail boundary. A score
of 50 means the noisiest derivative's 95th percentile or the cadence deviation
reaches its allowed threshold; higher is smoother.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
import json
import math
from pathlib import Path
import sys
from typing import Iterable


THRESHOLDS = {
    "angular_acceleration": (240.0, 3.5, 8.0),
    "angular_jerk": (12_000.0, 3.5, 400.0),
    "local_position_acceleration": (18.0, 4.0, 0.5),
    "local_position_jerk": (900.0, 4.0, 25.0),
}

FRAME_OFFSETS = {
    "angular_acceleration": 2,
    "angular_jerk": 3,
    "local_position_acceleration": 2,
    "local_position_jerk": 3,
}

TRACKED_BONES = {
    "pelvis": "pelvis",
    "chest": "chest",
    "head": "head",
    "upper_arm.L": "left_shoulder",
    "upper_arm.R": "right_shoulder",
    "forearm.L": "left_elbow",
    "forearm.R": "right_elbow",
    "hand.L": "left_hand",
    "hand.R": "right_hand",
    "thigh.L": "left_hip",
    "thigh.R": "right_hip",
    "shin.L": "left_knee",
    "shin.R": "right_knee",
    "foot.L": "left_foot",
    "foot.R": "right_foot",
    "toe.L": "left_toe",
    "toe.R": "right_toe",
}

PARENTS = {
    "chest": "pelvis",
    "head": "chest",
    "left_shoulder": "chest",
    "right_shoulder": "chest",
    "left_elbow": "left_shoulder",
    "right_elbow": "right_shoulder",
    "left_hand": "left_elbow",
    "right_hand": "right_elbow",
    "left_hip": "pelvis",
    "right_hip": "pelvis",
    "left_knee": "left_hip",
    "right_knee": "right_hip",
    "left_foot": "left_knee",
    "right_foot": "right_knee",
    "left_toe": "left_foot",
    "right_toe": "right_foot",
}

DIRECTION_VECTORS = {
    "forward": (0.0, 1.0),
    "backward": (0.0, -1.0),
    "left": (-1.0, 0.0),
    "right": (1.0, 0.0),
}

LOWER_BODY_BONES = {
    "pelvis",
    "left_hip",
    "right_hip",
    "left_knee",
    "right_knee",
    "left_foot",
    "right_foot",
    "left_toe",
    "right_toe",
}

MAX_BENCHMARK_RENDER_DELTA_SECONDS = 0.05
MAX_BENCHMARK_SOURCE_TICK_GAP = 4
CATASTROPHIC_FOOT_HORIZONTAL_HIP_OFFSET_METRES = 0.65
CATASTROPHIC_FOOT_DISPLACEMENT_SECONDS = 0.1
MINIMUM_GUARD_SWING_TRAVEL_METRES = 0.05
MINIMUM_GUARD_SWING_CLEARANCE_GAIN_METRES = 0.03


def vector_length(value: Iterable[float]) -> float:
    return math.sqrt(sum(component * component for component in value))


def quaternion_normalize(value: list[float] | tuple[float, ...]) -> tuple[float, ...]:
    length = vector_length(value)
    if not math.isfinite(length) or length <= 1.0e-9:
        raise ValueError("bone rotation is not a finite non-zero quaternion")
    return tuple(component / length for component in value)


def quaternion_inverse(value: list[float] | tuple[float, ...]) -> tuple[float, ...]:
    x, y, z, w = quaternion_normalize(value)
    return (-x, -y, -z, w)


def quaternion_multiply(left: tuple[float, ...], right: tuple[float, ...]) -> tuple[float, ...]:
    lx, ly, lz, lw = left
    rx, ry, rz, rw = right
    return (
        lw * rx + lx * rw + ly * rz - lz * ry,
        lw * ry - lx * rz + ly * rw + lz * rx,
        lw * rz + lx * ry - ly * rx + lz * rw,
        lw * rw - lx * rx - ly * ry - lz * rz,
    )


def rotate_vector(rotation: tuple[float, ...], value: tuple[float, ...]) -> tuple[float, ...]:
    x, y, z, w = quaternion_normalize(rotation)
    vx, vy, vz = value
    tx = 2.0 * (y * vz - z * vy)
    ty = 2.0 * (z * vx - x * vz)
    tz = 2.0 * (x * vy - y * vx)
    return (
        vx + w * tx + (y * tz - z * ty),
        vy + w * ty + (z * tx - x * tz),
        vz + w * tz + (x * ty - y * tx),
    )


def quaternion_angle(left: tuple[float, ...], right: tuple[float, ...]) -> float:
    left = quaternion_normalize(left)
    right = quaternion_normalize(right)
    dot = min(1.0, abs(sum(a * b for a, b in zip(left, right, strict=True))))
    return 2.0 * math.acos(dot)


def direction_for(movement: object) -> str | None:
    if not isinstance(movement, list) or len(movement) != 2:
        return None
    try:
        x, y = float(movement[0]), float(movement[1])
    except (TypeError, ValueError):
        return None
    magnitude = math.hypot(x, y)
    if magnitude <= 1.0e-6:
        return None
    unit = (x / magnitude, y / magnitude)
    return max(
        DIRECTION_VECTORS,
        key=lambda name: unit[0] * DIRECTION_VECTORS[name][0]
        + unit[1] * DIRECTION_VECTORS[name][1],
    )


def parent_local_bones(
    frame: dict[str, object],
) -> dict[str, dict[str, tuple[float, ...]]]:
    global_bones = {}
    for bone in frame.get("bones", []):
        tracked = TRACKED_BONES.get(str(bone.get("name", "")))
        if tracked is None:
            continue
        global_bones[tracked] = {
            "position": tuple(float(value) for value in bone["translation"]),
            "rotation": quaternion_normalize(bone["rotation_xyzw"]),
        }
    local_bones = {}
    for name, bone in global_bones.items():
        parent_name = PARENTS.get(name)
        parent = global_bones.get(parent_name) if parent_name else None
        if parent is None:
            position = (0.0, 0.0, 0.0) if name == "pelvis" else bone["position"]
            rotation = bone["rotation"]
        else:
            inverse = quaternion_inverse(parent["rotation"])
            position = rotate_vector(
                inverse,
                tuple(
                    a - b
                    for a, b in zip(
                        bone["position"], parent["position"], strict=True
                    )
                ),
            )
            rotation = quaternion_multiply(inverse, bone["rotation"])
        local_bones[name] = {"position": position, "rotation": rotation}
    return local_bones


def sample_dt(frames: list[dict[str, object]], index: int) -> float:
    if index <= 0 or index >= len(frames):
        return 1.0 / 64.0
    return max(
        abs(float(frames[index]["elapsed_seconds"]) - float(frames[index - 1]["elapsed_seconds"])),
        1.0 / 1000.0,
    )


def scalar_derivative(values: list[float], frames: list[dict[str, object]]) -> list[float]:
    return [
        abs(values[index] - values[index - 1]) / sample_dt(frames, index)
        for index in range(1, len(values))
    ]


def vector_derivative(
    values: list[tuple[float, ...]], frames: list[dict[str, object]]
) -> list[tuple[float, ...]]:
    return [
        tuple(
            (a - b) / sample_dt(frames, index)
            for a, b in zip(values[index], values[index - 1], strict=True)
        )
        for index in range(1, len(values))
    ]


def upper_median(values: list[float]) -> float:
    finite = sorted(value for value in values if math.isfinite(value))
    return finite[len(finite) // 2] if finite else 0.0


def percentile(values: list[float], quantile: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = max(0, math.ceil(quantile * len(ordered)) - 1)
    return ordered[index]


def derivative_values(
    frames: list[dict[str, object]], bone: str
) -> dict[str, list[float]]:
    positions = [frame["local_bones"][bone]["position"] for frame in frames]
    rotations = [frame["local_bones"][bone]["rotation"] for frame in frames]
    position_velocity = vector_derivative(positions, frames)
    position_acceleration = [
        vector_length(value)
        for value in vector_derivative(position_velocity, frames[1:])
    ]
    angular_velocity = [
        quaternion_angle(rotations[index - 1], rotations[index])
        / sample_dt(frames, index)
        for index in range(1, len(rotations))
    ]
    angular_acceleration = scalar_derivative(angular_velocity, frames[1:])
    return {
        "local_position_acceleration": position_acceleration,
        "local_position_jerk": scalar_derivative(position_acceleration, frames),
        "angular_acceleration": angular_acceleration,
        "angular_jerk": scalar_derivative(angular_acceleration, frames[2:]),
    }


def coefficient_of_variation(values: list[float]) -> float | None:
    if len(values) < 2:
        return None
    mean = sum(values) / len(values)
    if abs(mean) <= 1.0e-9:
        return None
    variance = sum((value - mean) ** 2 for value in values) / len(values)
    return math.sqrt(variance) / abs(mean)


def cadence_summary(frames: list[dict[str, object]]) -> dict[str, object]:
    contact_events = []
    previous_sequence = None
    phase_rates = []
    previous_phase = None
    previous_time = None
    for frame in frames:
        presented = frame.get("presented") or {}
        sequence = presented.get("contact_sequence")
        time_seconds = float(frame["elapsed_seconds"])
        if (
            sequence is not None
            and previous_sequence is not None
            and sequence != previous_sequence
        ):
            contact_events.append(
                {
                    "time_seconds": time_seconds,
                    "scenario_frame": int(frame["scenario_frame"]),
                    "command_elapsed_seconds": float(
                        frame["command_elapsed_seconds"]
                    ),
                }
            )
        previous_sequence = sequence

        evaluation = frame.get("evaluation") or {}
        phase = evaluation.get("gait_phase")
        if phase is not None and previous_phase is not None and previous_time is not None:
            delta_time = max(time_seconds - previous_time, 1.0 / 1000.0)
            delta_phase = (float(phase) - previous_phase) % 1.0
            phase_rates.append(delta_phase / delta_time)
        if phase is not None:
            previous_phase = float(phase)
            previous_time = time_seconds

    half_step_windows = [
        {
            "duration_seconds": right["time_seconds"] - left["time_seconds"],
            "start_scenario_frame": left["scenario_frame"],
            "end_scenario_frame": right["scenario_frame"],
            "start_command_elapsed_seconds": left["command_elapsed_seconds"],
            "end_command_elapsed_seconds": right["command_elapsed_seconds"],
        }
        for left, right in zip(contact_events, contact_events[1:])
    ]
    half_step_intervals = [
        window["duration_seconds"] for window in half_step_windows
    ]
    shortest_window = (
        min(half_step_windows, key=lambda window: window["duration_seconds"])
        if half_step_windows
        else None
    )
    longest_window = (
        max(half_step_windows, key=lambda window: window["duration_seconds"])
        if half_step_windows
        else None
    )
    cadence_ratio = (
        max(half_step_intervals) / min(half_step_intervals)
        if len(half_step_intervals) >= 2 and min(half_step_intervals) > 0.0
        else None
    )
    return {
        "complete_half_step_count": len(half_step_intervals),
        "median_half_step_seconds": (
            upper_median(half_step_intervals) if half_step_intervals else None
        ),
        "longest_to_shortest_half_step_ratio": cadence_ratio,
        "cadence_threshold_ratio": (
            max(0.0, (cadence_ratio - 1.0) / (1.5 - 1.0))
            if cadence_ratio is not None
            else None
        ),
        "cadence_gate_passed": cadence_ratio is not None and cadence_ratio <= 1.5,
        "shortest_half_step_seconds": (
            min(half_step_intervals) if half_step_intervals else None
        ),
        "longest_half_step_seconds": (
            max(half_step_intervals) if half_step_intervals else None
        ),
        "shortest_half_step_window": shortest_window,
        "longest_half_step_window": longest_window,
        "phase_speed_coefficient_of_variation": coefficient_of_variation(phase_rates),
    }


def timing_summary(frames: list[dict[str, object]]) -> dict[str, object]:
    render_stalls = []
    source_tick_gaps = []
    previous_source_tick = None
    for frame in frames:
        render_delta = float(frame.get("render_delta_seconds", 0.0))
        if render_delta > MAX_BENCHMARK_RENDER_DELTA_SECONDS:
            render_stalls.append(
                {
                    "scenario_frame": int(frame["scenario_frame"]),
                    "render_delta_seconds": render_delta,
                }
            )
        authoritative = frame.get("authoritative") or {}
        source_tick = authoritative.get("locomotion_sample_tick")
        if source_tick is not None:
            source_tick = int(source_tick)
            if previous_source_tick is not None:
                gap = source_tick - previous_source_tick
                if gap > MAX_BENCHMARK_SOURCE_TICK_GAP:
                    source_tick_gaps.append(
                        {
                            "scenario_frame": int(frame["scenario_frame"]),
                            "source_tick_gap": gap,
                            "source_tick": source_tick,
                        }
                    )
            previous_source_tick = source_tick
    benchmark_valid = not render_stalls and not source_tick_gaps
    return {
        "benchmark_valid": benchmark_valid,
        "render_stall_count": len(render_stalls),
        "source_tick_gap_count": len(source_tick_gaps),
        "render_stalls": render_stalls,
        "source_tick_gaps": source_tick_gaps,
        "render_delta_limit_seconds": MAX_BENCHMARK_RENDER_DELTA_SECONDS,
        "source_tick_gap_limit": MAX_BENCHMARK_SOURCE_TICK_GAP,
    }


def frame_context(frame: dict[str, object]) -> dict[str, object]:
    evaluation = frame.get("evaluation") or {}
    authoritative = frame.get("authoritative") or {}
    return {
        "scenario_frame": int(frame["scenario_frame"]),
        "command_elapsed_seconds": float(frame["command_elapsed_seconds"]),
        "render_delta_seconds": float(frame.get("render_delta_seconds", 0.0)),
        "source_tick": authoritative.get("locomotion_sample_tick"),
        "gait_phase": evaluation.get("gait_phase"),
        "phase_source_changed": frame.get("presentation_phase_source_changed"),
        "phase_measurement_error": frame.get("presentation_phase_measurement_error"),
        "phase_correction_delta": frame.get("presentation_phase_correction_delta"),
    }


def catastrophic_stance_summary(frames: list[dict[str, object]]) -> dict[str, object]:
    maximum_offset = 0.0
    maximum_frame = None
    sustained_seconds = 0.0
    maximum_sustained_seconds = 0.0
    failed = False
    for index, frame in enumerate(frames):
        positions = {
            TRACKED_BONES[str(bone.get("name", ""))]: tuple(
                float(value) for value in bone["translation"]
            )
            for bone in frame.get("bones", [])
            if str(bone.get("name", "")) in TRACKED_BONES
        }
        rotation = quaternion_inverse(
            frame.get("subject_rotation_xyzw")
            or frame.get("controller_global_transform", {}).get("rotation_xyzw")
            or (0.0, 0.0, 0.0, 1.0)
        )
        frame_offset = 0.0
        for hip_name, foot_name in (
            ("left_hip", "left_foot"),
            ("right_hip", "right_foot"),
        ):
            hip = positions.get(hip_name)
            foot = positions.get(foot_name)
            if hip is None or foot is None:
                continue
            local_delta = rotate_vector(
                rotation,
                tuple(a - b for a, b in zip(foot, hip, strict=True)),
            )
            frame_offset = max(frame_offset, math.hypot(local_delta[0], local_delta[2]))
        if frame_offset > maximum_offset:
            maximum_offset = frame_offset
            maximum_frame = frame_context(frame)
        if frame_offset > CATASTROPHIC_FOOT_HORIZONTAL_HIP_OFFSET_METRES:
            sustained_seconds += sample_dt(frames, index)
            maximum_sustained_seconds = max(maximum_sustained_seconds, sustained_seconds)
            failed |= sustained_seconds >= CATASTROPHIC_FOOT_DISPLACEMENT_SECONDS
        else:
            sustained_seconds = 0.0
    return {
        "failed": failed,
        "maximum_horizontal_hip_foot_offset_metres": maximum_offset,
        "maximum_sustained_seconds": maximum_sustained_seconds,
        "offset_limit_metres": CATASTROPHIC_FOOT_HORIZONTAL_HIP_OFFSET_METRES,
        "duration_limit_seconds": CATASTROPHIC_FOOT_DISPLACEMENT_SECONDS,
        "maximum_frame": maximum_frame,
    }


def global_bone_positions(frame: dict[str, object]) -> dict[str, tuple[float, ...]]:
    return {
        TRACKED_BONES[str(bone.get("name", ""))]: tuple(
            float(value) for value in bone["translation"]
        )
        for bone in frame.get("bones", [])
        if str(bone.get("name", "")) in TRACKED_BONES
    }


def bone_terrain_clearance(frame: dict[str, object], tracked_name: str) -> float | None:
    for bone in frame.get("bones", []):
        if TRACKED_BONES.get(str(bone.get("name", ""))) != tracked_name:
            continue
        clearance = bone.get("terrain_clearance_metres")
        if clearance is not None:
            return float(clearance)
        translation = bone.get("translation")
        if isinstance(translation, list) and len(translation) == 3:
            return float(translation[1])
    return None


def guard_step_liveness_summary(frames: list[dict[str, object]]) -> dict[str, object]:
    contact_indices = []
    previous_sequence = None
    diagnostics_complete = True
    for index, frame in enumerate(frames):
        presented = frame.get("presented") or {}
        sequence = presented.get("contact_sequence")
        contact_foot = presented.get("contact_foot")
        positions = global_bone_positions(frame)
        diagnostics_complete &= (
            sequence is not None
            and contact_foot is not None
            and "left_foot" in positions
            and "right_foot" in positions
        )
        if (
            sequence is not None
            and previous_sequence is not None
            and sequence != previous_sequence
        ):
            contact_indices.append(index)
        previous_sequence = sequence

    completed_half_steps = max(0, len(contact_indices) - 1)
    visible_half_steps = 0
    minimum_travel = math.inf
    minimum_clearance_gain = math.inf
    evidence = []
    for start_index, end_index in zip(contact_indices, contact_indices[1:]):
        end_presented = frames[end_index].get("presented") or {}
        end_contact = str(end_presented.get("contact_foot", "")).lower()
        swing_foot = "left_foot" if end_contact == "right" else "right_foot"
        interval_positions = [
            global_bone_positions(frame).get(swing_foot)
            for frame in frames[start_index : end_index + 1]
        ]
        if any(position is None for position in interval_positions):
            diagnostics_complete = False
            continue
        positions = [position for position in interval_positions if position is not None]
        clearances = [
            bone_terrain_clearance(frame, swing_foot)
            for frame in frames[start_index : end_index + 1]
        ]
        if any(clearance is None for clearance in clearances):
            diagnostics_complete = False
            continue
        terrain_clearances = [
            clearance for clearance in clearances if clearance is not None
        ]
        travel = math.hypot(
            positions[-1][0] - positions[0][0],
            positions[-1][2] - positions[0][2],
        )
        clearance_gain = max(terrain_clearances) - max(
            terrain_clearances[0], terrain_clearances[-1]
        )
        visible = (
            travel >= MINIMUM_GUARD_SWING_TRAVEL_METRES
            and clearance_gain >= MINIMUM_GUARD_SWING_CLEARANCE_GAIN_METRES
        )
        visible_half_steps += int(visible)
        minimum_travel = min(minimum_travel, travel)
        minimum_clearance_gain = min(minimum_clearance_gain, clearance_gain)
        evidence.append(
            {
                "start_scenario_frame": int(frames[start_index]["scenario_frame"]),
                "end_scenario_frame": int(frames[end_index]["scenario_frame"]),
                "swing_foot": swing_foot,
                "travel_metres": travel,
                "clearance_gain_metres": clearance_gain,
                "visible": visible,
            }
        )

    passed = (
        diagnostics_complete
        and completed_half_steps > 0
        and visible_half_steps == completed_half_steps
    )
    return {
        "passed": passed,
        "diagnostics_complete": diagnostics_complete,
        "completed_half_step_count": completed_half_steps,
        "visible_half_step_count": visible_half_steps,
        "minimum_swing_travel_metres": (
            minimum_travel if math.isfinite(minimum_travel) else 0.0
        ),
        "minimum_swing_clearance_gain_metres": (
            minimum_clearance_gain
            if math.isfinite(minimum_clearance_gain)
            else 0.0
        ),
        "travel_limit_metres": MINIMUM_GUARD_SWING_TRAVEL_METRES,
        "clearance_gain_limit_metres": MINIMUM_GUARD_SWING_CLEARANCE_GAIN_METRES,
        "half_steps": evidence,
    }


def score_direction(frames: list[dict[str, object]]) -> dict[str, object]:
    common_bones = set.intersection(*(set(frame["local_bones"]) for frame in frames))
    metrics: dict[str, dict[str, object]] = {}
    region_worst = {
        "lower_body": {"p95_threshold_ratio": 0.0},
        "upper_body": {"p95_threshold_ratio": 0.0},
    }
    all_incidents = 0
    for metric, (absolute, relative, noise_floor) in THRESHOLDS.items():
        bone_summaries = []
        for bone in sorted(common_bones):
            values = derivative_values(frames, bone)[metric]
            threshold = max(absolute, relative * min(upper_median(values), absolute))
            ratios = [
                value / threshold if value > noise_floor else 0.0
                for value in values
            ]
            peak_index = max(range(len(ratios)), key=ratios.__getitem__)
            peak_frame = frames[
                min(peak_index + FRAME_OFFSETS[metric], len(frames) - 1)
            ]
            incident_count = sum(
                value > noise_floor and value > threshold for value in values
            )
            all_incidents += incident_count
            bone_summaries.append(
                {
                    "bone": bone,
                    "p95_threshold_ratio": percentile(ratios, 0.95),
                    "peak_threshold_ratio": max(ratios, default=0.0),
                    "peak_scenario_frame": int(peak_frame["scenario_frame"]),
                    "peak_command_elapsed_seconds": float(
                        peak_frame["command_elapsed_seconds"]
                    ),
                    "peak_context": frame_context(peak_frame),
                    "incident_count": incident_count,
                    "threshold": threshold,
                }
            )
        worst = max(bone_summaries, key=lambda item: item["p95_threshold_ratio"])
        peak = max(bone_summaries, key=lambda item: item["peak_threshold_ratio"])
        for bone_summary in bone_summaries:
            region = (
                "lower_body"
                if bone_summary["bone"] in LOWER_BODY_BONES
                else "upper_body"
            )
            if (
                bone_summary["p95_threshold_ratio"]
                > region_worst[region]["p95_threshold_ratio"]
            ):
                region_worst[region] = {
                    "p95_threshold_ratio": bone_summary["p95_threshold_ratio"],
                    "worst_bone": bone_summary["bone"],
                    "worst_metric": metric,
                }
        metrics[metric] = {
            "worst_bone": worst["bone"],
            "p95_threshold_ratio": worst["p95_threshold_ratio"],
            "peak_threshold_ratio": peak["peak_threshold_ratio"],
            "peak_bone": peak["bone"],
            "peak_scenario_frame": peak["peak_scenario_frame"],
            "peak_command_elapsed_seconds": peak["peak_command_elapsed_seconds"],
            "peak_context": peak["peak_context"],
            "incident_count": sum(item["incident_count"] for item in bone_summaries),
        }
    worst_metric, worst = max(
        metrics.items(), key=lambda item: item[1]["p95_threshold_ratio"]
    )
    p95_ratio = float(worst["p95_threshold_ratio"])
    cadence = cadence_summary(frames)
    timing = timing_summary(frames)
    catastrophic_stance = catastrophic_stance_summary(frames)
    guard_step_liveness = guard_step_liveness_summary(frames)
    cadence_threshold_ratio = cadence["cadence_threshold_ratio"]
    score_basis = {
        "worst_metric": worst_metric,
        "worst_bone": worst["worst_bone"],
        "normalized_threshold_ratio": p95_ratio,
    }
    if (
        cadence_threshold_ratio is not None
        and cadence_threshold_ratio > p95_ratio
    ):
        p95_ratio = float(cadence_threshold_ratio)
        score_basis = {
            "worst_metric": "cadence_ratio",
            "worst_bone": "step_timing",
            "normalized_threshold_ratio": p95_ratio,
        }
    jitter_passed = all_incidents == 0 and cadence["cadence_gate_passed"]
    motion_smoothness_score = 100.0 / (1.0 + p95_ratio)
    hard_failure = catastrophic_stance["failed"] or not guard_step_liveness["passed"]
    quality_score = 0.0 if hard_failure else motion_smoothness_score
    if catastrophic_stance["failed"]:
        score_basis = {
            "worst_metric": "catastrophic_horizontal_foot_displacement",
            "worst_bone": "foot",
            "normalized_threshold_ratio": (
                catastrophic_stance["maximum_horizontal_hip_foot_offset_metres"]
                / CATASTROPHIC_FOOT_HORIZONTAL_HIP_OFFSET_METRES
            ),
        }
    elif not guard_step_liveness["passed"]:
        score_basis = {
            "worst_metric": "guard_step_liveness",
            "worst_bone": "foot",
            "normalized_threshold_ratio": 1.0,
        }
    regions = {
        name: {
            **summary,
            "smoothness_score": 100.0
            / (1.0 + float(summary["p95_threshold_ratio"])),
        }
        for name, summary in region_worst.items()
    }
    return {
        "quality_score": quality_score,
        "motion_smoothness_score": motion_smoothness_score,
        "smoothness_score": quality_score,
        "score_basis": score_basis,
        "jitter_gate_passed": jitter_passed,
        "derivative_incident_count": all_incidents,
        "maximum_legacy_quality_percent_if_other_categories_pass": (
            100.0 if jitter_passed else 100.0 * (1.0 - 1.0 / 31.0)
        ),
        "sampled_frames": len(frames),
        "sampled_seconds": float(frames[-1]["elapsed_seconds"])
        - float(frames[0]["elapsed_seconds"]),
        "cadence": cadence,
        "timing": timing,
        "catastrophic_stance": catastrophic_stance,
        "guard_step_liveness": guard_step_liveness,
        "regions": regions,
        "metrics": metrics,
    }


def analyze_trace(
    path: Path, trim_start_seconds: float = 1.0, trim_end_seconds: float = 0.5
) -> dict[str, object]:
    segments: dict[tuple[int, str], list[dict[str, object]]] = defaultdict(list)
    raised_guard_frames: list[dict[str, object]] = []
    with path.open("r", encoding="utf-8") as trace:
        for line_number, line in enumerate(trace, 1):
            if not line.strip():
                continue
            try:
                frame = json.loads(line)
                input_status = frame.get("input") or {}
                request = input_status.get("request") or {}
                frame["command_elapsed_seconds"] = float(
                    input_status["command_elapsed_seconds"]
                )
                frame["local_bones"] = parent_local_bones(frame)
                if request.get("weapon_guard") == "Raised":
                    raised_guard_frames.append(frame)
                direction = direction_for(request.get("movement"))
                if (
                    input_status.get("command_kind") != "move"
                    or direction is None
                    or request.get("weapon_guard") != "Raised"
                ):
                    continue
                segments[(int(input_status["command_index"]), direction)].append(frame)
            except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
                raise ValueError(f"invalid trace frame on line {line_number}: {error}") from error

    by_direction: dict[str, list[dict[str, object]]] = {}
    for (_, direction), frames in sorted(segments.items()):
        latest = max(frame["command_elapsed_seconds"] for frame in frames)
        steady = [
            frame
            for frame in frames
            if trim_start_seconds
            <= frame["command_elapsed_seconds"]
            <= latest - trim_end_seconds
        ]
        if direction in by_direction:
            raise ValueError(
                f"trace contains more than one move command for {direction}"
            )
        if len(steady) < 4:
            raise ValueError(f"{direction} has fewer than four steady-state frames")
        by_direction[direction] = steady

    missing = set(DIRECTION_VECTORS) - set(by_direction)
    if missing:
        raise ValueError(
            f"trace is missing raised-guard movement: {', '.join(sorted(missing))}"
        )
    scores = {
        direction: score_direction(by_direction[direction])
        for direction in DIRECTION_VECTORS
    }
    ranking = sorted(
        scores,
        key=lambda direction: scores[direction]["quality_score"],
        reverse=True,
    )
    benchmark_valid = all(
        score["timing"]["benchmark_valid"] for score in scores.values()
    )
    guard_catastrophic_stance = catastrophic_stance_summary(raised_guard_frames)
    benchmark_quality_score = (
        0.0
        if guard_catastrophic_stance["failed"]
        else min(score["quality_score"] for score in scores.values())
    )
    return {
        "trace": str(path.resolve()),
        "score_definition": (
            "zero for catastrophic horizontal hip-foot displacement or a missing "
            "visible guard step; otherwise "
            "100 / (1 + worst derivative p95 or normalized cadence threshold ratio)"
        ),
        "steady_state_trim_seconds": {
            "start": trim_start_seconds,
            "end": trim_end_seconds,
        },
        "ranking_smoothest_to_roughest": ranking,
        "benchmark_valid": benchmark_valid,
        "benchmark_quality_score": benchmark_quality_score,
        "guard_catastrophic_stance": guard_catastrophic_stance,
        "directions": scores,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--trim-start-seconds", type=float, default=1.0)
    parser.add_argument("--trim-end-seconds", type=float, default=0.5)
    parser.add_argument(
        "--summary",
        action="store_true",
        help="print compact per-direction scores instead of the full analysis",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.trim_start_seconds < 0.0 or args.trim_end_seconds < 0.0:
        raise SystemExit("steady-state trims must be non-negative")
    try:
        result = analyze_trace(
            args.trace, args.trim_start_seconds, args.trim_end_seconds
        )
    except (OSError, ValueError) as error:
        print(f"direction trace analysis failed: {error}", file=sys.stderr)
        return 2
    if args.summary:
        print(f"benchmark_valid={result['benchmark_valid']}")
        guard_stance = result["guard_catastrophic_stance"]
        print(
            f"benchmark_quality={result['benchmark_quality_score']:.2f} "
            f"guard_catastrophic={guard_stance['failed']} "
            "guard_max_horizontal_hip_foot_metres="
            f"{guard_stance['maximum_horizontal_hip_foot_offset_metres']:.3f}"
        )
        for direction, score in result["directions"].items():
            stance = score["catastrophic_stance"]
            liveness = score["guard_step_liveness"]
            timing = score["timing"]
            print(
                f"{direction}: quality={score['quality_score']:.2f} "
                f"motion={score['motion_smoothness_score']:.2f} "
                f"catastrophic={stance['failed']} "
                "max_horizontal_hip_foot_metres="
                f"{stance['maximum_horizontal_hip_foot_offset_metres']:.3f} "
                f"visible_steps={liveness['visible_half_step_count']}/"
                f"{liveness['completed_half_step_count']} "
                f"timing_valid={timing['benchmark_valid']} "
                f"render_stalls={len(timing['render_stalls'])} "
                f"source_tick_gaps={len(timing['source_tick_gaps'])}"
            )
    else:
        print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["benchmark_valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
