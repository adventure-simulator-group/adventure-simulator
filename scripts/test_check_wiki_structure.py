"""Tests for the active-wiki structural validator."""

from pathlib import Path
import tempfile
import textwrap
import unittest

from scripts.check_wiki_structure import validate_wiki


def _page(title: str, lead: str = "This is the lead.", extra: str = "") -> str:
    return (
        "---\n"
        "status: draft\n"
        "scope: Test scope\n"
        "content_type: reference\n"
        "---\n\n"
        f"# {title}\n\n"
        f"{lead}\n\n"
        f"{extra}\n"
        "## Details\n\n"
        "More detail.\n"
    )


class WikiStructureTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.repository = Path(self.temporary.name)
        self.wiki = self.repository / "wiki"
        (self.wiki / "section").mkdir(parents=True)
        (self.repository / "scripts").mkdir()
        (self.repository / "scripts" / "example.py").write_text(
            "def format_markdown(text):\n    return text\n", encoding="utf-8"
        )
        (self.repository / "LICENSE").write_text("Test license\n", encoding="utf-8")

        (self.wiki / "index.md").write_text(
            _page(
                "Home",
                extra="[Section](section/index.md#section)\n\n"
                "[License](../LICENSE)",
            ),
            encoding="utf-8",
        )
        (self.wiki / "section" / "index.md").write_text(
            _page("Section", extra="[Detail](detail.md)"), encoding="utf-8"
        )
        (self.wiki / "section" / "detail.md").write_text(
            _page(
                "Detail",
                extra="<!-- code-anchor: scripts/example.py :: def format_markdown -->",
            ),
            encoding="utf-8",
        )
        self._write_navigation(
            ["index.md", "section/index.md", "section/detail.md"]
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _write_navigation(
        self, paths: list[str], *, exclusions: str = "[]"
    ) -> None:
        lines = ["format_version = 1", f"excluded = {exclusions}", ""]
        for path in paths:
            lines.extend(
                (
                    "[[item]]",
                    'kind = "page"',
                    f'title = "{path}"',
                    f'path = "{path}"',
                    "",
                )
            )
        (self.wiki / "navigation.toml").write_text(
            "\n".join(lines), encoding="utf-8"
        )

    def _messages(self) -> list[str]:
        return [str(diagnostic) for diagnostic in validate_wiki(self.repository)]

    def test_valid_active_wiki_passes(self) -> None:
        self.assertEqual(self._messages(), [])

    def test_reports_heading_metadata_and_lead_failures(self) -> None:
        (self.wiki / "section" / "detail.md").write_text(
            textwrap.dedent(
                """\
                ---
                status:
                scope:
                content_type:
                ---

                # First title
                # Second title

                <!-- Comments and anchors are not lead prose. -->

                ## Details

                Text.
                """
            ),
            encoding="utf-8",
        )

        messages = self._messages()
        self.assertTrue(any("expected exactly one H1, found 2" in item for item in messages))
        self.assertTrue(any("'status' metadata must not be empty" in item for item in messages))
        self.assertTrue(any("'scope' metadata must not be empty" in item for item in messages))
        self.assertTrue(any("'content_type' metadata must not be empty" in item for item in messages))
        self.assertTrue(any("missing non-empty lead prose" in item for item in messages))

    def test_rejects_duplicate_or_visible_metadata(self) -> None:
        (self.wiki / "section" / "detail.md").write_text(
            "---\n"
            "status: draft\n"
            "status: duplicate\n"
            "scope: Test scope\n"
            "content_type: reference\n"
            "---\n\n"
            "# Detail\n\n"
            "This is the lead.\n\n"
            "## Details\n\nText.\n",
            encoding="utf-8",
        )
        self.assertTrue(
            any("appears more than once" in item for item in self._messages())
        )

        (self.wiki / "section" / "detail.md").write_text(
            "# Detail\n\n"
            "> **Status:** Draft\n>\n"
            "> **Scope:** Test scope\n>\n"
            "> **Content type:** Reference\n\n"
            "This is the lead.\n\n"
            "## Details\n\nText.\n",
            encoding="utf-8",
        )
        messages = self._messages()
        self.assertTrue(any("'status' page metadata" in item for item in messages))
        self.assertTrue(any("'scope' page metadata" in item for item in messages))
        self.assertTrue(any("'content_type' page metadata" in item for item in messages))

    def test_missing_h1_does_not_hide_missing_lead(self) -> None:
        (self.wiki / "section" / "detail.md").write_text(
            "---\n"
            "status: draft\n"
            "scope: Test scope\n"
            "content_type: reference\n"
            "---\n\n"
            "## Details\n\nText.\n"
            "<!-- code-anchor: scripts/example.py :: def format_markdown -->\n",
            encoding="utf-8",
        )

        messages = self._messages()
        self.assertTrue(any("expected exactly one H1, found 0" in item for item in messages))
        self.assertTrue(any("missing non-empty lead prose" in item for item in messages))

    def test_tables_and_indented_code_do_not_count_as_lead_prose(self) -> None:
        (self.wiki / "section" / "detail.md").write_text(
            "---\n"
            "status: draft\n"
            "scope: Test scope\n"
            "content_type: reference\n"
            "---\n\n"
            "# Detail\n\n"
            "Name | Value\n"
            "--- | ---\n"
            "example | result\n\n"
            "    print('not prose')\n\n"
            "## Details\n\n"
            "Text.\n\n"
            "<!-- code-anchor: scripts/example.py :: def format_markdown -->\n",
            encoding="utf-8",
        )

        self.assertTrue(
            any("missing non-empty lead prose" in item for item in self._messages())
        )

    def test_requires_index_in_every_active_page_directory(self) -> None:
        (self.wiki / "section" / "index.md").unlink()
        self._write_navigation(["index.md", "section/detail.md"])

        self.assertTrue(
            any(
                "wiki/section/index.md: every active wiki directory" in item
                for item in self._messages()
            )
        )

    def test_navigation_has_no_exclusions_duplicates_omissions_or_ghosts(self) -> None:
        self._write_navigation(
            ["index.md", "section/index.md", "section/index.md", "ghost.md"],
            exclusions='["section/detail.md"]',
        )

        messages = self._messages()
        self.assertTrue(any("navigation exclusions must be the empty array" in item for item in messages))
        self.assertTrue(any("is listed more than once" in item for item in messages))
        self.assertTrue(any("has no active page: 'ghost.md'" in item for item in messages))
        self.assertTrue(any("wiki/section/detail.md' is missing from navigation" in item for item in messages))

    def test_reports_missing_link_targets_and_fragments(self) -> None:
        (self.wiki / "index.md").write_text(
            _page(
                "Home",
                extra="[Missing](missing.md)\n\n"
                "[Missing fragment](section/index.md#not-a-heading)\n\n"
                "[External](https://example.com/missing)\n\n"
                "`[Code example](also-missing.md)`\n\n"
                "```markdown\n[Fenced example](also-missing.md)\n```",
            ),
            encoding="utf-8",
        )

        messages = self._messages()
        self.assertTrue(any("target does not exist: 'missing.md'" in item for item in messages))
        self.assertTrue(any("fragment does not exist" in item for item in messages))
        self.assertFalse(any("also-missing.md" in item for item in messages))

    def test_footnote_definitions_are_not_reference_links(self) -> None:
        (self.wiki / "index.md").write_text(
            _page(
                "Home",
                extra="A sentence with a footnote.[^1]\n\n"
                "[^1]: It may include an [external link](https://example.com).",
            ),
            encoding="utf-8",
        )

        self.assertEqual(self._messages(), [])

    def test_code_anchors_must_be_exact_and_resolve_symbol_text(self) -> None:
        (self.wiki / "section" / "detail.md").write_text(
            _page(
                "Detail",
                extra="<!-- code-anchor: scripts/example.py: def format_markdown -->\n\n"
                "<!-- code-anchor: scripts/example.py :: absent_symbol -->\n\n"
                "<!-- code-anchor: scripts/missing.py :: symbol -->",
            ),
            encoding="utf-8",
        )

        messages = self._messages()
        self.assertTrue(any("malformed code anchor" in item for item in messages))
        self.assertTrue(any("symbol text was not found" in item for item in messages))
        self.assertTrue(any("code anchor target does not exist" in item for item in messages))
        self.assertTrue(any("no valid code anchor found" in item for item in messages))

    def test_code_anchor_example_in_fenced_code_does_not_satisfy_requirement(self) -> None:
        (self.wiki / "section" / "detail.md").write_text(
            _page(
                "Detail",
                extra="```markdown\n"
                "<!-- code-anchor: scripts/example.py :: def format_markdown -->\n"
                "```",
            ),
            encoding="utf-8",
        )

        self.assertTrue(
            any("no valid code anchor found" in item for item in self._messages())
        )

    def test_generated_summary_and_assets_are_not_active_pages(self) -> None:
        (self.wiki / "SUMMARY.md").write_text("Not a valid active page.\n", encoding="utf-8")
        (self.wiki / "assets").mkdir()
        (self.wiki / "assets" / "notes.md").write_text(
            "Not a valid active page.\n", encoding="utf-8"
        )

        self.assertEqual(self._messages(), [])


if __name__ == "__main__":
    unittest.main()
