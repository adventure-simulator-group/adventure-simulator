#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
SPEC = importlib.util.spec_from_file_location("init_world_runtime", SCRIPT_DIR / "init_world_runtime.py")
assert SPEC and SPEC.loader
initializer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(initializer)


class Response:
    def __init__(self, payload: bytes, status: int = 200):
        self.payload = payload
        self.status = status
        self.headers = {"Content-Length": str(len(payload))}
        self.offset = 0

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def read(self, size: int) -> bytes:
        block = self.payload[self.offset:self.offset + size]
        self.offset += len(block)
        return block


class RuntimeInitializerTests(unittest.TestCase):
    def test_download_is_atomic_and_size_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "runtime.zip"
            with mock.patch.object(initializer.urllib.request, "urlopen", return_value=Response(b"archive")):
                initializer.download("https://example.com/runtime.zip", destination, 7)
            self.assertEqual(destination.read_bytes(), b"archive")
            self.assertFalse(destination.with_name("runtime.zip.part").exists())

    def test_download_rejects_wrong_response_size(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "runtime.zip"
            with mock.patch.object(initializer.urllib.request, "urlopen", return_value=Response(b"short")):
                with self.assertRaisesRegex(RuntimeError, "unexpected size"):
                    initializer.download("https://example.com/runtime.zip", destination, 8)
            self.assertFalse(destination.exists())

    def test_previous_downloaded_release_is_replaced_automatically(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            marker = repository / "target/world-runtime-release.lock.json"
            marker.parent.mkdir(parents=True)
            marker.write_text("{}", encoding="utf-8")
            current = {
                "archive_sha256": "a" * 64,
                "archive_url": f"https://{initializer.runtime.PUBLIC_HOST}{initializer.runtime.PUBLIC_PREFIX}runtime.zip",
                "archive_size": 7,
                "release": "current",
            }
            previous = {"release": "previous"}
            cache = repository / "target/world-runtime-cache" / ("a" * 64) / "runtime.zip"
            cache.parent.mkdir(parents=True)
            cache.write_bytes(b"archive")
            with mock.patch.object(initializer.runtime, "read_lock", side_effect=[current, previous]), \
                 mock.patch.object(initializer.runtime, "installed_files_match", side_effect=[False, True]), \
                 mock.patch.object(initializer.runtime, "sha256", return_value="a" * 64), \
                 mock.patch.object(initializer.runtime, "install") as install, \
                 mock.patch.object(initializer, "write_marker"):
                initializer.initialize(repository, repository / "lock.json")
            install.assert_called_once_with(cache, current, repository, True)


if __name__ == "__main__":
    unittest.main()
