import importlib.util
import inspect
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
    def test_expected_camera_version_matches_scene_capture_contract(self):
        self.assertEqual(MODULE.EXPECTED_CAMERA_VERSION, 9)

    def test_source_identity_includes_all_viewer_modules(self):
        self.assertIn(
            "crates/adventuresim-tactical-client/src/tactical_scene_viewer",
            MODULE.SOURCE_PATHS,
        )
        included = {path.name for path in MODULE.source_files()}
        self.assertIn("capture_state.rs", included)
        self.assertIn("manifest.rs", included)

    def test_default_matrix_is_compact_and_environment_only(self):
        matrix = MODULE.selected_matrix(None, None)
        self.assertEqual(len(matrix), 12)
        self.assertLessEqual(sum(len(case.views) for case, _, _ in matrix), 54)
        self.assertNotIn("heavy-rain-high-wind", {case.fixture for case, _, _ in matrix})
        sparse = [(case, time_name) for case, time_name, _ in matrix if case.fixture == "sparse-woodland"]
        for expected_time in ("morning", "grazing"):
            case = next(case for case, time_name in sparse if time_name == expected_time)
            self.assertIn("forest-floor-debris-detail", case.views)

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
                "generation_version": MODULE.EXPECTED_GENERATION_VERSION,
                "scene_source": {
                    "kind": "synthetic_fixture",
                    "id": "steep-open-hillside",
                },
                "resolution": MODULE.EXPECTED_RESOLUTION,
                "source_identity": "source-id",
                "revision": "head",
                "celestial": {
                    "sun_altitude_degrees": 8.0,
                    "moon_altitude_degrees": -20.0,
                    "lunar_illumination": 0.2,
                },
                "presentation_features": {
                    "requested": MODULE.EXPECTED_PRESENTATION_REQUEST,
                    "observed": {
                        "settings": MODULE.EXPECTED_PRESENTATION_REQUEST,
                        "camera_environment_map": True,
                        "camera_environment_map_size": [64, 64],
                        "camera_environment_map_allocated": True,
                        "camera_environment_map_intensity": 1.0,
                        "camera_exposure_ev100": 14.7,
                        "camera_tonemapping": "AcesFitted",
                        "ambient_color": [1.0, 1.0, 1.0, 1.0],
                        "ambient_brightness": 10500.0,
                        "expected_ambient_brightness": 10500.0,
                        "ambient_policy": "atmosphere_ibl_plus_bounded_multibounce",
                    },
                    "requested_matches_observed": True,
                },
                "requested_views": ["rock-detail"],
                "captures": [{
                    "view": "rock-detail",
                    "lighting_ready": True,
                    "forced_tree_lod": None,
                    "focused_tree_lod_queued": None,
                }],
                "validation": {"passed": True, "lighting_readiness": True},
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
                                  ("generation_version", 99),
                                  ("scene_source", {"kind": "synthetic_fixture", "id": "wrong"}),
                                 ("resolution", [1, 1]),
                                 ("presentation_features", {"celestial": False})):
                broken = dict(manifest)
                broken[field] = wrong
                manifest_path.write_text(json.dumps(broken), encoding="utf-8")
                with self.assertRaises(ValueError, msg=field):
                    MODULE.validated_child_manifest(
                        manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                        ("rock-detail",), "source-id", "head",
                    )

            for wrong_ambient in (0.0, 0.6):
                broken = json.loads(json.dumps(manifest))
                broken["presentation_features"]["observed"]["ambient_brightness"] = wrong_ambient
                manifest_path.write_text(json.dumps(broken), encoding="utf-8")
                with self.assertRaises(ValueError):
                    MODULE.validated_child_manifest(
                        manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                        ("rock-detail",), "source-id", "head",
                    )

            night = json.loads(json.dumps(manifest))
            night["celestial"] = {
                "sun_altitude_degrees": -25.0,
                "moon_altitude_degrees": -20.0,
                "lunar_illumination": 0.0,
            }
            night["presentation_features"]["observed"]["ambient_brightness"] = 10500.0
            manifest_path.write_text(json.dumps(night), encoding="utf-8")
            with self.assertRaises(ValueError):
                MODULE.validated_child_manifest(
                    manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                    ("rock-detail",), "source-id", "head",
                )

            forced = json.loads(json.dumps(manifest))
            forced["captures"][0]["view"] = "tree-billboard-lod"
            forced["requested_views"] = ["tree-billboard-lod"]
            forced["captures"][0]["forced_tree_lod"] = 4
            forced["captures"][0]["focused_tree_lod_queued"] = False
            (root / "rock-detail.png").unlink()
            (root / "tree-billboard-lod.png").write_bytes(b"x" * 65)
            manifest_path.write_text(json.dumps(forced), encoding="utf-8")
            with self.assertRaises(ValueError):
                MODULE.validated_child_manifest(
                    manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                    ("tree-billboard-lod",), "source-id", "head",
                )
            for bad_lod in (None, 3):
                broken = json.loads(json.dumps(forced))
                broken["captures"][0]["forced_tree_lod"] = bad_lod
                broken["captures"][0]["focused_tree_lod_queued"] = True
                manifest_path.write_text(json.dumps(broken), encoding="utf-8")
                with self.assertRaises(ValueError):
                    MODULE.validated_child_manifest(
                        manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                        ("tree-billboard-lod",), "source-id", "head",
                    )

            leaf_lod = json.loads(json.dumps(forced))
            leaf_lod["captures"][0]["view"] = "tree-textured-leaf-lod"
            leaf_lod["requested_views"] = ["tree-textured-leaf-lod"]
            leaf_lod["captures"][0]["forced_tree_lod"] = 0
            leaf_lod["captures"][0]["focused_tree_lod_queued"] = True
            (root / "tree-billboard-lod.png").unlink()
            (root / "tree-textured-leaf-lod.png").write_bytes(b"x" * 65)
            manifest_path.write_text(json.dumps(leaf_lod), encoding="utf-8")
            MODULE.validated_child_manifest(
                manifest_path, "steep-open-hillside", MODULE.NAMED_TIMES["grazing"],
                ("tree-textured-leaf-lod",), "source-id", "head",
            )

    def test_png_gate_rejects_extra_or_truncated_images(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "rock-detail.png").write_bytes(b"x" * 65)
            MODULE.validated_png_set(root, ("rock-detail",))
            (root / "extra.png").write_bytes(b"x" * 65)
            with self.assertRaises(ValueError):
                MODULE.validated_png_set(root, ("rock-detail",))

    def test_debris_child_requires_bounded_leaf_and_twig_evidence(self):
        source = inspect.getsource(MODULE.validated_child_manifest)
        self.assertIn('"debris_leaf_distance_metres"', source)
        self.assertIn('"debris_twig_distance_metres"', source)
        self.assertIn("<= .275", source)

    def test_moonlit_slot_is_distinct_verified_lunar_evidence(self):
        self.assertEqual(MODULE.NAMED_TIMES["moonlit"], 359_940)
        self.assertEqual(MODULE.SKY_MINUTES["moon"], MODULE.NAMED_TIMES["moonlit"])
        self.assertEqual(MODULE.SKY_SETTLE_FRAMES_MIN, 96)

    def test_sky_manifest_requires_current_semantic_pipeline(self):
        source = inspect.getsource(MODULE.validated_sky_manifest)
        self.assertIn('"tactical_sky_native_capture_v3"', source)
        self.assertIn('"upper_sky_luma_variance"', source)
        self.assertIn('"solar_source_illuminance_lux"', source)
        self.assertIn('expected_view == "moon"', source)
        self.assertIn('manifest.get("moon_altitude_degrees", -90) > 20', source)
        self.assertIn('manifest.get("lunar_illumination", 0) > .9', source)


if __name__ == "__main__":
    unittest.main()
