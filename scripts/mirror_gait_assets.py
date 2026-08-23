"""Bake character-space mirrored gait clips from canonical GLB motions.

The runtime must blend complete unmirrored and mirrored anchor poses. A
fractional reflection of an already blended skeleton collapses bilateral
forward/back separation, so gait parity is baked into distinct clips instead.

Requires Python 3 and NumPy. Run from the repository root:

    python scripts/mirror_gait_assets.py
    python scripts/mirror_gait_assets.py --check
"""

from __future__ import annotations

import argparse
import json
import struct
from pathlib import Path

import numpy as np

from prepare_animation_motion import strip_motion_mesh


ROOT = Path(__file__).resolve().parents[1]
ASSET_DIR = ROOT / "assets" / "animations" / "biped" / "unarmed"
MIRRORED_MOTIONS = {
    "walk": "walk_mirrored",
    "run": "run_mirrored",
    "prone_crawl": "prone_crawl_mirrored",
    "supine_scamper": "supine_scamper_mirrored",
    "prone_supine_roll_left": "prone_supine_roll_right",
}
REFLECTION = np.diag((-1.0, 1.0, 1.0, 1.0))


def read_glb(path: Path) -> tuple[dict, bytearray]:
    data = path.read_bytes()
    magic, version, length = struct.unpack_from("<4sII", data, 0)
    if magic != b"glTF" or version != 2 or length != len(data):
        raise ValueError(f"{path} is not a valid GLB 2 file")
    offset = 12
    chunks: list[tuple[int, bytes]] = []
    while offset < length:
        size, kind = struct.unpack_from("<II", data, offset)
        chunks.append((kind, data[offset + 8 : offset + 8 + size]))
        offset += 8 + size
    if len(chunks) != 2 or chunks[0][0] != 0x4E4F534A or chunks[1][0] != 0x004E4942:
        raise ValueError(f"{path} must contain one JSON and one BIN chunk")
    return json.loads(chunks[0][1].rstrip(b"\x00 ")), bytearray(chunks[1][1])


def encode_glb(document: dict, binary: bytearray) -> bytes:
    encoded = json.dumps(document, separators=(",", ":")).encode("utf-8")
    encoded += b" " * (-len(encoded) % 4)
    binary += b"\x00" * (-len(binary) % 4)
    length = 12 + 8 + len(encoded) + 8 + len(binary)
    return (
        struct.pack("<4sII", b"glTF", 2, length)
        + struct.pack("<II", len(encoded), 0x4E4F534A)
        + encoded
        + struct.pack("<II", len(binary), 0x004E4942)
        + binary
    )


def accessor_view(document: dict, binary: bytearray, index: int) -> np.ndarray:
    accessor = document["accessors"][index]
    if accessor.get("componentType") != 5126 or "sparse" in accessor:
        raise ValueError("gait animation accessors must be dense FLOAT data")
    width = {"SCALAR": 1, "VEC3": 3, "VEC4": 4}[accessor["type"]]
    view = document["bufferViews"][accessor["bufferView"]]
    stride = view.get("byteStride", width * 4)
    if stride != width * 4:
        raise ValueError("interleaved animation accessors are not supported")
    offset = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    return np.ndarray(
        (accessor["count"], width), dtype="<f4", buffer=binary, offset=offset
    )


def quaternion_matrix(value: np.ndarray) -> np.ndarray:
    x, y, z, w = (float(component) for component in value)
    norm = x * x + y * y + z * z + w * w
    if norm <= 1e-12:
        return np.identity(3)
    scale = 2.0 / norm
    xx, yy, zz = x * x * scale, y * y * scale, z * z * scale
    xy, xz, yz = x * y * scale, x * z * scale, y * z * scale
    wx, wy, wz = w * x * scale, w * y * scale, w * z * scale
    return np.array(
        (
            (1.0 - yy - zz, xy - wz, xz + wy),
            (xy + wz, 1.0 - xx - zz, yz - wx),
            (xz - wy, yz + wx, 1.0 - xx - yy),
        )
    )


def compose(translation: np.ndarray, rotation: np.ndarray, scale: np.ndarray) -> np.ndarray:
    matrix = np.identity(4)
    matrix[:3, :3] = quaternion_matrix(rotation) @ np.diag(scale)
    matrix[:3, 3] = translation
    return matrix


