import importlib.util
import pathlib
import sys
import tempfile
import unittest

import numpy as np


PATH = pathlib.Path(__file__).parents[1] / "prepare_animation_motion.py"
sys.path.insert(0, str(PATH.parent))
SPEC = importlib.util.spec_from_file_location("prepare_animation_motion", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(MODULE)


class PrepareAnimationMotionTests(unittest.TestCase):
    BASE = pathlib.Path("assets/animations/biped/unarmed/base.glb")
    SOURCE_DIR = pathlib.Path("assets_src/biped/unarmed")
    RUNTIME_DIR = pathlib.Path("assets/animations/biped/unarmed")

    def test_arrived_motions_match_base_and_catalog_timelines(self):
        expected = {"idle_relaxed": 0}
        for motion, last_frame in expected.items():
            with self.subTest(motion=motion):
                duration, targets = MODULE.prepare_motion(
                    self.SOURCE_DIR / f"{motion}.glb",
                    self.BASE,
                    self.RUNTIME_DIR / f"{motion}.glb",
                    last_frame=last_frame,
                    check=True,
                )
                self.assertGreaterEqual(duration + 0.5 / MODULE.ANIMATION_FPS, last_frame / 30.0)
                self.assertEqual(targets, 131)

    def test_non_locomotion_runtime_motion_contains_animation_but_no_mesh(self):
        source = self.SOURCE_DIR / "idle_relaxed.glb"
        with tempfile.TemporaryDirectory() as directory:
            destination = pathlib.Path(directory) / "idle_relaxed.glb"
            MODULE.prepare_motion(source, self.BASE, destination, last_frame=0)
            document, binary = MODULE.read_glb(destination)
            self.assertNotEqual(destination.read_bytes(), source.read_bytes())
            self.assertLess(len(destination.read_bytes()), len(source.read_bytes()))
            self.assertLess(len(destination.read_bytes()), 60_000)
            self.assertNotIn("meshes", document)
            self.assertNotIn("skins", document)
            self.assertTrue(document["animations"][0]["channels"])
            self.assertLess(len(document["animations"][0]["channels"]), 131 * 3)
            self.assertTrue(binary)
            self.assertTrue(all("mesh" not in node for node in document["nodes"]))
            self.assertTrue(all("skin" not in node for node in document["nodes"]))
            MODULE.prepare_motion(source, self.BASE, destination, last_frame=0, check=True)

    def test_foreign_animation_target_is_rejected(self):
        base, _ = MODULE.read_glb(self.BASE)
        motion, _ = MODULE.read_glb(self.SOURCE_DIR / "idle_relaxed.glb")
        foreign = next(
            index
            for index, node in enumerate(motion["nodes"])
            if node["name"] == "John Fabelgeist"
        )
        motion["animations"][0]["channels"][0]["target"]["node"] = foreign
        with self.assertRaisesRegex(MODULE.GlbError, "foreign base path"):
            MODULE.validate_motion(base, motion, last_frame=0)

    def test_quickstep_import_removes_only_lateral_root_motion(self):
        source = self.SOURCE_DIR / "quickstep_forward.glb"
        source_document, source_binary = MODULE.read_glb(source)
        optimized, optimized_binary = MODULE.optimize_animation(
            source_document,
            source_binary,
            kept_frames=(3, 6, 9),
            remove_root_lateral_motion=True,
        )

        source_paths = MODULE.scene_paths(source_document)
        optimized_paths = MODULE.scene_paths(optimized)

        def root_translation(document, binary, paths):
            animation = document["animations"][0]
            channel = next(
                channel
                for channel in animation["channels"]
                if paths[channel["target"]["node"]] == MODULE.ROOT_PATH
                and channel["target"]["path"] == "translation"
            )
            sampler = animation["samplers"][channel["sampler"]]
            return (
                MODULE.accessor_view(document, binary, sampler["input"])[:, 0],
                MODULE.accessor_view(document, binary, sampler["output"]),
            )

        source_times, source_values = root_translation(
            source_document, source_binary, source_paths
        )
        output_times, output_values = root_translation(
            optimized, optimized_binary, optimized_paths
        )
        source_indices = [
            int(np.flatnonzero(np.isclose(source_times, frame / 30.0))[0])
            for frame in (3, 6, 9)
        ]
        root_node = next(
            node
            for index, node in enumerate(optimized["nodes"])
            if optimized_paths[index] == MODULE.ROOT_PATH
        )

        np.testing.assert_allclose(output_times, np.asarray((0.1, 0.2, 0.3)))
        np.testing.assert_allclose(output_values[:, 1], source_values[source_indices, 1])
        np.testing.assert_allclose(output_values[:, 0], root_node["translation"][0])
        np.testing.assert_allclose(output_values[:, 2], root_node["translation"][2])


if __name__ == "__main__":
    unittest.main()
