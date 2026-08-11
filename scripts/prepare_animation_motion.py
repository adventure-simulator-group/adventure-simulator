#!/usr/bin/env python3
"""Validate and byte-copy one independently exported animation motion GLB."""

from __future__ import annotations

import argparse
import pathlib
import sys

from prepare_rig_base import GlbError, read_glb


ANIMATION_FPS = 30.0


def scene_paths(document: dict) -> dict[int, tuple[str, ...]]:
    try:
        nodes = document["nodes"]
        scene = document["scenes"][document.get("scene", 0)]
        roots = scene["nodes"]
    except (KeyError, IndexError, TypeError) as error:
        raise GlbError("GLB scene hierarchy is malformed") from error

    paths: dict[int, tuple[str, ...]] = {}
    active: set[int] = set()

    def visit(index: int, parent_path: tuple[str, ...]) -> None:
        if not isinstance(index, int) or not 0 <= index < len(nodes):
            raise GlbError("GLB scene references an invalid node")
        if index in active:
            raise GlbError("GLB scene hierarchy contains a cycle")
        node = nodes[index]
        if not isinstance(node, dict) or not isinstance(node.get("name"), str):
            raise GlbError(f"animation node {index} must have an explicit name")
        path = (*parent_path, node["name"])
        previous = paths.setdefault(index, path)
        if previous != path:
            raise GlbError(f"animation node {node['name']} has multiple scene parents")
        active.add(index)
        for child in node.get("children", []):
            visit(child, path)
        active.remove(index)

    for root in roots:
        visit(root, ())
    return paths


def canonical_base_paths(base: dict) -> set[tuple[str, ...]]:
    paths = scene_paths(base)
    skeleton_roots = [index for index, path in paths.items() if path == ("Skeleton",)]
    if len(skeleton_roots) != 1:
        raise GlbError("base GLB must have exactly one Skeleton scene root")
    root = skeleton_roots[0]
    canonical = {path for path in paths.values() if path[:1] == ("Skeleton",)}
    try:
        skins = base["skins"]
        joints = skins[0]["joints"]
    except (KeyError, IndexError, TypeError) as error:
        raise GlbError("base GLB skin is malformed") from error
    joint_paths = {paths.get(index) for index in joints}
    if None in joint_paths or joint_paths | {("Skeleton",)} != canonical:
        raise GlbError("base skin joints must exactly fill the Skeleton hierarchy")
    if root in joints:
        raise GlbError("Skeleton scene root must not also be a skin joint")
    return canonical


def validate_motion(
    base: dict,
    motion: dict,
    *,
    last_frame: int,
    fps: float = ANIMATION_FPS,
) -> tuple[float, int]:
    animations = motion.get("animations")
    if not isinstance(animations, list) or len(animations) != 1:
        count = len(animations) if isinstance(animations, list) else 0
        raise GlbError(f"motion GLB must contain exactly one animation, found {count}")
    if last_frame < 0 or fps <= 0.0:
        raise GlbError("last frame and FPS must define a non-negative timeline")

    base_paths = canonical_base_paths(base)
    paths = scene_paths(motion)
    motion_skeleton = {path for path in paths.values() if path[:1] == ("Skeleton",)}
    if not base_paths.issubset(motion_skeleton):
        missing = sorted(base_paths - motion_skeleton)
        raise GlbError(f"motion GLB is missing canonical base path {missing[0]}")

    animation = animations[0]
    try:
        channels = animation["channels"]
        samplers = animation["samplers"]
        accessors = motion["accessors"]
    except (KeyError, TypeError) as error:
        raise GlbError("motion animation is malformed") from error
    target_paths: set[tuple[str, ...]] = set()
    for channel in channels:
        try:
            index = channel["target"]["node"]
            path = paths[index]
        except (KeyError, IndexError, TypeError) as error:
            raise GlbError("motion channel targets an invalid scene node") from error
        if path not in base_paths:
            raise GlbError(f"motion channel targets foreign base path {path}")
        target_paths.add(path)
    if not target_paths:
        raise GlbError("motion animation must target at least one canonical node")

    duration = 0.0
    for sampler in samplers:
        try:
            maximum = accessors[sampler["input"]]["max"]
            timestamp = float(maximum[0])
        except (KeyError, IndexError, TypeError, ValueError) as error:
            raise GlbError("motion timestamp accessor must declare a numeric maximum") from error
        duration = max(duration, timestamp)
    required = last_frame / fps
    if duration + 0.5 / fps < required:
        raise GlbError(
            f"motion duration {duration:.6f}s does not cover frame {last_frame} "
            f"at {fps:g}fps ({required:.6f}s)"
        )
    return duration, len(target_paths)


def prepare_motion(
    source: pathlib.Path,
    base: pathlib.Path,
    destination: pathlib.Path,
    *,
    last_frame: int,
    check: bool = False,
) -> tuple[float, int]:
    base_document, _ = read_glb(base)
    motion_document, _ = read_glb(source)
    result = validate_motion(base_document, motion_document, last_frame=last_frame)
    source_bytes = source.read_bytes()
    if check:
        if not destination.is_file() or destination.read_bytes() != source_bytes:
            raise GlbError(f"{destination} is not an exact prepared copy of {source}")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.is_file() or destination.read_bytes() != source_bytes:
            destination.write_bytes(source_bytes)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("base", type=pathlib.Path)
    parser.add_argument("destination", type=pathlib.Path)
    parser.add_argument("--last-frame", type=int, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        duration, targets = prepare_motion(
            args.source,
            args.base,
            args.destination,
            last_frame=args.last_frame,
            check=args.check,
        )
    except (GlbError, OSError) as error:
        parser.error(str(error))
    action = "verified" if args.check else "prepared"
    print(f"{action} {args.destination} ({duration:.6f}s, {targets} animation targets)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
