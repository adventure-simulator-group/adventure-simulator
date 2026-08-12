import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "capture_tactical_scenes.py"
SPEC = importlib.util.spec_from_file_location("capture_tactical_scenes", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class CaptureTacticalScenesTests(unittest.TestCase):
    def test_default_matrix_is_compact_and_environment_only(self):
        matrix = MODULE.selected_matrix(None, None)
        self.assertEqual(len(matrix), 9)
        self.assertLessEqual(sum(len(case.views) for case, _, _ in matrix), 50)
        self.assertNotIn("heavy-rain-high-wind", {case.fixture for case, _, _ in matrix})

    def test_fixture_and_named_time_filters_cross_product(self):
        matrix = MODULE.selected_matrix(["steep-open-hillside"], ["noon", "moonlit"])
        self.assertEqual(
            [(case.fixture, name, minute) for case, name, minute in matrix],
            [
                ("steep-open-hillside", "noon", MODULE.NAMED_TIMES["noon"]),
                ("steep-open-hillside", "moonlit", MODULE.NAMED_TIMES["moonlit"]),
            ],
        )
        with self.assertRaises(ValueError):
            MODULE.selected_matrix(["heavy-rain-high-wind"], None)

    def test_child_manifest_gate_fails_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest_path = root / "manifest.json"
            manifest = {
                "fixture": "steep-open-hillside",
                "absolute_minute": MODULE.NAMED_TIMES["grazing"],
                "pipeline": MODULE.EXPECTED_PIPELINE,
                "capture_profile": "environment-review",
                "capture_profile_version": MODULE.EXPECTED_PROFILE_VERSION,
                "camera_version": MODULE.EXPECTED_CAMERA_VERSION,
                "resolution": MODULE.EXPECTED_RESOLUTION,
                "source_identity": "source-id",
                "revision": "head",
                "requested_views": ["rock-detail"],
                "captures": [{"view": "rock-detail"}],
                "validation": {"passed": True},
            }
            (root / "rock-detail.png").write_bytes(b"x" * 65)
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            self.assertEqual(
                MODULE.validated_child_manifest(
                    manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                    ("rock-detail",), "source-id", "head",
                ),
                manifest,
            )
            for field, wrong in (("captures", []), ("fixture", "wrong"),
                                 ("absolute_minute", 0), ("source_identity", "stale"),
                                 ("revision", "wrong"), ("camera_version", 99),
                                 ("resolution", [1, 1])):
                broken = dict(manifest)
                broken[field] = wrong
                manifest_path.write_text(json.dumps(broken), encoding="utf-8")
                with self.assertRaises(ValueError, msg=field):
                    MODULE.validated_child_manifest(
                        manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                        ("rock-detail",), "source-id", "head",
                    )

    def test_png_gate_rejects_extra_or_truncated_images(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "rock-detail.png").write_bytes(b"x" * 65)
            MODULE.validated_png_set(root, ("rock-detail",))
            (root / "extra.png").write_bytes(b"x" * 65)
            with self.assertRaises(ValueError):
                MODULE.validated_png_set(root, ("rock-detail",))

    def test_moonlit_slot_is_distinct_verified_lunar_evidence(self):
        self.assertEqual(MODULE.NAMED_TIMES["moonlit"], 359_940)


if __name__ == "__main__":
    unittest.main()
