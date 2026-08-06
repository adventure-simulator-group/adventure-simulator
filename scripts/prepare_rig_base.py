#!/usr/bin/env python3
"""Prepare and validate the authored humanoid base rig for runtime use.

The operation is intentionally dependency-free and deterministic: it preserves
the source binary chunk, canonicalizes the JSON chunk, and removes authoring-only
scene roots without attempting to rewrite mesh or skin indices.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import struct
import sys


JSON_CHUNK = 0x4E4F534A
BIN_CHUNK = 0x004E4942
EXPECTED_JOINT_COUNT = 74
REQUIRED_JOINTS = {
    "root", "pelvis", "stomach_01", "stomach_02", "chest",
    "neck_01", "neck_02", "head", "clavicle.L", "clavicle.R",
    "upper_arm.L", "upper_arm.R", "upper_arm_twist.L", "upper_arm_twist.R",
    "forearm.L", "forearm.R", "forearm_twist.L", "forearm_twist.R",
    "hand.L", "hand.R", "weapon.L", "weapon.R", "thigh.L", "thigh.R",
    "thigh_twist.L", "thigh_twist.R", "shin.L", "shin.R",
    "shin_twist.L", "shin_twist.R", "foot.L", "foot.R", "toe.L", "toe.R",
}
AUTHORING_ROOTS = {"weapon", "Cylinder"}
EXPECTED_PARENTS = {
    "pelvis": "root", "stomach_01": "pelvis", "stomach_02": "stomach_01",
    "chest": "stomach_02", "neck_01": "chest", "neck_02": "neck_01", "head": "neck_02",
    "clavicle.L": "chest", "upper_arm.L": "clavicle.L", "upper_arm_twist.L": "upper_arm.L",
    "forearm.L": "upper_arm_twist.L", "forearm_twist.L": "forearm.L", "hand.L": "forearm_twist.L", "weapon.L": "hand.L",
    "clavicle.R": "chest", "upper_arm.R": "clavicle.R", "upper_arm_twist.R": "upper_arm.R",
    "forearm.R": "upper_arm_twist.R", "forearm_twist.R": "forearm.R", "hand.R": "forearm_twist.R", "weapon.R": "hand.R",
    "thigh.L": "pelvis", "thigh_twist.L": "thigh.L", "shin.L": "thigh_twist.L", "shin_twist.L": "shin.L", "foot.L": "shin_twist.L", "toe.L": "foot.L",
    "thigh.R": "pelvis", "thigh_twist.R": "thigh.R", "shin.R": "thigh_twist.R", "shin_twist.R": "shin.R", "foot.R": "shin_twist.R", "toe.R": "foot.R",
}


class GlbError(ValueError):
    pass


def read_glb(path: pathlib.Path) -> tuple[dict, bytes]:
    data = path.read_bytes()
    if len(data) < 20:
        raise GlbError("file is too short to be a GLB")
    magic, version, declared_length = struct.unpack_from("<4sII", data)
    if magic != b"glTF" or version != 2 or declared_length != len(data):
        raise GlbError("expected a complete glTF 2.0 binary file")
    chunks: list[tuple[int, bytes]] = []
    offset = 12
    while offset < len(data):
        if offset + 8 > len(data):
            raise GlbError("truncated chunk header")
        length, kind = struct.unpack_from("<II", data, offset)
        offset += 8
        end = offset + length
        if end > len(data):
            raise GlbError("truncated chunk data")
        chunks.append((kind, data[offset:end]))
        offset = end
    if not chunks or chunks[0][0] != JSON_CHUNK:
        raise GlbError("first GLB chunk must be JSON")
    binary = next((chunk for kind, chunk in chunks[1:] if kind == BIN_CHUNK), b"")
    try:
        document = json.loads(chunks[0][1].decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise GlbError(f"invalid JSON chunk: {error}") from error
    return document, binary


def validate_and_prepare(document: dict) -> dict:
    if not isinstance(document, dict):
        raise GlbError("top-level JSON must be an object")
    nodes = document.get("nodes")
    skins = document.get("skins")
    scenes = document.get("scenes")
    if not isinstance(nodes, list) or not isinstance(skins, list) or not isinstance(scenes, list):
        raise GlbError("nodes, skins, and scenes must be arrays")
    if not all(isinstance(node, dict) for node in nodes):
        raise GlbError("every node must be an object")
    if not all(isinstance(skin, dict) for skin in skins):
        raise GlbError("every skin must be an object")
    if not all(isinstance(scene, dict) for scene in scenes):
        raise GlbError("every scene must be an object")
    if "extras" in document and not isinstance(document["extras"], dict):
        raise GlbError("top-level extras must be an object")
    if len(skins) != 1:
        raise GlbError(f"expected exactly one skin, found {len(skins)}")
    joints = skins[0].get("joints")
    if not isinstance(joints, list) or len(joints) != EXPECTED_JOINT_COUNT:
        raise GlbError(f"expected {EXPECTED_JOINT_COUNT} skin joints")
    if any(type(i) is not int or i < 0 or i >= len(nodes) for i in joints):
        raise GlbError("skin joints must be unique valid node indices")
    if len(set(joints)) != len(joints):
        raise GlbError("skin joints must be unique valid node indices")
    names = [nodes[i].get("name") for i in joints]
    if not all(isinstance(name, str) for name in names) or len(set(names)) != len(names):
        raise GlbError("every skin joint must have a unique name")
    missing = sorted(REQUIRED_JOINTS.difference(names))
    if missing:
        raise GlbError("missing required joints: " + ", ".join(missing))
    parent_indices: dict[int, int] = {}
    for parent_index, node in enumerate(nodes):
        children = node.get("children", [])
        if not isinstance(children, list) or any(
            type(child) is not int or child < 0 or child >= len(nodes) for child in children
        ):
            raise GlbError("node children must be arrays of valid node indices")
        for child in children:
            if child in parent_indices:
                raise GlbError("a runtime joint cannot have multiple parents")
            parent_indices[child] = parent_index
    indices_by_name = {nodes[index]["name"]: index for index in joints}
    for child_name, parent_name in EXPECTED_PARENTS.items():
        child = indices_by_name[child_name]
        actual_parent = parent_indices.get(child)
        expected_parent = indices_by_name[parent_name]
        if actual_parent != expected_parent:
            actual_name = nodes[actual_parent].get("name") if actual_parent is not None else None
            raise GlbError(f"expected {child_name} parent {parent_name}, found {actual_name}")
    if document.get("animations", []) not in ([], None):
        # Clips are allowed as poses arrive; this guard is deliberately only a type check.
        if not isinstance(document["animations"], list):
            raise GlbError("animations must be an array")

    default_scene = document.get("scene", 0)
    if not isinstance(default_scene, int) or not 0 <= default_scene < len(scenes):
        raise GlbError("default scene index is invalid")
    roots = scenes[default_scene].get("nodes")
    if not isinstance(roots, list):
        raise GlbError("default scene must contain root nodes")
    if any(not isinstance(i, int) or i < 0 or i >= len(nodes) for i in roots):
        raise GlbError("default scene roots must be valid node indices")
    kept = [i for i in roots if nodes[i].get("name") not in AUTHORING_ROOTS]
    removed = [nodes[i].get("name") for i in roots if i not in kept]
    if not removed:
        raise GlbError("expected an authoring-only weapon/Cylinder scene root")
    if not any(nodes[i].get("skin") == 0 for i in kept):
        raise GlbError("runtime scene would not contain the skinned human mesh")
    scenes[default_scene]["nodes"] = kept
    document.setdefault("extras", {})["adventuresim_runtime_rig"] = {
        "source": "assets_src/base.glb",
        "excluded_scene_roots": removed,
    }
    return document


def encode_glb(document: dict, binary: bytes) -> bytes:
    json_bytes = json.dumps(document, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    json_bytes += b" " * (-len(json_bytes) % 4)
    binary += b"\0" * (-len(binary) % 4)
    chunks = struct.pack("<II", len(json_bytes), JSON_CHUNK) + json_bytes
    if binary:
        chunks += struct.pack("<II", len(binary), BIN_CHUNK) + binary
    return struct.pack("<4sII", b"glTF", 2, 12 + len(chunks)) + chunks


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        document, binary = read_glb(args.source)
        prepared = encode_glb(validate_and_prepare(document), binary)
    except (OSError, GlbError) as error:
        print(f"base rig preparation failed: {error}", file=sys.stderr)
        return 1
    if args.check:
        try:
            existing = args.output.read_bytes()
        except OSError as error:
            print(f"runtime pack is unavailable: {error}", file=sys.stderr)
            return 1
        if existing != prepared:
            print(f"runtime pack is stale: {args.output}", file=sys.stderr)
            return 1
        print(f"validated {args.output}")
        return 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(prepared)
    print(f"prepared {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
