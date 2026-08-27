#!/usr/bin/env python3
"""Validate and compact one independently exported animation motion GLB."""

from __future__ import annotations

import argparse
import copy
import pathlib
import sys

import numpy as np

from prepare_rig_base import GlbError, encode_glb, read_glb


ANIMATION_FPS = 30.0
ROOT_PATH = ("Skeleton", "body_world", "root")


def accessor_view(document: dict, binary: bytes, index: int) -> np.ndarray:
    """Expose one dense FLOAT accessor as a read-only NumPy view."""
    try:
        accessor = document["accessors"][index]
        view = document["bufferViews"][accessor["bufferView"]]
        width = {"SCALAR": 1, "VEC3": 3, "VEC4": 4}[accessor["type"]]
    except (KeyError, IndexError, TypeError) as error:
        raise GlbError("animation accessor is malformed") from error
    if accessor.get("componentType") != 5126 or "sparse" in accessor:
        raise GlbError("animation accessors must use dense FLOAT data")
    stride = view.get("byteStride", width * 4)
    if stride != width * 4:
        raise GlbError("interleaved animation accessors are not supported")
    offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    try:
        return np.ndarray(
            (accessor["count"], width), dtype="<f4", buffer=binary, offset=offset
        )
    except (TypeError, ValueError) as error:
        raise GlbError("animation accessor exceeds the GLB binary chunk") from error


def append_float_accessor(
    document: dict,
    binary: bytearray,
    values: np.ndarray,
    accessor_type: str,
    *,
    minimum: list[float] | None = None,
    maximum: list[float] | None = None,
) -> int:
    values = np.ascontiguousarray(values, dtype="<f4")
    binary.extend(b"\0" * (-len(binary) % 4))
    offset = len(binary)
    payload = values.tobytes()
    binary.extend(payload)
    view_index = len(document.setdefault("bufferViews", []))
    document["bufferViews"].append(
        {"buffer": 0, "byteOffset": offset, "byteLength": len(payload)}
    )
    accessor = {
        "bufferView": view_index,
        "componentType": 5126,
        "count": values.shape[0],
        "type": accessor_type,
    }
    if minimum is not None:
        accessor["min"] = minimum
    if maximum is not None:
        accessor["max"] = maximum
    index = len(document.setdefault("accessors", []))
    document["accessors"].append(accessor)
    return index


def _node_default(node: dict, path: str) -> np.ndarray:
    defaults = {
        "translation": (0.0, 0.0, 0.0),
        "rotation": (0.0, 0.0, 0.0, 1.0),
        "scale": (1.0, 1.0, 1.0),
    }
    try:
        return np.asarray(node.get(path, defaults[path]), dtype="<f4")
    except KeyError as error:
        raise GlbError(f"unsupported animation target path {path!r}") from error


