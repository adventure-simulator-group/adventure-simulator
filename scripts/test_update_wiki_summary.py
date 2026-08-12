from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from scripts.update_wiki_summary import NavigationError, SUMMARY, load_and_render


class UpdateWikiSummaryTests(unittest.TestCase):
    def test_manifest_reproduces_committed_summary(self) -> None:
        self.assertEqual(SUMMARY.read_text(encoding="utf-8"), load_and_render())

    def test_duplicate_page_is_rejected(self) -> None:
        manifest = """
format_version = 1
excluded = []

[[item]]
kind = "page"
title = "Introduction"
path = "../README.md"

[[item]]
kind = "page"
title = "Introduction again"
path = "../README.md"
"""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "navigation.toml"
            path.write_text(manifest, encoding="utf-8")
            with self.assertRaisesRegex(NavigationError, "listed more than once"):
                load_and_render(path)

    def test_missing_page_is_rejected(self) -> None:
        manifest = """
format_version = 1
excluded = []

[[item]]
kind = "page"
title = "Missing"
path = "this-page-does-not-exist.md"
"""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "navigation.toml"
            path.write_text(manifest, encoding="utf-8")
            with self.assertRaisesRegex(NavigationError, "does not exist"):
                load_and_render(path)


if __name__ == "__main__":
    unittest.main()
