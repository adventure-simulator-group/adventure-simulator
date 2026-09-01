from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

from scripts.format_wiki_markdown import (
    _fenced_code_lines,
    _line_exceeds_width,
    format_markdown,
)


class FormatWikiMarkdownTests(unittest.TestCase):
    def test_wraps_plain_prose_without_changing_tokens(self) -> None:
        source = (
            "This paragraph is deliberately long enough that it needs to wrap "
            "while preserving every one of its words exactly.\n"
        )
        rendered = format_markdown(source, width=52)
        self.assertEqual(source.split(), rendered.split())
        self.assertTrue(all(len(line) <= 52 for line in rendered.splitlines()))

    def test_keeps_links_together(self) -> None:
        source = (
            "Read [the deliberately long reference title]"
            "(https://example.com/a/very/long/reference) before proceeding.\n"
        )
        rendered = format_markdown(source, width=40)
        self.assertIn(
            "[the deliberately long reference title]"
            "(https://example.com/a/very/long/reference)",
            rendered,
        )

    def test_wraps_long_prose_around_short_link(self) -> None:
        source = (
            "Read [this](https://example.com) because the surrounding prose is "
            "deliberately much too long for this narrow test width.\n"
        )
        rendered = format_markdown(source, width=48)
        self.assertEqual(source.split(), rendered.split())
        self.assertGreater(len(rendered.splitlines()), 1)
        self.assertTrue(all(len(line) <= 48 for line in rendered.splitlines()))

    def test_preserves_intentionally_wrapped_prose(self) -> None:
        source = "This paragraph is already wrapped\nwithin the requested width.\n"
        self.assertEqual(source, format_markdown(source, width=40))

    def test_wraps_lists_but_preserves_code_and_comments(self) -> None:
        source = (
            "*   This authored list line remains untouched even when it is long.\n"
            "    Its authored continuation indentation also remains untouched.\n"
            "\n"
            "```rust\n"
            "let long_identifier = \"this is not prose wrapping\";\n"
            "```\n"
            "\n"
            "<!-- a deliberately long comment that remains untouched -->\n"
        )
        rendered = format_markdown(source, width=30)
        self.assertIn(
            "*   This authored list line\n"
            "    remains untouched even\n"
            "    when it is long.\n",
            rendered,
        )
        self.assertIn(
            "```rust\n"
            "let long_identifier = \"this is not prose wrapping\";\n"
            "```\n",
            rendered,
        )
        self.assertIn(
            "<!-- a deliberately long comment that remains untouched -->\n",
            rendered,
        )

    def test_line_policy_allows_long_link_atom(self) -> None:
        line = "Read [reference](https://example.com/" + ("long/" * 20) + ")"
        self.assertFalse(_line_exceeds_width(line, 80))

    def test_line_policy_rejects_long_prose(self) -> None:
        self.assertTrue(_line_exceeds_width("ordinary " * 12, 80))

    def test_fenced_code_lines_are_identified(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sample.md"
            path.write_text(
                "Before\n```text\na very long literal line\n```\nAfter\n",
                encoding="utf-8",
            )
            self.assertEqual({2, 3, 4}, _fenced_code_lines(path))


if __name__ == "__main__":
    unittest.main()
