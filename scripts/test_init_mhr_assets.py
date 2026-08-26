import hashlib
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import init_mhr_assets as init


class FakeResponse:
    def __init__(self, chunks, content_length=None):
        self.chunks = iter(chunks)
        self.headers = {}
        self.read_calls = 0
        if content_length is not None:
            self.headers["Content-Length"] = str(content_length)

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return False

    def read(self, _size):
        self.read_calls += 1
        return next(self.chunks, b"")


class InitMhrAssetsTests(unittest.TestCase):
    def test_download_is_size_and_checksum_pinned(self):
        payload = b"pinned MHR archive"
        response = FakeResponse([payload], len(payload))
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "assets.zip"
            with mock.patch.object(init.urllib.request, "urlopen", return_value=response):
                init.download(destination, len(payload), hashlib.sha256(payload).hexdigest())
            self.assertEqual(destination.read_bytes(), payload)

    def test_download_rejects_wrong_content_length_before_reading(self):
        response = FakeResponse([b"data"], 5)
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "assets.zip"
            with mock.patch.object(init.urllib.request, "urlopen", return_value=response):
                with self.assertRaisesRegex(RuntimeError, "declares 5 bytes; expected 4"):
                    init.download(destination, 4, hashlib.sha256(b"data").hexdigest())
            self.assertEqual(response.read_calls, 0)
            self.assertFalse(destination.exists())

    def test_archive_member_validation_rejects_traversal(self):
        with tempfile.TemporaryDirectory() as directory:
            archive_path = Path(directory) / "unsafe.zip"
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("../lod0.fbx", b"unsafe")
            with zipfile.ZipFile(archive_path) as archive:
                with self.assertRaisesRegex(RuntimeError, "unsafe path"):
                    init.archive_members(archive)

    def test_extract_installs_only_required_authoring_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive_path = root / "assets.zip"
            with zipfile.ZipFile(archive_path, "w") as archive:
                for name in init.CORE_FILES:
                    archive.writestr(f"assets/{name}", name.encode())
                archive.writestr("assets/mhr_model.pt", b"unused torch model")
            destination = root / "installed" / "assets"
            init.extract_archive(archive_path, destination)
            self.assertTrue(init.installed(destination))
            self.assertFalse((destination / "mhr_model.pt").exists())
            self.assertEqual(
                {path.name for path in destination.iterdir()},
                {*init.CORE_FILES, init.MANIFEST_NAME},
            )

    def test_manifest_records_official_release_and_installed_files(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            for name in init.CORE_FILES:
                (destination / name).write_bytes(name.encode())
            init.write_manifest(destination, init.CORE_FILES)
            record = json.loads((destination / init.MANIFEST_NAME).read_text())
            self.assertEqual(record["version"], "1.0.1")
            self.assertEqual(record["license"], "Apache-2.0")
            self.assertEqual(record["size_bytes"], 198_943_157)
            self.assertTrue(init.installed(destination))

    def test_lod1_corrective_profile_is_bounded(self):
        files = init.selected_files(lod1_correctives=True)
        self.assertIn("corrective_activation.npz", files)
        self.assertIn("corrective_blendshapes_lod1.npz", files)
        self.assertNotIn("corrective_blendshapes_lod0.npz", files)


if __name__ == "__main__":
    unittest.main()