def optimize_animation(
    document: dict,
    binary: bytes,
    *,
    kept_frames: tuple[int, ...] | None = None,
    remove_root_lateral_motion: bool = False,
) -> tuple[dict, bytes]:
    """Keep authored keys, remove bind-default tracks, and collapse constants.

    ``kept_frames`` names the 30 fps catalog frames that are authoritative for
    an ordinary motion. Locomotion passes ``None`` because its five cubic keys
    have already replaced Cascadeur's exported in-betweens.
    """
    optimized = copy.deepcopy(document)
    animation = optimized["animations"][0]
    source_animation = document["animations"][0]
    paths = scene_paths(document)
    packed = bytearray(binary)
    timestamp_accessors: dict[tuple[float, ...], int] = {}
    channels: list[dict] = []
    samplers: list[dict] = []
    def timestamps_accessor(times: np.ndarray) -> int:
        key = tuple(float(value) for value in times)
        existing = timestamp_accessors.get(key)
        if existing is not None:
            return existing
        index = append_float_accessor(
            optimized,
            packed,
            times.reshape(-1, 1),
            "SCALAR",
            minimum=[float(times[0])],
            maximum=[float(times[-1])],
        )
        timestamp_accessors[key] = index
        return index

    for channel in source_animation["channels"]:
        source_sampler = source_animation["samplers"][channel["sampler"]]
        interpolation = source_sampler.get("interpolation", "LINEAR")
        times = accessor_view(document, binary, source_sampler["input"])[:, 0]
        values = accessor_view(document, binary, source_sampler["output"])
        if interpolation == "CUBICSPLINE":
            if values.shape[0] != times.shape[0] * 3:
                raise GlbError("cubic animation output must contain tangent triples")
            key_values = values[1::3]
        elif interpolation in {"LINEAR", "STEP"}:
            if values.shape[0] != times.shape[0]:
                raise GlbError("animation input and output sample counts differ")
            key_values = values
        else:
            raise GlbError(f"unsupported animation interpolation {interpolation!r}")

        if kept_frames is not None:
            if interpolation == "CUBICSPLINE":
                raise GlbError("catalog frame trimming does not accept cubic source data")
            indices = []
            for frame in kept_frames:
                target_time = frame / ANIMATION_FPS
                matches = np.flatnonzero(np.isclose(times, target_time, atol=1e-5))
                if matches.size != 1:
                    raise GlbError(
                        f"animation does not expose one exact sample for frame {frame}"
                    )
                indices.append(int(matches[0]))
            times = times[indices]
            values = values[indices]
            key_values = values

        target = channel["target"]
        path = target["path"]
        if (
            remove_root_lateral_motion
            and paths.get(target["node"]) == ROOT_PATH
            and path == "translation"
        ):
            values = values.copy()
            values[:, 0] = 0.0
            values[:, 2] = 0.0
            key_values = values[1::3] if interpolation == "CUBICSPLINE" else values
        default = _node_default(optimized["nodes"][target["node"]], path)
        if np.allclose(key_values, default, rtol=0.0, atol=1e-6):
            continue

        constant = np.allclose(key_values, key_values[0], rtol=0.0, atol=1e-6)
        if constant:
            times = times[:1]
            values = key_values[:1]
            interpolation = "STEP"

        input_accessor = timestamps_accessor(times)
        output_accessor = append_float_accessor(
            optimized,
            packed,
            values,
            "VEC4" if path == "rotation" else "VEC3",
        )
        samplers.append(
            {
                "input": input_accessor,
                "output": output_accessor,
                "interpolation": interpolation,
            }
        )
        updated_channel = copy.deepcopy(channel)
        updated_channel["sampler"] = len(samplers) - 1
        channels.append(updated_channel)

    if not channels:
        raise GlbError("animation optimization removed every channel")
    animation["channels"] = channels
    animation["samplers"] = samplers
    optimized["buffers"][0]["byteLength"] = len(packed)
    return optimized, bytes(packed)


