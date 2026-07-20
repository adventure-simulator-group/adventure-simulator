import hashlib
import io
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import init_viabundus


class Response(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


class InitViabundusTests(unittest.TestCase):
    def test_rejects_broad_or_misnamed_force_destinations(self):
        with self.assertRaises(RuntimeError):
            init_viabundus.validate_destination(Path(__file__).resolve().parents[1])
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(RuntimeError):
                init_viabundus.validate_destination(Path(temporary) / "not-viabundus")

    def test_download_requires_matching_upstream_size_and_checksum(self):
        payload = b"trusted fixture"
        metadata = {
            "key": "nodes.csv",
            "size": len(payload),
            "checksum": f"md5:{hashlib.md5(payload).hexdigest()}",
            "links": {"content": "https://example.invalid/nodes.csv"},
        }
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "nodes.csv"
            with mock.patch.object(init_viabundus, "request", return_value=Response(payload)):
                result = init_viabundus.download_file(metadata, destination)
            self.assertEqual(result["size"], len(payload))

            metadata["size"] = len(payload) + 1
            with mock.patch.object(init_viabundus, "request", return_value=Response(payload)):
                with self.assertRaises(RuntimeError):
                    init_viabundus.download_file(metadata, destination)
            self.assertFalse(destination.exists())

    def test_publish_replacement_swaps_complete_directories(self):
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary)
            destination = parent / "viabundus"
            replacement = parent / "replacement"
            destination.mkdir()
            replacement.mkdir()
            (destination / "old").write_text("old", encoding="utf-8")
            (replacement / "new").write_text("new", encoding="utf-8")
            init_viabundus.publish_replacement(replacement, destination)
            self.assertTrue((destination / "new").is_file())
            self.assertFalse((destination / "old").exists())
            self.assertFalse((parent / ".viabundus.previous").exists())


if __name__ == "__main__":
    unittest.main()
