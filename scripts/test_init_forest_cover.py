import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "init_forest_cover", Path(__file__).with_name("init_forest_cover.py")
)
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class ForestCoverInitializerTests(unittest.TestCase):
    def test_dotenv_loads_only_values_without_expansion(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / ".env"
            path.write_text(
                "# comment\nexport COPERNICUS_CLIENT_ID='client'\n"
                'COPERNICUS_CLIENT_SECRET="secret$value"\n',
                encoding="utf-8",
            )
            self.assertEqual(
                module.dotenv(path),
                {"COPERNICUS_CLIENT_ID": "client", "COPERNICUS_CLIENT_SECRET": "secret$value"},
            )

    def test_environment_credentials_override_file(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / ".env"
            path.write_text("COPERNICUS_CLIENT_ID=file\nCOPERNICUS_CLIENT_SECRET=file\n")
            with patch.dict(
                os.environ,
                {"COPERNICUS_CLIENT_ID": "environment", "COPERNICUS_CLIENT_SECRET": "secret"},
            ):
                self.assertEqual(module.credentials(path), ("environment", "secret"))

    def test_default_plan_covers_playable_area_source_envelope(self):
        with tempfile.TemporaryDirectory() as directory:
            env = Path(directory) / ".env"
            client_id = "credential-client-id-marker"
            client_secret = "credential-client-secret-marker"
            env.write_text(
                f"COPERNICUS_CLIENT_ID={client_id}\n"
                f"COPERNICUS_CLIENT_SECRET={client_secret}\n"
            )
            result = module.plan(env, Path(directory) / "forest", module.DEFAULT_BOUNDS)
            self.assertEqual(result["bounds"], [8, 50, 12, 53])
            self.assertEqual(result["degree_tiles"], 12)
            self.assertEqual(result["prepared_files"], 24)
            self.assertEqual(result["credential_preflight"], "present (values redacted)")
            serialized = json.dumps(result)
            self.assertNotIn(client_id, serialized)
            self.assertNotIn(client_secret, serialized)

    def test_tile_names_are_importer_compatible(self):
        self.assertEqual(module.tile_key(53, 9), "N53_E009")
        self.assertEqual(module.tile_key(-2, -7), "S02_W007")
        names = module.expected_names((9, 53, 11, 54))
        self.assertEqual(
            names,
            ["DLT_N53_E009.tif", "DLT_N53_E010.tif", "TCD_N53_E009.tif", "TCD_N53_E010.tif"],
        )

    def test_tcd_request_uses_fixed_grid_and_collection(self):
        request = json.loads(module.request_payload("TCD", 53, 9))
        self.assertEqual(request["input"]["bounds"]["bbox"], [9, 53, 10, 54])
        self.assertEqual(request["output"]["width"], 1000)
        self.assertEqual(request["output"]["height"], 1000)
        self.assertEqual(
            request["input"]["data"][0]["type"], f"byoc-{module.COLLECTIONS['tcd']}"
        )
        self.assertIn("nodataValue: 255", request["evalscript"])

    def test_dlt_request_uses_official_aggregated_leaf_densities(self):
        request = json.loads(module.request_payload("DLT", 53, 9))
        self.assertEqual([item["id"] for item in request["input"]["data"]], ["bcd", "ccd"])
        self.assertIn("broadleaf.BCD * 4 >= total * 3", request["evalscript"])
        self.assertIn("conifer.CCD * 4 >= total * 3", request["evalscript"])
        self.assertIn("if (total <= 0) return [255]", request["evalscript"])
        self.assertIn("return [3]", request["evalscript"])

    def test_bounds_are_bounded(self):
        with self.assertRaises(Exception):
            module.validate_bounds((5, 50, 5, 56))
        with self.assertRaises(Exception):
            module.validate_bounds((-20, 20, 20, 40))

    def test_explicit_source_tree_keeps_backup_beside_that_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "target/world-data-sources/raw/forest-cover"
            staging = root / "target/world-data-sources/raw/.forest-cover-staging"
            output.mkdir(parents=True)
            staging.mkdir()
            (output / "old").write_text("old")
            (staging / "new").write_text("new")
            backup = module.publish(staging, output)
            self.assertIsNotNone(backup)
            self.assertEqual(backup.parent, root / "target/world-data-backups")
            self.assertEqual((backup / "old").read_text(), "old")
            self.assertEqual((output / "new").read_text(), "new")


if __name__ == "__main__":
    unittest.main()