def strip_motion_mesh(document: dict, binary: bytes) -> tuple[dict, bytes]:
    """Return an animation-only GLB payload with compacted binary storage."""
    stripped = copy.deepcopy(document)
    animations = stripped.get("animations")
    if not isinstance(animations, list) or len(animations) != 1:
        raise GlbError("motion mesh stripping requires exactly one animation")

    kept_accessors: set[int] = set()
    for sampler in animations[0].get("samplers", []):
        for field in ("input", "output"):
            accessor = sampler.get(field)
            if not isinstance(accessor, int):
                raise GlbError(f"animation sampler {field} must reference an accessor")
            kept_accessors.add(accessor)

    source_accessors = stripped.get("accessors")
    source_views = stripped.get("bufferViews")
    if not isinstance(source_accessors, list) or not isinstance(source_views, list):
        raise GlbError("motion accessors and buffer views must be arrays")
    if any(index < 0 or index >= len(source_accessors) for index in kept_accessors):
        raise GlbError("animation sampler references an invalid accessor")

    kept_views: set[int] = set()
    for index in kept_accessors:
        accessor = source_accessors[index]
        view = accessor.get("bufferView")
        if not isinstance(view, int):
            raise GlbError("animation accessors must use dense buffer views")
        kept_views.add(view)
        sparse = accessor.get("sparse")
        if sparse is not None:
            raise GlbError("sparse animation accessors are not supported")
    if any(index < 0 or index >= len(source_views) for index in kept_views):
        raise GlbError("animation accessor references an invalid buffer view")

    view_map: dict[int, int] = {}
    compact_views: list[dict] = []
    compact_binary = bytearray()
    for old_index in sorted(kept_views):
        view = copy.deepcopy(source_views[old_index])
        if view.get("buffer", 0) != 0:
            raise GlbError("motion animation data must use the GLB binary buffer")
        offset = view.get("byteOffset", 0)
        length = view.get("byteLength")
        if (
            not isinstance(offset, int)
            or not isinstance(length, int)
            or offset < 0
            or length < 0
            or offset + length > len(binary)
        ):
            raise GlbError("animation buffer view exceeds the GLB binary chunk")
        compact_binary.extend(b"\0" * (-len(compact_binary) % 4))
        view["buffer"] = 0
        view["byteOffset"] = len(compact_binary)
        compact_binary.extend(binary[offset : offset + length])
        view_map[old_index] = len(compact_views)
        compact_views.append(view)

    accessor_map: dict[int, int] = {}
    compact_accessors: list[dict] = []
    for old_index in sorted(kept_accessors):
        accessor = copy.deepcopy(source_accessors[old_index])
        accessor["bufferView"] = view_map[accessor["bufferView"]]
        accessor_map[old_index] = len(compact_accessors)
        compact_accessors.append(accessor)
    for sampler in animations[0]["samplers"]:
        sampler["input"] = accessor_map[sampler["input"]]
        sampler["output"] = accessor_map[sampler["output"]]

    mesh_nodes = {
        index for index, node in enumerate(stripped.get("nodes", [])) if "mesh" in node
    }
    for node in stripped.get("nodes", []):
        node.pop("mesh", None)
        node.pop("skin", None)
    for scene in stripped.get("scenes", []):
        if isinstance(scene.get("nodes"), list):
            scene["nodes"] = [index for index in scene["nodes"] if index not in mesh_nodes]

    stripped["accessors"] = compact_accessors
    stripped["bufferViews"] = compact_views
    stripped["buffers"] = [{"byteLength": len(compact_binary)}]
    for field in ("meshes", "skins", "materials", "textures", "images", "samplers"):
        stripped.pop(field, None)
    return stripped, bytes(compact_binary)


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
    kept_frames: tuple[int, ...] | None = None,
    remove_root_lateral_motion: bool = False,
    check: bool = False,
) -> tuple[float, int]:
    base_document, _ = read_glb(base)
    motion_document, motion_binary = read_glb(source)
    result = validate_motion(base_document, motion_document, last_frame=last_frame)
    if kept_frames is None:
        kept_frames = tuple(range(last_frame + 1))
    optimized_document, optimized_binary = optimize_animation(
        motion_document,
        motion_binary,
        kept_frames=kept_frames,
        remove_root_lateral_motion=remove_root_lateral_motion,
    )
    stripped_document, stripped_binary = strip_motion_mesh(
        optimized_document, optimized_binary
    )
    prepared_bytes = encode_glb(stripped_document, stripped_binary)
    if check:
        if not destination.is_file() or destination.read_bytes() != prepared_bytes:
            raise GlbError(f"{destination} is not the prepared animation for {source}")
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.is_file() or destination.read_bytes() != prepared_bytes:
            destination.write_bytes(prepared_bytes)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("base", type=pathlib.Path)
    parser.add_argument("destination", type=pathlib.Path)
    parser.add_argument("--last-frame", type=int, required=True)
    parser.add_argument("--keep-frame", type=int, action="append", dest="kept_frames")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        duration, targets = prepare_motion(
            args.source,
            args.base,
            args.destination,
            last_frame=args.last_frame,
            kept_frames=(
                tuple(args.kept_frames) if args.kept_frames is not None else None
            ),
            check=args.check,
        )
    except (GlbError, OSError) as error:
        parser.error(str(error))
    action = "verified" if args.check else "prepared"
    print(f"{action} {args.destination} ({duration:.6f}s, {targets} animation targets)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
