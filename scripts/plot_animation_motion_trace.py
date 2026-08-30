#!/usr/bin/env python3
"""Plot subject-relative bone motion over one attack chain from a live trace."""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import NamedTuple
from xml.etree import ElementTree


Vector3 = tuple[float, float, float]
Quaternion = tuple[float, float, float, float]


class MotionSample(NamedTuple):
    scenario_frame: int
    elapsed_seconds: float
    position: Vector3
    rotation: Quaternion
    action_sample: dict[str, object]


class MotionSeries(NamedTuple):
    times: list[float]
    linear_speed: list[float]
    angular_speed: list[float]
    linear_acceleration: list[float]
    angular_acceleration: list[float]


class PoseMarker(NamedTuple):
    time: float
    label: str


def vector_length(value: Vector3) -> float:
    return math.sqrt(sum(component * component for component in value))


def normalize_quaternion(value: list[float] | tuple[float, ...]) -> Quaternion:
    length = math.sqrt(sum(component * component for component in value))
    if not math.isfinite(length) or length <= 1.0e-9:
        raise ValueError("rotation is not a finite non-zero quaternion")
    return tuple(component / length for component in value)  # type: ignore[return-value]


def quaternion_inverse(value: Quaternion) -> Quaternion:
    x, y, z, w = normalize_quaternion(value)
    return (-x, -y, -z, w)


def quaternion_multiply(left: Quaternion, right: Quaternion) -> Quaternion:
    lx, ly, lz, lw = left
    rx, ry, rz, rw = right
    return (
        lw * rx + lx * rw + ly * rz - lz * ry,
        lw * ry - lx * rz + ly * rw + lz * rx,
        lw * rz + lx * ry - ly * rx + lz * rw,
        lw * rw - lx * rx - ly * ry - lz * rz,
    )


def rotate_vector(rotation: Quaternion, value: Vector3) -> Vector3:
    x, y, z, w = normalize_quaternion(rotation)
    vx, vy, vz = value
    tx = 2.0 * (y * vz - z * vy)
    ty = 2.0 * (z * vx - x * vz)
    tz = 2.0 * (x * vy - y * vx)
    return (
        vx + w * tx + (y * tz - z * ty),
        vy + w * ty + (z * tx - x * tz),
        vz + w * tz + (x * ty - y * tx),
    )


def subtract(left: list[float] | tuple[float, ...], right: list[float] | tuple[float, ...]) -> Vector3:
    return tuple(a - b for a, b in zip(left, right, strict=True))  # type: ignore[return-value]


def subject_relative_transform(frame: dict[str, object], bone: dict[str, object]) -> tuple[Vector3, Quaternion]:
    subject_rotation = normalize_quaternion(frame["subject_rotation_xyzw"])  # type: ignore[arg-type]
    inverse_subject = quaternion_inverse(subject_rotation)
    position = rotate_vector(
        inverse_subject,
        subtract(bone["translation"], frame["subject_translation"]),  # type: ignore[arg-type]
    )
    rotation = normalize_quaternion(
        quaternion_multiply(
            inverse_subject,
            normalize_quaternion(bone["rotation_xyzw"]),  # type: ignore[arg-type]
        )
    )
    return position, rotation


def angular_velocity(previous: Quaternion, current: Quaternion, seconds: float) -> Vector3:
    # current * inverse(previous) expresses the shortest delta in the stable
    # subject-relative coordinate frame, so successive vectors can be differenced.
    delta = normalize_quaternion(quaternion_multiply(current, quaternion_inverse(previous)))
    if delta[3] < 0.0:
        delta = tuple(-component for component in delta)  # type: ignore[assignment]
    vector_magnitude = vector_length(delta[:3])
    if vector_magnitude <= 1.0e-9:
        return (0.0, 0.0, 0.0)
    angle = 2.0 * math.atan2(vector_magnitude, delta[3])
    return tuple(
        component * angle / (vector_magnitude * seconds) for component in delta[:3]
    )  # type: ignore[return-value]


def evaluation_action_sample(frame: dict[str, object]) -> dict[str, object]:
    evaluation = frame.get("evaluation")
    if not isinstance(evaluation, dict):
        return {}
    action = evaluation.get("action")
    if not isinstance(action, list) or not action or not isinstance(action[0], dict):
        return {}
    return action[0]