def matrix_quaternion(matrix: np.ndarray) -> np.ndarray:
    trace = float(np.trace(matrix))
    if trace > 0.0:
        root = np.sqrt(trace + 1.0) * 2.0
        value = np.array(
            ((matrix[2, 1] - matrix[1, 2]) / root,
             (matrix[0, 2] - matrix[2, 0]) / root,
             (matrix[1, 0] - matrix[0, 1]) / root,
             0.25 * root)
        )
    else:
        axis = int(np.argmax(np.diag(matrix)))
        if axis == 0:
            root = np.sqrt(1.0 + matrix[0, 0] - matrix[1, 1] - matrix[2, 2]) * 2.0
            value = np.array((0.25 * root, (matrix[0, 1] + matrix[1, 0]) / root,
                              (matrix[0, 2] + matrix[2, 0]) / root,
                              (matrix[2, 1] - matrix[1, 2]) / root))
        elif axis == 1:
            root = np.sqrt(1.0 + matrix[1, 1] - matrix[0, 0] - matrix[2, 2]) * 2.0
            value = np.array(((matrix[0, 1] + matrix[1, 0]) / root, 0.25 * root,
                              (matrix[1, 2] + matrix[2, 1]) / root,
                              (matrix[0, 2] - matrix[2, 0]) / root))
        else:
            root = np.sqrt(1.0 + matrix[2, 2] - matrix[0, 0] - matrix[1, 1]) * 2.0
            value = np.array(((matrix[0, 2] + matrix[2, 0]) / root,
                              (matrix[1, 2] + matrix[2, 1]) / root, 0.25 * root,
                              (matrix[1, 0] - matrix[0, 1]) / root))
    return value / np.linalg.norm(value)


