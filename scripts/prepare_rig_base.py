#!/usr/bin/env python3
"""Prepare and validate the authored humanoid base rig for runtime use.

The operation is intentionally dependency-free and deterministic: it validates
the complete MHR hierarchy and skin contract, preserves the source binary
chunk, and canonicalizes the JSON chunk without rewriting mesh or skin indices.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import struct
import sys


JSON_CHUNK = 0x4E4F534A
BIN_CHUNK = 0x004E4942
EXPECTED_JOINT_COUNT = 130
EXPECTED_RIG_SIGNATURE = "1488314aceabd8ec688dedb0a283a4cd3b03be840bc402238851e679fb8bd0d4"
REQUIRED_JOINTS = {
    "body_world", "root", "c_spine0", "c_spine1", "c_spine2", "c_spine3",
    "c_neck", "c_head", "l_clavicle", "r_clavicle", "l_uparm", "r_uparm",
    "l_lowarm", "r_lowarm", "l_wrist", "r_wrist", "l_upleg", "r_upleg",
    "l_lowleg", "r_lowleg", "l_foot", "r_foot", "l_ball", "r_ball",
    "l_eye", "r_eye", "c_jaw", "l_weapon", "r_weapon", "c_camera",
}
EXPECTED_PARENTS = {
    "root": "body_world", "c_spine0": "root", "c_spine1": "c_spine0",
    "c_spine2": "c_spine1", "c_spine3": "c_spine2", "c_neck": "c_spine3",
    "c_head": "c_neck", "l_clavicle": "c_spine3", "r_clavicle": "c_spine3",
    "l_uparm": "l_clavicle", "r_uparm": "r_clavicle",
    "l_lowarm": "l_uparm", "r_lowarm": "r_uparm",
    "l_wrist": "l_wrist_twist", "r_wrist": "r_wrist_twist",
    "l_upleg": "root", "r_upleg": "root",
    "l_lowleg": "l_upleg", "r_lowleg": "r_upleg",
    "l_foot": "l_lowleg", "r_foot": "r_lowleg",
    "l_ball": "l_transversetarsal", "r_ball": "r_transversetarsal",
    "l_weapon": "l_wrist", "r_weapon": "r_wrist", "c_camera": "c_head",
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
    signature_lines = []
    for joint in joints:
        parent = parent_indices.get(joint)
        parent_name = nodes[parent].get("name", "") if parent in joints else ""
        signature_lines.append(f"{nodes[joint]['name']}\0{parent_name}")
    signature = hashlib.sha256("\n".join(signature_lines).encode()).hexdigest()
    if signature != EXPECTED_RIG_SIGNATURE:
        raise GlbError(f"MHR joint order or hierarchy has changed ({signature})")
    if document.get("animations", []) not in ([], None):
        raise GlbError("the spawnable base rig must contain zero animations")

    extras = document.get("extras", {})
    rig = extras.get("adventuresim_rig")
    if not isinstance(rig, dict) or rig.get("family") != "mhr":
        raise GlbError("base rig must declare the MHR rig family")
    expected_attachments = [
        {"name": "l_weapon", "parent": "l_wrist", "role": "left_weapon_grip"},
        {"name": "r_weapon", "parent": "r_wrist", "role": "right_weapon_grip"},
        {"name": "c_camera", "parent": "c_head", "role": "first_person_camera"},
    ]
    if rig.get("attachments") != expected_attachments:
        raise GlbError("base rig does not declare the canonical MHR attachments")
    meshes = document.get("meshes")
    if not isinstance(meshes, list) or len(meshes) != 1:
        raise GlbError("expected exactly one MHR character mesh")
    primitives = meshes[0].get("primitives") if isinstance(meshes[0], dict) else None
    if not isinstance(primitives, list) or len(primitives) != 1:
        raise GlbError("expected exactly one MHR mesh primitive")
    attributes = primitives[0].get("attributes") if isinstance(primitives[0], dict) else None
    required_attributes = {"POSITION", "NORMAL", "JOINTS_0", "WEIGHTS_0", "JOINTS_1", "WEIGHTS_1"}
    if not isinstance(attributes, dict) or not required_attributes.issubset(attributes):
        raise GlbError("MHR mesh is missing required geometry or skinning attributes")
    if not isinstance(skins[0].get("inverseBindMatrices"), int):
        raise GlbError("MHR skin is missing inverse bind matrices")

    default_scene = document.get("scene", 0)
    if not isinstance(default_scene, int) or not 0 <= default_scene < len(scenes):
        raise GlbError("default scene index is invalid")
    roots = scenes[default_scene].get("nodes")
    if not isinstance(roots, list):
        raise GlbError("default scene must contain root nodes")
    if any(not isinstance(i, int) or i < 0 or i >= len(nodes) for i in roots):
        raise GlbError("default scene roots must be valid node indices")
    if sum(nodes[i].get("name") == "Skeleton" for i in roots) != 1:
        raise GlbError("default scene must contain one Skeleton hierarchy root")
    if not any(nodes[i].get("skin") == 0 for i in roots):
        raise GlbError("default scene must contain the skinned MHR mesh")
    document.setdefault("extras", {})["adventuresim_runtime_rig"] = {
        "source": "assets_src/biped/unarmed/base.glb",
        "rig_signature": EXPECTED_RIG_SIGNATURE,
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