def frame_is_attack(frame: dict[str, object]) -> bool:
    return frame.get("action") == "Attack" and bool(evaluation_action_sample(frame))


def load_attack_cycles(path: Path, bone_name: str) -> list[list[MotionSample]]:
    cycles: list[list[MotionSample]] = []
    active: list[MotionSample] = []
    previous_frame: int | None = None
    with path.open("r", encoding="utf-8") as trace:
        for line_number, line in enumerate(trace, 1):
            if not line.strip():
                continue
            try:
                frame = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"invalid JSON on line {line_number}: {error}") from error
            scenario_frame = int(frame["scenario_frame"])
            if not frame_is_attack(frame):
                if active:
                    cycles.append(active)
                    active = []
                previous_frame = scenario_frame
                continue
            bone = next(
                (candidate for candidate in frame.get("bones", []) if candidate.get("name") == bone_name),
                None,
            )
            if bone is None:
                raise ValueError(f"attack frame {scenario_frame} has no {bone_name!r} bone")
            if active and previous_frame is not None and scenario_frame != previous_frame + 1:
                cycles.append(active)
                active = []
            position, rotation = subject_relative_transform(frame, bone)
            active.append(
                MotionSample(
                    scenario_frame,
                    float(frame["elapsed_seconds"]),
                    position,
                    rotation,
                    evaluation_action_sample(frame),
                )
            )
            previous_frame = scenario_frame
    if active:
        cycles.append(active)
    return cycles


def calculate_motion(samples: list[MotionSample]) -> MotionSeries:
    if len(samples) < 3:
        raise ValueError("an attack cycle needs at least three rendered samples")
    start_time = samples[0].elapsed_seconds
    times = [sample.elapsed_seconds - start_time for sample in samples]
    linear_velocities: list[Vector3] = [(0.0, 0.0, 0.0)]
    angular_velocities: list[Vector3] = [(0.0, 0.0, 0.0)]
    for previous, current in zip(samples, samples[1:]):
        seconds = current.elapsed_seconds - previous.elapsed_seconds
        if not math.isfinite(seconds) or seconds <= 0.0:
            raise ValueError("attack samples must have strictly increasing elapsed_seconds")
        linear_velocities.append(
            tuple(component / seconds for component in subtract(current.position, previous.position))
        )
        angular_velocities.append(angular_velocity(previous.rotation, current.rotation, seconds))

    linear_accelerations: list[Vector3] = [(0.0, 0.0, 0.0)]
    angular_accelerations: list[Vector3] = [(0.0, 0.0, 0.0)]
    for index in range(1, len(samples)):
        seconds = samples[index].elapsed_seconds - samples[index - 1].elapsed_seconds
        linear_accelerations.append(
            tuple(
                (current - previous) / seconds
                for current, previous in zip(
                    linear_velocities[index], linear_velocities[index - 1], strict=True
                )
            )
        )
        angular_accelerations.append(
            tuple(
                (current - previous) / seconds
                for current, previous in zip(
                    angular_velocities[index], angular_velocities[index - 1], strict=True
                )
            )
        )
    return MotionSeries(
        times,
        [vector_length(value) for value in linear_velocities],
        [vector_length(value) for value in angular_velocities],
        [vector_length(value) for value in linear_accelerations],
        [vector_length(value) for value in angular_accelerations],
    )


def sampling(sample: MotionSample) -> tuple[str, dict[str, object]]:
    value = sample.action_sample.get("sampling")
    if isinstance(value, str):
        return value, {}
    if isinstance(value, dict) and len(value) == 1:
        kind, payload = next(iter(value.items()))
        return str(kind), payload if isinstance(payload, dict) else {}
    return "Unknown", {}


