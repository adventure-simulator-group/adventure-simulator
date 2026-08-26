from __future__ import annotations

import importlib.util
import pathlib
import sys
import tempfile
import unittest

import numpy as np


ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "build_locomotion_cycles", ROOT / "scripts" / "build_locomotion_cycles.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class LocomotionCycleTests(unittest.TestCase):
    @staticmethod
    def animation_values_at_frame(
        document: dict, binary: bytearray, frame: int
    ) -> dict[tuple[int, str], np.ndarray]:
        animation = document["animations"][0]
        timestamp = frame / 30.0
        values = {}
        for channel in animation["channels"]:
            sampler = animation["samplers"][channel["sampler"]]
            times = MODULE.accessor_view(document, binary, sampler["input"])[:, 0]
            if times.size == 1:
                index = 0
            else:
                matches = np.flatnonzero(np.isclose(times, timestamp, atol=1e-5))
                if matches.size != 1:
                    raise AssertionError(f"frame {frame} is not an exact animation key")
                index = int(matches[0])
            output = MODULE.accessor_view(document, binary, sampler["output"])
            if sampler.get("interpolation", "LINEAR") == "CUBICSPLINE":
                index = index * 3 + 1
            target = channel["target"]
            values[(target["node"], target["path"])] = output[index]
        return values

    @classmethod
    def animation_globals(
        cls, document: dict, binary: bytearray, frame: int
    ) -> list[np.ndarray]:
        values = cls.animation_values_at_frame(document, binary, frame)
        parents = {
            child: parent
            for parent, node in enumerate(document["nodes"])
            for child in node.get("children", ())
        }
        local = []
        for index, node in enumerate(document["nodes"]):
            translation = values.get((index, "translation"))
            rotation = values.get((index, "rotation"))
            scale = values.get((index, "scale"))
            local.append(
                MODULE.compose(
                    translation
                    if translation is not None
                    else np.array(node.get("translation", (0, 0, 0))),
                    rotation
                    if rotation is not None
                    else np.array(node.get("rotation", (0, 0, 0, 1))),
                    scale
                    if scale is not None
                    else np.array(node.get("scale", (1, 1, 1))),
                )
            )
        return MODULE.resolve_global_matrices(local, parents)

    def test_global_matrices_support_parents_after_children_in_node_table(self) -> None:
        child = np.identity(4)
        child[0, 3] = 2.0
        parent = np.identity(4)
        parent[1, 3] = 3.0

        resolved = MODULE.resolve_global_matrices([child, parent], {0: 1})

        np.testing.assert_allclose(resolved[0][:3, 3], (2.0, 3.0, 0.0))
        np.testing.assert_allclose(resolved[1][:3, 3], (0.0, 3.0, 0.0))

    def test_mhr_bilateral_pairs_include_twists_fingers_feet_and_weapons(self) -> None:
        names = {
            "l_uparm_twist3_proc": 1,
            "r_uparm_twist3_proc": 2,
            "l_index2": 3,
            "r_index2": 4,
            "l_subtalar": 5,
            "r_subtalar": 6,
            "l_weapon": 7,
            "r_weapon": 8,
            "c_spine2": 9,
        }

        self.assertEqual(
            set(MODULE.mhr_bilateral_pairs(names)),
            {(1, 2), (3, 4), (5, 6), (7, 8)},
        )

    def test_bind_relative_mirroring_preserves_rolled_mhr_bind_frame(self) -> None:
        quarter_turn = np.sin(np.pi / 4.0)
        bind = MODULE.compose(
            np.array((0.0, 1.0, 0.0)),
            np.array((quarter_turn, 0.0, 0.0, quarter_turn)),
            np.ones(3),
        )
        deformation = MODULE.compose(
            np.zeros(3),
            np.array((0.0, 0.0, np.sin(0.15), np.cos(0.15))),
            np.ones(3),
        )
        animated = deformation @ bind

        mirrored = MODULE.mirrored_pose_globals(
            [animated],
            [bind],
            [np.linalg.inv(bind)],
            {0: 0},
        )[0]
        expected = MODULE.REFLECTION @ deformation @ MODULE.REFLECTION @ bind

        np.testing.assert_allclose(mirrored, expected, atol=1e-7)
        np.testing.assert_allclose(
            MODULE.mirrored_pose_globals(
                [bind],
                [bind],
                [np.linalg.inv(bind)],
                {0: 0},
            )[0],
            bind,
            atol=1e-7,
        )

    def test_generated_opposite_contact_mirrors_skin_deformation_not_mhr_bind(self) -> None:
        source_document, source_binary = MODULE.read_glb(
            MODULE.SOURCE_DIR / "walk.glb"
        )
        generated = MODULE.build_cycle(MODULE.SOURCE_DIR / "walk.glb")
        with tempfile.TemporaryDirectory() as temporary:
            generated_document, generated_binary = MODULE.decode_glb_bytes(
                generated, pathlib.Path(temporary) / "walk.glb"
            )

        nodes = source_document["nodes"]
        names = {node.get("name"): index for index, node in enumerate(nodes)}
        parents = {
            child: parent
            for parent, node in enumerate(nodes)
            for child in node.get("children", ())
        }
        bind_local = [
            MODULE.compose(
                np.array(node.get("translation", (0, 0, 0))),
                np.array(node.get("rotation", (0, 0, 0, 1))),
                np.array(node.get("scale", (1, 1, 1))),
            )
            for node in nodes
        ]
        bind_globals = MODULE.resolve_global_matrices(bind_local, parents)
        inverse_bind = [np.linalg.inv(matrix) for matrix in bind_globals]
        counterpart = {index: index for index in range(len(nodes))}
        for left, right in MODULE.mhr_bilateral_pairs(names):
            counterpart[left] = right
            counterpart[right] = left

        source_contact = self.animation_globals(source_document, source_binary, 0)
        opposite_contact = self.animation_globals(
            generated_document, generated_binary, 32
        )
        expected = MODULE.mirrored_pose_globals(
            source_contact,
            bind_globals,
            inverse_bind,
            counterpart,
        )
        for index, node in enumerate(nodes):
            if node.get("name") in {"Skeleton", "mesh"}:
                continue
            np.testing.assert_allclose(
                opposite_contact[index],
                expected[index],
                atol=2e-5,
                err_msg=f"mirrored node {node.get('name', index)}",
            )

        spine = names["c_spine2"]
        old_absolute_reflection = (
            MODULE.REFLECTION @ source_contact[spine] @ MODULE.REFLECTION
        )
        self.assertGreater(
            np.max(np.abs(old_absolute_reflection - expected[spine])),
            0.1,
            "the fixture must distinguish the old inverted-bind behavior",
        )

    def test_committed_runtime_cycles_match_authored_semantic_poses(self) -> None:
        for motion in MODULE.MOTIONS:
            generated = MODULE.build_cycle(MODULE.SOURCE_DIR / f"{motion}.glb")
            committed = MODULE.ASSET_DIR / f"{motion}.glb"
            self.assertEqual(committed.read_bytes(), generated, motion)
            self.assertLess(len(generated), 80_000, motion)

    def test_runtime_cycles_store_five_cubic_keys_not_exported_in_betweens(self) -> None:
        generated = MODULE.build_cycle(MODULE.SOURCE_DIR / "walk.glb")
        with tempfile.TemporaryDirectory() as temporary:
            document, binary = MODULE.decode_glb_bytes(
                generated, pathlib.Path(temporary) / "walk.glb"
            )
        animation = document["animations"][0]
        for sampler in animation["samplers"]:
            input_count = document["accessors"][sampler["input"]]["count"]
            output_count = document["accessors"][sampler["output"]]["count"]
            if sampler["interpolation"] == "CUBICSPLINE":
                self.assertEqual(input_count, 5)
                self.assertEqual(output_count, 15)
            else:
                self.assertEqual(input_count, 1)
                self.assertEqual(output_count, 1)

    def test_cubic_quaternion_keys_closely_match_the_previous_slerp_curve(self) -> None:
        for motion in MODULE.MOTIONS:
            source = MODULE.SOURCE_DIR / f"{motion}.glb"
            document, binary = MODULE.read_glb(source)
            with tempfile.TemporaryDirectory() as temporary:
                mirrored_document, mirrored_binary = MODULE.decode_glb_bytes(
                    MODULE.mirrored_glb(source),
                    pathlib.Path(temporary) / f"{motion}.glb",
                )
            canonical = MODULE.channel_values(document, binary)
            mirrored = MODULE.channel_values(mirrored_document, mirrored_binary)
            passing = next(iter(canonical.values())).shape[0] - 1
            maximum_error = 0.0
            for key, values in canonical.items():
                if key[1] != "rotation":
                    continue
                poses = (
                    values[0],
                    values[passing],
                    mirrored[key][0],
                    mirrored[key][passing],
                    values[0],
                )
                for left, right in zip(poses, poses[1:]):
                    if np.dot(left, right) < 0.0:
                        right = -right
                    for step in range(17):
                        factor = MODULE.smoothstep(step / 16.0)
                        cubic = left * (1.0 - factor) + right * factor
                        cubic /= np.linalg.norm(cubic)
                        expected = MODULE.slerp(left, right, factor)
                        dot = np.clip(abs(float(np.dot(cubic, expected))), 0.0, 1.0)
                        maximum_error = max(
                            maximum_error, float(np.degrees(2.0 * np.arccos(dot)))
                        )
            self.assertLess(maximum_error, 1.5, f"{motion}: {maximum_error} degrees")

    def test_runtime_quarter_cycle_preserves_the_exact_authored_pose(self) -> None:
        for motion in MODULE.MOTIONS:
            source_document, source_binary = MODULE.read_glb(
                MODULE.SOURCE_DIR / f"{motion}.glb"
            )
            source_values = MODULE.channel_values(source_document, source_binary)
            passing_frame = next(iter(source_values.values())).shape[0] - 1
            generated = MODULE.build_cycle(MODULE.SOURCE_DIR / f"{motion}.glb")
            with tempfile.TemporaryDirectory() as temporary:
                generated_document, generated_binary = MODULE.decode_glb_bytes(
                    generated, pathlib.Path(temporary) / f"{motion}.glb"
                )
            generated_at_frame = self.animation_values_at_frame(
                generated_document, generated_binary, 16
            )
            for key in source_values:
                node, path = key
                expected = generated_document["nodes"][node].get(
                    path,
                    {
                        "translation": (0, 0, 0),
                        "rotation": (0, 0, 0, 1),
                        "scale": (1, 1, 1),
                    }[path],
                )
                actual = np.asarray(generated_at_frame.get(key, expected))
                desired = source_values[key][passing_frame]
                if path == "rotation" and np.dot(actual, desired) < 0.0:
                    actual = -actual
                np.testing.assert_allclose(
                    actual,
                    desired,
                    atol=1e-6,
                    err_msg=f"{motion} channel {key}",
                )


if __name__ == "__main__":
    unittest.main()
