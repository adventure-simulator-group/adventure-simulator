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

from mirror_gait_assets import (
    ASSET_DIR,
    accessor_view,
    encode_glb,
    mirrored_glb,
    read_glb,
)


ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIR = ROOT / "assets_src" / "biped" / "unarmed"
CYCLE_FRAMES = 64
MOTIONS = {"walk": 8, "run": 5}


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


def build_cycle(source: Path, passing_frame: int) -> bytes:
    document, binary = read_glb(source)
    temporary = source.with_suffix(".mirrored-cycle-tmp.glb")
    mirrored_document, mirrored_binary = decode_glb_bytes(mirrored_glb(source), temporary)
    values = channel_values(document, binary)
    mirrored_values = channel_values(mirrored_document, mirrored_binary)
    if values.keys() != mirrored_values.keys():
        raise ValueError(f"{source} and its mirror have different animation channels")

    anchors = (0, 16, 32, 48, 64)
    for key, output in values.items():
        if output.shape[0] < CYCLE_FRAMES + 1:
            raise ValueError(f"{source} must expose at least 65 dense animation samples")
        canonical = output.copy()
        mirrored = mirrored_values[key]
        poses = (
            canonical[0],
            canonical[passing_frame],
            mirrored[0],
            mirrored[passing_frame],
            canonical[0],
        )
        for segment in range(4):
            start, end = anchors[segment], anchors[segment + 1]
            for frame in range(start, end + 1):
                factor = smoothstep((frame - start) / (end - start))
                output[frame] = (
                    slerp(poses[segment], poses[segment + 1], factor)
                    if key[1] == "rotation"
                    else poses[segment] + (poses[segment + 1] - poses[segment]) * factor
                )
    return encode_glb(document, binary)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale: list[Path] = []
    for motion, passing_frame in MOTIONS.items():
        output = ASSET_DIR / f"{motion}.glb"
        generated = build_cycle(SOURCE_DIR / f"{motion}.glb", passing_frame)
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