def pose_markers(samples: list[MotionSample]) -> list[PoseMarker]:
    start = samples[0].elapsed_seconds
    first_pose = str(samples[0].action_sample.get("pose", "attack start"))
    markers = [PoseMarker(0.0, first_pose)]
    contact_marked = False
    previous_kind, previous_payload = sampling(samples[0])
    previous_pose = first_pose
    previous = samples[0]
    for current in samples[1:]:
        current_kind, current_payload = sampling(current)
        current_pose = str(current.action_sample.get("pose", previous_pose))
        relative_time = current.elapsed_seconds - start
        if current_pose != previous_pose:
            markers.append(PoseMarker(relative_time, current_pose))
        if previous_kind == "CurveSpan" and current_kind == "CurveSpan" and not contact_marked:
            previous_coordinate = float(previous_payload.get("coordinate", 0.0))
            current_coordinate = float(current_payload.get("coordinate", 0.0))
            if previous_coordinate < 1.0 <= current_coordinate:
                fraction = (1.0 - previous_coordinate) / (current_coordinate - previous_coordinate)
                crossing = previous.elapsed_seconds + fraction * (
                    current.elapsed_seconds - previous.elapsed_seconds
                )
                markers.append(
                    PoseMarker(crossing - start, str(current_payload.get("end", "contact")))
                )
                contact_marked = True
        if current_kind == "ContinuationSpan" and previous_kind != "ContinuationSpan":
            markers.append(PoseMarker(relative_time, "full backswing (extrapolated)"))
        previous_kind, previous_payload = current_kind, current_payload
        previous_pose = current_pose
        previous = current

    last_kind, last_payload = sampling(samples[-1])
    end_pose = last_payload.get("end") if last_kind in {"Span", "CurveSpan"} else None
    if end_pose is not None:
        markers.append(PoseMarker(samples[-1].elapsed_seconds - start, str(end_pose)))
    markers.sort(key=lambda marker: marker.time)
    unique: list[PoseMarker] = []
    for marker in markers:
        if unique and abs(marker.time - unique[-1].time) < 1.0e-6 and marker.label == unique[-1].label:
            continue
        unique.append(marker)
    return unique


def nice_ceiling(value: float) -> float:
    if not math.isfinite(value) or value <= 0.0:
        return 1.0
    power = 10.0 ** math.floor(math.log10(value))
    scaled = value / power
    step = next(
        candidate
        for candidate in (1.0, 1.25, 1.5, 2.0, 2.5, 5.0, 10.0)
        if scaled <= candidate
    )
    return step * power


def svg_element(parent: ElementTree.Element, tag: str, **attributes: object) -> ElementTree.Element:
    return ElementTree.SubElement(parent, tag, {key.replace("_", "-"): str(value) for key, value in attributes.items()})


def svg_text(parent: ElementTree.Element, text: str, **attributes: object) -> None:
    node = svg_element(parent, "text", **attributes)
    node.text = text


