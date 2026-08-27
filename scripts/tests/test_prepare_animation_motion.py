import importlib.util
import pathlib
import sys
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
    RUNTIME_DIR = pathlib.Path("assets/animations/biped/unarmed")
    SOURCE_DIR = RUNTIME_DIR

    def test_runtime_motions_match_base_and_catalog_timelines(self):
        base, _ = MODULE.read_glb(self.BASE)
        expected = {"idle_relaxed": 0}
        for motion, last_frame in expected.items():
            with self.subTest(motion=motion):
                document, _ = MODULE.read_glb(self.RUNTIME_DIR / f"{motion}.glb")
                duration, targets = MODULE.validate_motion(
                    base, document, last_frame=last_frame
                )
                self.assertGreaterEqual(duration + 0.5 / MODULE.ANIMATION_FPS, last_frame / 30.0)
                self.assertGreater(targets, 0)

    def test_non_locomotion_runtime_motion_contains_animation_but_no_mesh(self):
        source = self.SOURCE_DIR / "idle_relaxed.glb"
        document, binary = MODULE.read_glb(source)
        self.assertLess(len(source.read_bytes()), 60_000)
        self.assertNotIn("meshes", document)
        self.assertNotIn("skins", document)
        self.assertTrue(document["animations"][0]["channels"])
        self.assertLess(len(document["animations"][0]["channels"]), 131 * 3)
        self.assertTrue(binary)
        self.assertTrue(all("mesh" not in node for node in document["nodes"]))
        self.assertTrue(all("skin" not in node for node in document["nodes"]))

    def test_foreign_animation_target_is_rejected(self):
        base, _ = MODULE.read_glb(self.BASE)
        motion, _ = MODULE.read_glb(self.SOURCE_DIR / "idle_relaxed.glb")
        foreign = len(motion["nodes"])
        motion["nodes"].append({"name": "foreign_animation_target"})
        motion["scenes"][motion.get("scene", 0)]["nodes"].append(foreign)
        motion["animations"][0]["channels"][0]["target"]["node"] = foreign
        with self.assertRaisesRegex(MODULE.GlbError, "foreign base path"):
            MODULE.validate_motion(base, motion, last_frame=0)

    def test_runtime_quickstep_removes_lateral_root_motion(self):
        source = self.SOURCE_DIR / "quickstep_forward.glb"
        optimized, optimized_binary = MODULE.read_glb(source)
        optimized_paths = MODULE.scene_paths(optimized)

        def root_translation(document, binary, paths):
            animation = document["animations"][0]
            channel = next(
                channel
                for channel in animation["channels"]
                if paths[channel["target"]["node"]]
                == ("Skeleton", "body_world", "root")
                and channel["target"]["path"] == "translation"
            )
            sampler = animation["samplers"][channel["sampler"]]
            return (
                MODULE.accessor_view(document, binary, sampler["input"])[:, 0],
                MODULE.accessor_view(document, binary, sampler["output"]),
            )

        output_times, output_values = root_translation(
            optimized, optimized_binary, optimized_paths
        )
        root_node = next(
            node
            for index, node in enumerate(optimized["nodes"])
            if optimized_paths.get(index) == ("Skeleton", "body_world", "root")
        )

        np.testing.assert_allclose(
            output_times,
            np.arange(13, dtype=np.float32) / MODULE.ANIMATION_FPS,
        )
        np.testing.assert_allclose(output_values[:, 0], 0.0)
        np.testing.assert_allclose(output_values[:, 2], 0.0)
        np.testing.assert_allclose(np.asarray(root_node["translation"])[[0, 2]], 0.0)


if __name__ == "__main__":
    unittest.main()