def decompose(matrix: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    translation = matrix[:3, 3].copy()
    columns = matrix[:3, :3]
    scale = np.linalg.norm(columns, axis=0)
    rotation = columns / scale
    if np.linalg.det(rotation) < 0.0:
        scale[0] = -scale[0]
        rotation[:, 0] = -rotation[:, 0]
    return translation, matrix_quaternion(rotation), scale


def resolve_global_matrices(
    local_matrices: list[np.ndarray], parents: dict[int, int]
) -> list[np.ndarray]:
    """Resolve node globals independently of the GLB node-table order."""
    resolved: list[np.ndarray | None] = [None] * len(local_matrices)
    visiting: set[int] = set()

    def resolve(index: int) -> np.ndarray:
        if index < 0 or index >= len(local_matrices):
            raise ValueError(f"node hierarchy references missing node {index}")
        cached = resolved[index]
        if cached is not None:
            return cached
        if index in visiting:
            raise ValueError(f"node hierarchy contains a cycle at node {index}")
        visiting.add(index)
        parent = parents.get(index)
        matrix = local_matrices[index]
        global_matrix = matrix if parent is None else resolve(parent) @ matrix
        visiting.remove(index)
        resolved[index] = global_matrix
        return global_matrix

    return [resolve(index) for index in range(len(local_matrices))]


def mhr_bilateral_pairs(by_name: dict[object, int]) -> list[tuple[int, int]]:
    """Return every complete canonical MHR ``l_*``/``r_*`` node pair."""
    return [
        (left, by_name[f"r_{name[2:]}"])
        for name, left in by_name.items()
        if isinstance(name, str)
        and name.startswith("l_")
        and f"r_{name[2:]}" in by_name
    ]


def mirrored_pose_globals(
    animated_globals: list[np.ndarray],
    bind_globals: list[np.ndarray],
    inverse_bind_globals: list[np.ndarray],
    counterpart: dict[int, int],
) -> list[np.ndarray]:
    """Mirror skinning deformations while preserving target bind frames."""
    return [
        REFLECTION
        @ animated_globals[counterpart[index]]
        @ inverse_bind_globals[counterpart[index]]
        @ REFLECTION
        @ bind_globals[index]
        for index in range(len(animated_globals))
    ]


def mirrored_glb(source: Path) -> bytes:
    document, binary = read_glb(source)
    nodes = document["nodes"]
    by_name = {node.get("name"): index for index, node in enumerate(nodes)}
    parents: dict[int, int] = {}
    for parent, node in enumerate(nodes):
        for child in node.get("children", ()):
            parents[child] = parent
    # Discover the complete canonical MHR bilateral hierarchy so palms,
    # fingers, distributed twists, feet, and any future authored descendants
    # follow their exchanged hand/limb parents.
    pairs = mhr_bilateral_pairs(by_name)
    counterpart = {index: index for index in range(len(nodes))}
    for left, right in pairs:
        counterpart[left] = right
        counterpart[right] = left

    bind_local = [
        compose(
            np.array(node.get("translation", (0, 0, 0))),
            np.array(node.get("rotation", (0, 0, 0, 1))),
            np.array(node.get("scale", (1, 1, 1))),
        )
        for node in nodes
    ]
    bind_globals = resolve_global_matrices(bind_local, parents)
    inverse_bind_globals = [np.linalg.inv(matrix) for matrix in bind_globals]

    channels: dict[tuple[int, str], np.ndarray] = {}
    animation = document["animations"][0]
    for channel in animation["channels"]:
        target = channel["target"]
        sampler = animation["samplers"][channel["sampler"]]
        if sampler.get("interpolation", "LINEAR") != "LINEAR":
            raise ValueError("gait mirror baking requires linear animation samplers")
        channels[(target["node"], target["path"])] = accessor_view(
            document, binary, sampler["output"]
        )
    counts = {values.shape[0] for values in channels.values()}
    if len(counts) != 1:
        raise ValueError("gait channels must share a common sample count")

    source_values = {key: values.copy() for key, values in channels.items()}
    for frame in range(counts.pop()):
        local: list[np.ndarray] = []
        for index, node in enumerate(nodes):
            translation = source_values.get((index, "translation"))
            rotation = source_values.get((index, "rotation"))
            scale = source_values.get((index, "scale"))
            local.append(
                compose(
                    translation[frame] if translation is not None else np.array(node.get("translation", (0, 0, 0))),
                    rotation[frame] if rotation is not None else np.array(node.get("rotation", (0, 0, 0, 1))),
                    scale[frame] if scale is not None else np.array(node.get("scale", (1, 1, 1))),
                )
            )
        global_matrices = resolve_global_matrices(local, parents)
        # Mirror each source bone's deformation relative to its bind frame,
        # then apply that deformation to the counterpart's bind frame. MHR
        # joints have deliberately rolled local axes, so reflecting absolute
        # globals would also reflect the bind skeleton and invert center bones.
        desired = mirrored_pose_globals(
            global_matrices,
            bind_globals,
            inverse_bind_globals,
            counterpart,
        )
        animated_nodes = {node for node, _path in channels}
        for target in animated_nodes:
            parent = parents.get(target)
            local_matrix = (
                desired[target]
                if parent is None
                else np.linalg.inv(desired[parent]) @ desired[target]
            )
            translation, rotation, scale = decompose(local_matrix)
            # q and -q encode the same orientation, but Bevy blends active
            # sibling clips component-wise before normalization. Keep each
            # mirrored key in the same quaternion hemisphere as the canonical
            # target bone so a contact-to-mirrored-contact blend cannot pass
            # through a near-zero quaternion and tear the skinned hierarchy.
            source_rotation = source_values.get((target, "rotation"))
            if source_rotation is not None and np.dot(rotation, source_rotation[frame]) < 0.0:
                rotation = -rotation
            if (target, "translation") in channels:
                channels[(target, "translation")][frame] = translation
            if (target, "rotation") in channels:
                channels[(target, "rotation")][frame] = rotation
            if (target, "scale") in channels:
                channels[(target, "scale")][frame] = scale

    stripped_document, stripped_binary = strip_motion_mesh(document, binary)
    return encode_glb(stripped_document, bytearray(stripped_binary))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    stale: list[Path] = []
    skipped: list[Path] = []
    for motion, output_motion in MIRRORED_MOTIONS.items():
        source = ASSET_DIR / f"{motion}.glb"
        output = ASSET_DIR / f"{output_motion}.glb"
        if not source.is_file():
            skipped.append(source)
            continue
        generated = mirrored_glb(source)
        if args.check:
            if not output.exists() or output.read_bytes() != generated:
                stale.append(output)
        else:
            output.write_bytes(generated)
            print(output.relative_to(ROOT))
    if skipped:
        names = ", ".join(str(path.relative_to(ROOT)) for path in skipped)
        print(f"skipped missing mirror sources: {names}")
    if stale:
        names = ", ".join(str(path.relative_to(ROOT)) for path in stale)
        raise SystemExit(f"stale mirrored gait assets: {names}")


if __name__ == "__main__":
    main()
