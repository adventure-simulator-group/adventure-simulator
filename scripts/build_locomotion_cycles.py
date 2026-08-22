#!/usr/bin/env python3
"""Bake continuous runtime locomotion cycles from authored semantic poses.

The source walk and run files own the left contact and passing/flight poses.
Their exported in-betweens are not runtime data. This builder combines those
two poses with their character-space mirrors into a closed 64-frame cycle.
Requires Python 3 and NumPy. Run from the repository root:

    python scripts/build_locomotion_cycles.py
    python scripts/mirror_gait_assets.py
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np

from prepare_animation_motion import (
    append_float_accessor,
    optimize_animation,
    strip_motion_mesh,
)
from mirror_gait_assets import (
    ASSET_DIR,
    REFLECTION,
    accessor_view,
    compose,
    encode_glb,
    mhr_bilateral_pairs,
    mirrored_glb,
    mirrored_pose_globals,
    read_glb,
    resolve_global_matrices,
)


ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIR = ROOT / "assets_src" / "biped" / "unarmed"
CYCLE_FRAMES = 64
MOTIONS = ("walk", "run")


def decode_glb_bytes(payload: bytes, temporary: Path) -> tuple[dict, bytearray]:
    temporary.write_bytes(payload)
    try:
        return read_glb(temporary)
    finally:
        temporary.unlink(missing_ok=True)


def channel_values(document: dict, binary: bytearray) -> dict[tuple[int, str], np.ndarray]:
    animation = document["animations"][0]
    return {
        (channel["target"]["node"], channel["target"]["path"]): accessor_view(
            document,
            binary,
            animation["samplers"][channel["sampler"]]["output"],
        )
        for channel in animation["channels"]
    }


def slerp(left: np.ndarray, right: np.ndarray, factor: float) -> np.ndarray:
    left = left.astype(np.float64)
    right = right.astype(np.float64)
    left /= np.linalg.norm(left)
    right /= np.linalg.norm(right)
    dot = float(np.dot(left, right))
    if dot < 0.0:
        right = -right
        dot = -dot
    if dot > 0.9995:
        result = left + (right - left) * factor
        return (result / np.linalg.norm(result)).astype("<f4")
    angle = np.arccos(np.clip(dot, -1.0, 1.0))
    result = (
        np.sin((1.0 - factor) * angle) * left
        + np.sin(factor * angle) * right
    ) / np.sin(angle)
    return result.astype("<f4")


def smoothstep(value: float) -> float:
    return value * value * (3.0 - 2.0 * value)


def build_cycle(source: Path, *, mirrored_start: bool = False) -> bytes:
    document, binary = read_glb(source)
    temporary = source.with_suffix(".mirrored-cycle-tmp.glb")
    mirrored_document, mirrored_binary = decode_glb_bytes(mirrored_glb(source), temporary)
    values = channel_values(document, binary)
    mirrored_values = channel_values(mirrored_document, mirrored_binary)
    if mirrored_start:
        values, mirrored_values = mirrored_values, values
    if values.keys() != mirrored_values.keys():
        raise ValueError(f"{source} and its mirror have different animation channels")
    sample_counts = {output.shape[0] for output in values.values()}
    if len(sample_counts) != 1:
        raise ValueError(f"{source} animation channels must share one sample count")
    source_samples = sample_counts.pop()
    if source_samples < 2:
        raise ValueError(f"{source} must expose a contact and passing/flight sample")
    passing_frame = source_samples - 1

    anchors = (0, 16, 32, 48, 64)
    generated_values: dict[tuple[int, str], np.ndarray] = {}
    for key, canonical in values.items():
        mirrored = mirrored_values[key]
        poses = np.asarray(
            (
                canonical[0],
                canonical[passing_frame],
                mirrored[0],
                mirrored[passing_frame],
                canonical[0],
            ),
            dtype="<f4",
        )
        if key[1] == "rotation":
            # A quaternion and its negation encode the same pose, but cubic
            # component interpolation must follow one continuous hemisphere.
            for index in range(1, len(poses)):
                if np.dot(poses[index - 1], poses[index]) < 0.0:
                    poses[index] *= -1.0
        # glTF cubic keys are in-tangent, value, out-tangent triples. Zero
        # tangents produce the same scalar smoothstep used by the old 65-key
        # bake, with Bevy normalizing quaternion results after interpolation.
        output = np.zeros((len(poses) * 3, canonical.shape[1]), dtype="<f4")
        output[1::3] = poses
        generated_values[key] = output

    timestamps = np.asarray(anchors, dtype="<f4") / 30.0
    timestamp_accessor = append_float_accessor(
        document,
        binary,
        timestamps.reshape(-1, 1),
        "SCALAR",
        minimum=[0.0],
        maximum=[CYCLE_FRAMES / 30.0],
    )
    output_accessors = {
        key: append_float_accessor(
            document,
            binary,
            output,
            "VEC4" if key[1] == "rotation" else "VEC3",
        )
        for key, output in generated_values.items()
    }
    animation = document["animations"][0]
    for channel in animation["channels"]:
        target = channel["target"]
        sampler = animation["samplers"][channel["sampler"]]
        sampler["input"] = timestamp_accessor
        sampler["output"] = output_accessors[(target["node"], target["path"])]
        sampler["interpolation"] = "CUBICSPLINE"
    document["buffers"][0]["byteLength"] = len(binary)
    optimized_document, optimized_binary = optimize_animation(document, binary)
    stripped_document, stripped_binary = strip_motion_mesh(
        optimized_document, optimized_binary
    )
    return encode_glb(stripped_document, bytearray(stripped_binary))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale: list[Path] = []
    for motion in MOTIONS:
        output = ASSET_DIR / f"{motion}.glb"
        generated = build_cycle(SOURCE_DIR / f"{motion}.glb")
        if args.check:
            if not output.exists() or output.read_bytes() != generated:
                stale.append(output)
        else:
            output.write_bytes(generated)
            print(output.relative_to(ROOT))
    if stale:
        names = ", ".join(str(path.relative_to(ROOT)) for path in stale)
        raise SystemExit(f"stale locomotion cycles: {names}")


if __name__ == "__main__":
    main()
