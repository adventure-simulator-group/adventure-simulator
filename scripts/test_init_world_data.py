from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("init_world_data", Path(__file__).with_name("init_world_data.py"))
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class Response:
    def __init__(self, status: int, content: bytes):
        self.status = status
        self.headers = {"Content-Length": str(len(content))}
        self.content = content

    def read(self, _: int) -> bytes:
        content, self.content = self.content, b""
        return content

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return None


class WorldDataInitializerTests(unittest.TestCase):
    def release(self) -> dict[str, object]:
        return {
            "schema": 1,
            "profile": "full",
            "archive_url": f"https://{module.PUBLIC_HOST}/releases/world-data/release.zip",
            "archive_size": 5,
            "descriptor_url": f"https://{module.PUBLIC_HOST}/releases/world-data/release.release.json",
            "descriptor_sha256": "0" * 64,
        }

    def test_release_lock_requires_the_fixed_public_r2_url(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "release.json"
            release = self.release()
            path.write_text(json.dumps(release), encoding="utf-8")
            self.assertEqual(module.load_release_lock(path), release)
            release["archive_url"] = "https://example.com/releases/world-data/release.zip"
            path.write_text(json.dumps(release), encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "unsafe archive_url"):
                module.load_release_lock(path)

    def test_download_resumes_only_when_server_confirms_range(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "release.zip"
            partial = target.with_name(target.name + ".part")
            partial.write_bytes(b"ab")
            with patch("urllib.request.urlopen", return_value=Response(206, b"cde")) as open_url:
                module.download(f"https://{module.PUBLIC_HOST}/releases/world-data/release.zip", target, 5)
            self.assertEqual(target.read_bytes(), b"abcde")
            self.assertEqual(open_url.call_args.args[0].headers["Range"], "bytes=2-")

    def test_marker_is_exact_release_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / "marker.json"
            release = self.release()
            module.write_marker(marker, release)
            self.assertTrue(module.installed_marker_matches(marker, release))
            changed = {**release, "archive_size": 6}
            self.assertFalse(module.installed_marker_matches(marker, changed))


if __name__ == "__main__":
    unittest.main()