def render_svg(
    output: Path,
    trace_name: str,
    bone_name: str,
    series: MotionSeries,
    markers: list[PoseMarker],
) -> None:
    width, height = 1400, 1140
    left, right, top, bottom = 110, 36, 142, 68
    gap = 28
    panel_height = (height - top - bottom - 3 * gap) / 4.0
    plot_width = width - left - right
    duration = max(series.times[-1], 1.0e-6)
    panels = (
        ("Linear speed", "m/s", series.linear_speed, "#2563eb"),
        ("Angular speed", "rad/s", series.angular_speed, "#7c3aed"),
        ("Linear acceleration", "m/s²", series.linear_acceleration, "#ea580c"),
        ("Angular acceleration", "rad/s²", series.angular_acceleration, "#dc2626"),
    )
    root = ElementTree.Element(
        "svg",
        {
            "xmlns": "http://www.w3.org/2000/svg",
            "width": str(width),
            "height": str(height),
            "viewBox": f"0 0 {width} {height}",
            "role": "img",
            "aria-labelledby": "title description",
        },
    )
    svg_element(root, "rect", x=0, y=0, width=width, height=height, fill="#ffffff")
    svg_text(root, f"{bone_name} attack-chain motion", id="title", x=left, y=28, font_size=22, font_weight=600)
    svg_text(
        root,
        f"Subject-relative motion from {trace_name}; vertical rules mark semantic pose anchors and transitions.",
        id="description",
        x=left,
        y=52,
        font_size=13,
        fill="#374151",
    )
    x_ticks = 6
    for panel_index, (title, unit, values, color) in enumerate(panels):
        panel_top = top + panel_index * (panel_height + gap)
        panel_bottom = panel_top + panel_height
        maximum = nice_ceiling(max(values) * 1.05)
        svg_element(root, "rect", x=left, y=panel_top, width=plot_width, height=panel_height, fill="none", stroke="#9ca3af", stroke_width=1)
        svg_text(root, title, x=left, y=panel_top - 9, font_size=15, font_weight=600)
        for tick in range(5):
            value = maximum * tick / 4.0
            y = panel_bottom - panel_height * tick / 4.0
            svg_element(root, "line", x1=left, y1=y, x2=width - right, y2=y, stroke="#e5e7eb", stroke_width=1)
            svg_text(root, f"{value:.2g}", x=left - 10, y=y + 4, text_anchor="end", font_size=11, fill="#4b5563")
        svg_text(root, unit, x=24, y=panel_top + panel_height / 2.0, text_anchor="middle", font_size=12, fill="#374151", transform=f"rotate(-90 24 {panel_top + panel_height / 2.0})")
        points = []
        for time, value in zip(series.times, values, strict=True):
            x = left + plot_width * time / duration
            y = panel_bottom - panel_height * value / maximum
            points.append(f"{x:.2f},{y:.2f}")
        svg_element(root, "polyline", points=" ".join(points), fill="none", stroke=color, stroke_width=2, stroke_linejoin="round", stroke_linecap="round")

    chart_bottom = top + 4 * panel_height + 3 * gap
    for marker_index, marker in enumerate(markers):
        x = left + plot_width * marker.time / duration
        svg_element(root, "line", x1=x, y1=top, x2=x, y2=chart_bottom, stroke="#111827", stroke_width=1, stroke_dasharray="5 4", opacity=0.72)
        label_y = 78 + 20 * (marker_index % 3)
        if marker.time <= duration * 0.02:
            label_x, anchor = x + 4, "start"
        elif marker.time >= duration * 0.98:
            label_x, anchor = x - 4, "end"
        else:
            label_x, anchor = x, "middle"
        svg_text(root, marker.label, x=label_x, y=label_y, text_anchor=anchor, font_size=11, fill="#111827")

    for tick in range(x_ticks + 1):
        value = duration * tick / x_ticks
        x = left + plot_width * tick / x_ticks
        svg_element(root, "line", x1=x, y1=chart_bottom, x2=x, y2=chart_bottom + 6, stroke="#111827", stroke_width=1)
        svg_text(root, f"{value:.2f}", x=x, y=chart_bottom + 24, text_anchor="middle", font_size=12, fill="#111827")
    svg_text(root, "Time since attack-chain start (s)", x=left + plot_width / 2.0, y=height - 20, text_anchor="middle", font_size=14, fill="#111827")
    output.parent.mkdir(parents=True, exist_ok=True)
    ElementTree.indent(root, space="  ")
    ElementTree.ElementTree(root).write(output, encoding="unicode", xml_declaration=False)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path, help="real-client animation-state JSONL")
    parser.add_argument("--output", type=Path, required=True, help="destination SVG")
    parser.add_argument("--bone", default="r_weapon", help="global bone name to measure")
    parser.add_argument("--cycle", type=int, default=-1, help="zero-based attack chain; negative indexes count from the end")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        cycles = load_attack_cycles(args.trace, args.bone)
        if not cycles:
            raise ValueError("trace contains no rendered attack cycles")
        try:
            samples = cycles[args.cycle]
        except IndexError as error:
            raise ValueError(f"cycle {args.cycle} is outside the {len(cycles)} captured attack cycles") from error
        series = calculate_motion(samples)
        markers = pose_markers(samples)
        render_svg(args.output, args.trace.name, args.bone, series, markers)
    except (OSError, KeyError, TypeError, ValueError) as error:
        print(f"animation motion graph failed: {error}", file=sys.stderr)
        return 2
    summary = {
        "attack_cycle_count": len(cycles),
        "bone": args.bone,
        "duration_seconds": series.times[-1],
        "markers": [{"time_seconds": marker.time, "pose": marker.label} for marker in markers],
        "maximums": {
            "linear_speed_metres_per_second": max(series.linear_speed),
            "angular_speed_radians_per_second": max(series.angular_speed),
            "linear_acceleration_metres_per_second_squared": max(series.linear_acceleration),
            "angular_acceleration_radians_per_second_squared": max(series.angular_acceleration),
        },
        "output": str(args.output.resolve()),
        "sample_count": len(series.times),
    }
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
