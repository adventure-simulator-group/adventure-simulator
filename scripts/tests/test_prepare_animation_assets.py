from __future__ import annotations

import importlib.util
import pathlib
import sys
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "prepare_animation_assets", ROOT / "scripts" / "prepare_animation_assets.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class PrepareAnimationAssetsTests(unittest.TestCase):
    def test_combat_cycles_close_after_the_four_authored_keys(self):
        self.assertEqual(MODULE.COMBAT_CYCLE_AUTHORED_FRAMES, (0, 6, 12, 18))
        self.assertEqual(MODULE.COMBAT_CYCLE_LAST_FRAME, 24)

    def test_quicksteps_preserve_idle_endpoints_and_authored_action_frames(self):
        for motion in (
            "quickstep_forward",
            "quickstep_right",
            "quickstep_left",
            "quickstep_back",
        ):
            self.assertEqual(MODULE.DIRECT_MOTIONS[motion], tuple(range(13)))

    def test_unknown_source_motion_is_rejected_instead_of_silently_copied(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = pathlib.Path(temporary) / "source"
            source.mkdir()
            (source / "mystery.glb").write_bytes(b"not a GLB")
            with self.assertRaisesRegex(
                MODULE.GlbError, "absent from the publication contract: mystery"
            ):
                MODULE.publish_animation_assets(
                    source_dir=source,
                    runtime_dir=pathlib.Path(temporary) / "runtime",
                    runtime_base=MODULE.RUNTIME_BASE,
                )

    def test_publishes_every_available_source_and_generated_counterpart_mesh_free(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = pathlib.Path(temporary)
            report = MODULE.publish_animation_assets(
                source_dir=MODULE.SOURCE_DIR,
                runtime_dir=runtime,
                runtime_base=MODULE.RUNTIME_BASE,
            )

            authored = {
                path.stem
                for path in MODULE.SOURCE_DIR.glob("*.glb")
                if path.stem != "base"
            }
            expected = authored | {
                output
                for source, output in MODULE.MIRRORED_MOTIONS.items()
                if source in authored
            }
            self.assertEqual(set(report.published), expected)
            for motion in report.published:
                path = runtime / f"{motion}.glb"
                document, _ = MODULE.read_glb(path)
                self.assertNotIn("meshes", document, motion)
                self.assertNotIn("skins", document, motion)
                self.assertTrue(document["animations"], motion)
                self.assertTrue(all("mesh" not in node for node in document["nodes"]))

            checked = MODULE.publish_animation_assets(
                source_dir=MODULE.SOURCE_DIR,
                runtime_dir=runtime,
                runtime_base=MODULE.RUNTIME_BASE,
                check=True,
            )
            self.assertEqual(checked, report)


if __name__ == "__main__":
    unittest.main()
