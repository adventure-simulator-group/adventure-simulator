import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import json
import subprocess
import urllib.error

SPEC = importlib.util.spec_from_file_location("init_soilgrids", Path(__file__).with_name("init_soilgrids.py"))
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class SoilGridsInitializerTests(unittest.TestCase):
    def test_inventory_is_fixed_and_complete(self):
        layers = module.layers()
        self.assertEqual(len(layers), 207)
        self.assertEqual(len({(x["property"], x["depth"], x["quantile"]) for x in layers}), 207)
        self.assertTrue(all(x["source_url"].startswith("https://files.isric.org/soilgrids/latest/") for x in layers))
        self.assertIn("sand_0-5cm_Q0.5.vrt", next(x["source_url"] for x in layers if x["property"] == "sand" and x["quantile"] == "Q0.50"))
        self.assertIn("data_aggregated/1000m/wv0033", next(x["source_url"] for x in layers if x["property"] == "wv0033"))

    def test_grid_size_is_bounded_and_aligned(self):
        self.assertEqual(module.cell_size("1000"), 1000)
        self.assertEqual(module.cell_size("5000"), 5000)
        for value in ("0", "250", "500", "750", "100250"):
            with self.assertRaises(Exception):
                module.cell_size(value)
        for extent in ((900001, 900000, 7400001, 5500000), (0, 0, 1000, 1000)):
            with self.assertRaises(Exception):
                module.validate_extent(extent, 1000)

    def test_plan_is_non_shell_and_tap_aligned(self):
        layer = module.layers()[0]
        command = module.command(layer, Path("input.vrt"), Path("output.tif"), 1000)
        self.assertEqual(command[0], "gdalwarp")
        self.assertIn("-tap", command)
        self.assertIn("-srcnodata", command)
        self.assertIn("-dstnodata", command)
        self.assertEqual(command[command.index("-ot") + 1], "Float32")
        self.assertEqual(command[command.index("-t_srs") + 1], "EPSG:3035")
        self.assertIn(f"GDAL_HTTP_MAX_RETRY={module.HTTP_RETRIES}", command)
        self.assertIn("GDAL_HTTP_RETRY_CODES=429,500,502,503,504", command)

    def test_retry_delay_is_bounded_exponential_backoff(self):
        self.assertEqual(module.retry_delay(1), module.HTTP_INITIAL_RETRY_DELAY_SECONDS)
        self.assertEqual(module.retry_delay(2), module.HTTP_INITIAL_RETRY_DELAY_SECONDS * 2)
        self.assertEqual(module.retry_delay(99), module.HTTP_MAX_RETRY_DELAY_SECONDS)
        with self.assertRaises(ValueError):
            module.retry_delay(0)

    @patch("time.sleep")
    @patch("subprocess.run")
    def test_transient_gdal_failure_retries_and_clears_partial_output(self, run, sleep):
        run.side_effect = [subprocess.CalledProcessError(1, ["gdalwarp"]), None]
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "prepared.tif"
            output.write_bytes(b"partial")
            module.warp_with_retries(module.layers()[0], output, 1000)
            self.assertEqual(run.call_count, 2)
            self.assertEqual(sleep.call_args.args[0], module.retry_delay(1))
            self.assertFalse(output.exists())

    @patch("time.sleep")
    @patch("urllib.request.build_opener")
    def test_source_metadata_retries_network_failure(self, opener, sleep):
        response = type("Response", (), {"geturl": lambda self: "https://files.isric.org/soilgrids/latest/data/sand/sand_0-5cm_Q0.05.vrt", "headers": {}, "read": lambda self, _: b"", "__enter__": lambda self: self, "__exit__": lambda self, *args: None})()
        opener.side_effect = [urllib.error.URLError("temporary"), type("Opener", (), {"open": lambda self, request, timeout: response})()]
        result = module.source_metadata("https://files.isric.org/soilgrids/latest/data/sand/sand_0-5cm_Q0.05.vrt")
        self.assertEqual(result["source_observation_size"], 0)
        self.assertEqual(sleep.call_args.args[0], module.retry_delay(1))

    @patch.object(module, "validate_prepared")
    def test_checkpoint_reuses_verified_layers_and_discards_uncheckpointed_output(self, validate):
        layer = module.layers()[0]
        with tempfile.TemporaryDirectory() as directory:
            staging = Path(directory) / ".soilgrids-staging"
            staging.mkdir()
            completed = staging / layer["filename"]
            completed.write_bytes(b"completed")
            record = {**layer, "source_observation_size": 0,
                "source_observation_sha256": "0" * 64, "source_observation_etag": None,
                "source_observation_last_modified": None, "prepared_size": completed.stat().st_size,
                "prepared_sha256": module.sha256(completed)}
            module.save_checkpoint(staging, 1000, [record])
            (staging / module.layers()[1]["filename"]).write_bytes(b"incomplete")

            records = module.load_checkpoint(staging, 1000)
            module.discard_uncheckpointed_layers(staging, records)

            self.assertEqual(records, [record])
            self.assertTrue(completed.exists())
            self.assertFalse((staging / module.layers()[1]["filename"]).exists())
            self.assertEqual(validate.call_count, 1)

    def test_vsicurl_preserves_master_url_for_relative_vrt_tiles(self):
        url = module.layers()[0]["source_url"]
        source = module.vsicurl(url)
        self.assertEqual(source, "/vsicurl/" + url)
        self.assertIn(source, module.command(module.layers()[0], source, Path("out.tif"), 1000))
        with self.assertRaises(RuntimeError):
            module.vsicurl("https://example.com/soilgrids/latest/data/x.vrt")

    def test_verify_rejects_missing_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):
                module.verify(Path(directory), 1000)

    @patch("subprocess.run")
    def test_gdalinfo_contract_is_checked_before_publication(self, run):
        info = {"size": [6500, 4600], "geoTransform": [900000, 1000, 0.0, 5500000, 0.0, -1000],
            "coordinateSystem": {"wkt": 'PROJCRS["ETRS89 / LAEA Europe",ID["EPSG",3035]]'},
            "bands": [{"type": "Float32", "noDataValue": "NaN"}],
            "metadata": {"IMAGE_STRUCTURE": {"COMPRESSION": "DEFLATE"}}}
        run.return_value = subprocess.CompletedProcess([], 0, stdout=json.dumps(info), stderr="")
        module.validate_prepared(Path("fixture.tif"), 1000)
        self.assertEqual(run.call_args.args[0][:2], ["gdalinfo", "-json"])
        info["geoTransform"][0] += 1
        run.return_value = subprocess.CompletedProcess([], 0, stdout=json.dumps(info), stderr="")
        with self.assertRaises(RuntimeError):
            module.validate_prepared(Path("fixture.tif"), 1000)


if __name__ == "__main__":
    unittest.main()
