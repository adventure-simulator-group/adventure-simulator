#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
SPEC = importlib.util.spec_from_file_location("world_runtime_release", SCRIPT_DIR / "world_runtime_release.py")
assert SPEC and SPEC.loader
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)


def write_fixture(root: Path, blocked: bool = False) -> None:
    map_dir = root / "target/strategic-map"
    map_dir.mkdir(parents=True)
    tile_pack = b"avif-pack"
    terrain_pack = b"terrain-pack"
    source_identity = {"release-blocked": {"reason": "test"}} if blocked else {"raw-sha256": {"sha256": "1" * 64}}
    world = {
        "metadata": {
            "world_year": 1544,
            "sources": [{
                "id": "fixture",
                "name": "Fixture source",
                "canonical_url": "https://example.com/source",
                "license": "cc0-1-0",
                "required_notices": ["Retain fixture attribution."],
                "content_identity": source_identity,
                "notes_markdown": "Fixture notes.",
            }],
        },
        "nodes": [],
    }
    (root / "target/world-1544.json").write_text(json.dumps(world), encoding="utf-8")
    terrain_digest = runtime.bytes_sha256(terrain_pack)
    terrain_package = "2" * 64
    map_manifest = {
        "schema": 5,
        "year": 1544,
        "tiles": {"format": "avif", "content_sha256": runtime.bytes_sha256(tile_pack)},
        "terrain_package_sha256": terrain_package,
        "cultivation": {
            "grid_crs": "EPSG:3035",
            "grid_resolution_m": 1000,
            "rules_version": 1,
            "source_sha256": "3" * 64,
            "square_count": 7,
        },
    }
    terrain_manifest = {
        "schema": 8,
        "purpose": "final",
        "content_sha256": terrain_digest,
        "package_sha256": terrain_package,
        "cultivation_grid_crs": "EPSG:3035",
        "cultivation_grid_resolution_m": 1000,
        "cultivation_rules_version": 1,
        "cultivation_source_sha256": "3" * 64,
        "cultivated_square_count": 7,
    }
    (map_dir / "strategic-map-v1.json").write_text(json.dumps(map_manifest), encoding="utf-8")
    (map_dir / "strategic-map-tiles-v1.pack").write_bytes(tile_pack)
    (map_dir / "terrain-routing-v3.json").write_text(json.dumps(terrain_manifest), encoding="utf-8")
    (map_dir / "terrain-routing-v3.pack").write_bytes(terrain_pack)
    (map_dir / "STRATEGIC_MAP_DATA_LICENSE.md").write_text("fixture map notice\n", encoding="utf-8")


class RuntimeReleaseTests(unittest.TestCase):
    def test_build_verify_and_install_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            destination = root / "destination"
            source.mkdir()
            destination.mkdir()
            write_fixture(source)
            archive = root / "runtime.zip"
            lock_path = root / "runtime.lock.json"
            lock = runtime.build(source, archive, lock_path, "1544-test")
            runtime.inspect(archive, lock)
            runtime.install(archive, lock, destination)
            self.assertTrue(runtime.installed_files_match(destination, lock))
            self.assertIn("Fixture source", (destination / "target/WORLD_RUNTIME_DATA_NOTICE.md").read_text(encoding="utf-8"))

    def test_build_preserves_release_blocked_source_warning(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_fixture(root, blocked=True)
            archive = root / "runtime.zip"
            lock = runtime.build(root, archive, root / "runtime.lock.json", "1544-test")
            destination = root / "installed"
            destination.mkdir()
            runtime.install(archive, lock, destination)
            notice = (destination / "target/WORLD_RUNTIME_DATA_NOTICE.md").read_text(encoding="utf-8")
            self.assertIn("Upstream reproducibility warning: test", notice)

    def test_lock_rejects_untrusted_host(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_fixture(root)
            lock = runtime.build(root, root / "runtime.zip", root / "runtime.lock.json", "1544-test")
            lock["archive_url"] = "https://example.com/releases/world-runtime/runtime.zip"
            with self.assertRaisesRegex(RuntimeError, "unsafe archive URL"):
                runtime.validate_lock(lock)

    def test_install_preserves_mismatched_local_artifacts_without_replace(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            destination = root / "destination"
            source.mkdir()
            destination.mkdir()
            write_fixture(source)
            archive = root / "runtime.zip"
            lock = runtime.build(source, archive, root / "runtime.lock.json", "1544-test")
            local = destination / "target/world-1544.json"
            local.parent.mkdir(parents=True)
            local.write_text("local", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "already exist"):
                runtime.install(archive, lock, destination)
            self.assertEqual(local.read_text(encoding="utf-8"), "local")

    def test_install_completes_partial_matching_install(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            destination = root / "destination"
            source.mkdir()
            destination.mkdir()
            write_fixture(source)
            archive = root / "runtime.zip"
            lock = runtime.build(source, archive, root / "runtime.lock.json", "1544-test")
            first = lock["files"][0]
            existing = destination / first["destination"]
            existing.parent.mkdir(parents=True)
            with zipfile.ZipFile(archive) as bundle:
                existing.write_bytes(bundle.read(first["path"]))
            runtime.install(archive, lock, destination)
            self.assertTrue(runtime.installed_files_match(destination, lock))

    def test_publish_refuses_to_overwrite_immutable_object(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            write_fixture(root)
            archive = root / "runtime.zip"
            lock = runtime.build(root, archive, root / "runtime.lock.json", "1544-test")
            with mock.patch.object(runtime.shutil, "which", return_value="aws"), \
                 mock.patch.object(runtime.world_data_bundle, "r2_environment", return_value=({}, "https://account.r2.cloudflarestorage.com")), \
                 mock.patch.object(runtime.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, stdout="{}")) as run:
                with self.assertRaisesRegex(RuntimeError, "refusing to overwrite"):
                    runtime.publish(archive, lock, root / ".env")
            self.assertEqual(run.call_count, 1)


if __name__ == "__main__":
    unittest.main()
