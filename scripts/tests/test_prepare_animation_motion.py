import importlib.util
import pathlib
import sys
import tempfile
import unittest


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


if __name__ == "__main__":
    unittest.main()
