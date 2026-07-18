from __future__ import annotations

import hashlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import world_source_init as init


class Response(io.BytesIO):
    def __init__(self, payload: bytes, *, length: int | None = None, url: str | None = None):
        super().__init__(payload)
        self.headers = {} if length is None else {"Content-Length": str(length)}
        self._url = url or init.CONTRACTS["trees4f"].canonical_url

    def geturl(self):
        return self._url

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()


class Opener:
    def __init__(self, response):
        self.response = response

    def open(self, request, timeout):
        return self.response


class TrickleResponse(Response):
    def read1(self, size):
        return b"x"


class InitializerTests(unittest.TestCase):
    def inventory(self, root: Path, source: str, files: list[dict]) -> None:
        contract = init.CONTRACTS[source]
        (root / "source-inventory.json").write_text(json.dumps({
            "schema": 1, "source": contract.source, "version": contract.version,
            "files": files,
        }), encoding="utf-8")

    def test_redirect_allowlist_rejects_cross_host_and_query(self):
        with self.assertRaisesRegex(RuntimeError, "allowlist"):
            init.validate_url("https://evil.example/EU-Trees4F_ens-clim.zip", {"ies-ows.jrc.ec.europa.eu"}, "/efdac/")
        with self.assertRaisesRegex(RuntimeError, "allowlist"):
            init.validate_url("https://ies-ows.jrc.ec.europa.eu/efdac/file.zip?token=secret", {"ies-ows.jrc.ec.europa.eu"}, "/efdac/")

    def test_oversize_and_partial_download_do_not_replace_valid_file(self):
        payload = b"valid candidate"
        pinned = {"name": "EU-Trees4F_ens-clim.zip", "size": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}
        with tempfile.TemporaryDirectory() as name, mock.patch.dict(init.TREES_FILE, pinned, clear=True):
            root = Path(name)
            current = root / pinned["name"]
            current.write_bytes(b"old remains")
            with mock.patch("world_source_init.urllib.request.build_opener", return_value=Opener(Response(payload, length=len(payload) + 1))):
                with self.assertRaisesRegex(RuntimeError, "Content-Length"):
                    init.download_trees(root, force=True)
            self.assertEqual(current.read_bytes(), b"old remains")
            with mock.patch("world_source_init.urllib.request.build_opener", return_value=Opener(Response(payload[:-1], length=None))):
                with self.assertRaisesRegex(RuntimeError, "checksum"):
                    init.download_trees(root, force=True)
            self.assertEqual(current.read_bytes(), b"old remains")

    def test_slow_download_hits_total_deadline(self):
        payload = b"candidate"
        pinned = {"name": "EU-Trees4F_ens-clim.zip", "size": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}
        with tempfile.TemporaryDirectory() as name, mock.patch.dict(init.TREES_FILE, pinned, clear=True), \
                mock.patch("world_source_init.urllib.request.build_opener", return_value=Opener(Response(payload))), \
                mock.patch("world_source_init.time.monotonic", side_effect=[0, 2]):
            with self.assertRaisesRegex(RuntimeError, "deadline"):
                init.download_trees(Path(name), deadline_seconds=1)

    def test_slow_trickle_read1_hits_true_deadline(self):
        payload = b"xx"
        pinned = {"name": "EU-Trees4F_ens-clim.zip", "size": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}
        with tempfile.TemporaryDirectory() as name, mock.patch.dict(init.TREES_FILE, pinned, clear=True), \
                mock.patch("world_source_init.urllib.request.build_opener", return_value=Opener(TrickleResponse(payload))), \
                mock.patch("world_source_init.time.monotonic", side_effect=[0.0, 0.1, 0.2, 0.9, 1.1]):
            with self.assertRaisesRegex(RuntimeError, "deadline"):
                init.download_trees(Path(name), deadline_seconds=1)

    def test_basename_rejects_ads_controls_reserved_and_normalized_duplicates(self):
        for name in ("../x", "x/y", "x\\y", "x:y", "x ", "x.", "a\n.tif", "CON", "nul.txt", "COM1.gpkg", "LPT9.foo"):
            with self.subTest(name=name), self.assertRaises(RuntimeError):
                init.validate_basename(name)
        self.assertEqual(init.validate_basename("EU_HYDRO-basin_01.gpkg"), "EU_HYDRO-basin_01.gpkg")
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            entries = [
                {"name": "A.gpkg", "size": 1, "sha256": "0" * 64},
                {"name": "a.gpkg", "size": 1, "sha256": "1" * 64},
            ]
            self.inventory(root, "eu-hydro", entries)
            with self.assertRaisesRegex(RuntimeError, "normalized"):
                init.load_inventory(root, init.CONTRACTS["eu-hydro"])

    def test_open_inventory_structures_are_source_specific(self):
        cases = (
            ("glo30", ["junk.bin"], "GLO-30"),
            ("forest", ["TCD_N48_E002.tif"], "pair"),
            ("forest", ["TCD_N48_E002.tif", "DLT_N49_E002.tif"], "pair"),
            ("eu-hydro", ["junk.bin"], "GeoPackage"),
        )
        for source, names, message in cases:
            with self.subTest(source=source, names=names), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.inventory(root, source, [{"name": name, "size": 1, "sha256": "0" * 64} for name in sorted(names)])
                with self.assertRaisesRegex(RuntimeError, message):
                    init.load_inventory(root, init.CONTRACTS[source])
        valid = {
            "glo30": ["Copernicus_DSM_COG_10_N52_00_W002_00_DEM.tif"],
            "forest": ["DLT_N48_E002.tif", "TCD_N48_E002.tif"],
            "eu-hydro": ["EU_HYDRO_River_Net_Basin01.gpkg"],
        }
        for source, names in valid.items():
            with self.subTest(valid=source), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.inventory(root, source, [{"name": name, "size": 1, "sha256": "0" * 64} for name in names])
                self.assertEqual(len(init.load_inventory(root, init.CONTRACTS[source])), len(names))

    def test_positive_coordinate_edges_are_not_importer_tiles(self):
        invalid = {
            "glo30": ["Copernicus_DSM_COG_10_N90_00_E180_00_DEM.tif"],
            "forest": ["DLT_N90_E180.tif", "TCD_N90_E180.tif"],
        }
        for source, names in invalid.items():
            with self.subTest(source=source), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.inventory(
                    root,
                    source,
                    [{"name": name, "size": 1, "sha256": "0" * 64} for name in names],
                )
                with self.assertRaisesRegex(RuntimeError, "importer-compatible"):
                    init.load_inventory(root, init.CONTRACTS[source])

    def test_valid_active_repairs_missing_generation_and_rejects_reparse_escape(self):
        payload = b"valid"
        pinned = {"name": "EU-Trees4F_ens-clim.zip", "size": len(payload), "sha256": hashlib.sha256(payload).hexdigest()}
        with tempfile.TemporaryDirectory() as name, mock.patch.dict(init.TREES_FILE, pinned, clear=True):
            root = Path(name)
            (root / pinned["name"]).write_bytes(payload)
            init.download_trees(root)
            generation = root / "generations" / pinned["sha256"] / pinned["name"]
            self.assertEqual(generation.read_bytes(), payload)
            generation.unlink()
            with mock.patch("world_source_init.has_reparse_point", side_effect=lambda path: path.name == pinned["sha256"]):
                with self.assertRaisesRegex(RuntimeError, "reparse"):
                    init.download_trees(root)

    def test_generation_resolved_escape_is_rejected(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            escaped = root.parent / "escaped-generation"
            with mock.patch.object(Path, "resolve", side_effect=[root, escaped]):
                with self.assertRaisesRegex(RuntimeError, "escapes"):
                    init.validated_directory_child(root, "generations")

    def test_traversal_symlink_duplicate_and_extra_inventory_fail(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            contract = init.CONTRACTS["hyde35"]
            self.inventory(root, "hyde35", [{"name": "../escape", "size": 1, "sha256": "0" * 64}])
            with self.assertRaises(RuntimeError):
                init.load_inventory(root, contract)
            duplicate = [{"name": "cropland.nc", "size": 1, "sha256": "0" * 64}] * 2
            self.inventory(root, "hyde35", duplicate)
            with self.assertRaisesRegex(RuntimeError, "duplicate"):
                init.load_inventory(root, contract)
            self.inventory(root, "hyde35", [{"name": "extra.nc4", "size": 1, "sha256": "0" * 64}])
            with self.assertRaisesRegex(RuntimeError, "missing or adds"):
                init.load_inventory(root, contract)
            outside = root.parent / "outside-world-source-test"
            outside.write_bytes(b"x")
            link = root / "LUHa_u2.v1_gcrop.nc4"
            try:
                os.symlink(outside, link)
            except OSError:
                self.skipTest("symlinks unavailable")
            with self.assertRaisesRegex(RuntimeError, "symbolic"):
                init.safe_child(root, "LUHa_u2.v1_gcrop.nc4")
            outside.unlink(missing_ok=True)

    def test_secret_preflight_is_redacted_and_absence_is_explicit(self):
        contract = init.CONTRACTS["glo30"]
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertIn("absent", init.credential_status(contract))
        with tempfile.NamedTemporaryFile(delete=False) as stream:
            stream.write(b"very-secret-token")
            credential = stream.name
        try:
            with mock.patch.dict(os.environ, {"CDSE_TOKEN_FILE": credential}, clear=True):
                status = init.credential_status(contract)
                self.assertIn("redacted", status)
                self.assertNotIn("very-secret-token", status)
                self.assertNotIn(credential, status)
        finally:
            Path(credential).unlink(missing_ok=True)

    def test_manifest_order_is_deterministic(self):
        contract = init.CONTRACTS["hyde35"]
        value = init.canonical_manifest(contract, [
            {"name": "z", "size": 1, "sha256": "0" * 64},
            {"name": "a", "size": 1, "sha256": "1" * 64},
        ], "release-blocked", "reason")
        self.assertEqual([entry["name"] for entry in value["files"]], ["a", "z"])
        self.assertEqual(json.dumps(value, sort_keys=True), json.dumps(value, sort_keys=True))

    def test_contract_ids_and_versions_match_compiler_manifests(self):
        expected = {
            "glo30": ("copernicus-dem-glo30", "GLO-30"),
            "hyde35": ("hyde-3-5-c9", "3.5 c9"),
            "forest": ("clms-forest-2018", "2018"),
            "trees4f": ("eu-trees4f-v2", "2"),
            "egdi": ("egdi-surface-geology-1m", "EGDI-GE-1M-SURFACE"),
            "eu-hydro": ("copernicus-eu-hydro-1-3", "1.3"),
        }
        self.assertEqual({key: (value.source, value.version) for key, value in init.CONTRACTS.items()}, expected)

    def test_curated_religion_tamper_and_schema_are_rejected(self):
        init.verify_religion()
        with tempfile.TemporaryDirectory() as name:
            path = Path(name) / "religion.csv"
            path.write_bytes(init.RELIGION_FILE.read_bytes() + b"tamper")
            with self.assertRaisesRegex(RuntimeError, "digest"):
                init.verify_religion(path)

    def test_atomic_manifest_rollback(self):
        with tempfile.TemporaryDirectory() as name:
            path = Path(name) / "manifest.json"
            path.write_text("old\n", encoding="utf-8")
            with mock.patch("world_source_init.os.replace", side_effect=OSError("stop")):
                with self.assertRaises(OSError):
                    init.write_atomic(path, {"new": True})
            self.assertEqual(path.read_text(encoding="utf-8"), "old\n")


if __name__ == "__main__":
    unittest.main()
