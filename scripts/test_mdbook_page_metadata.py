"""Tests for invisible mdBook page metadata."""

import unittest

from scripts.mdbook_page_metadata import (
    FrontMatterError,
    preprocess_book,
    split_page_front_matter,
)


class PageMetadataTests(unittest.TestCase):
    def test_parses_scalar_yaml_front_matter(self) -> None:
        source = (
            "---\n"
            "status: draft\n"
            'scope: "A question: answered"\n'
            "content_type: overview\n"
            "---\n\n"
            "# Page\n"
        )

        result = split_page_front_matter(source)

        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(
            result.metadata,
            {
                "status": "draft",
                "scope": "A question: answered",
                "content_type": "overview",
            },
        )
        self.assertEqual(result.lines["scope"], 3)
        self.assertEqual(result.end_line, 5)
        self.assertEqual(result.body, "\n# Page\n")

    def test_rejects_duplicate_or_unclosed_metadata(self) -> None:
        with self.assertRaisesRegex(FrontMatterError, "appears more than once"):
            split_page_front_matter(
                "---\nstatus: draft\nstatus: canonical\n---\n# Page\n"
            )
        with self.assertRaisesRegex(FrontMatterError, "missing its closing"):
            split_page_front_matter("---\nstatus: draft\n# Page\n")

    def test_preprocessor_strips_nested_chapter_metadata(self) -> None:
        content = "---\nstatus: draft\n---\n\n# Page\n"
        book = {
            "sections": [
                {
                    "Chapter": {
                        "content": content,
                        "sub_items": [
                            {"Chapter": {"content": content, "sub_items": []}}
                        ],
                    }
                }
            ]
        }

        rendered = preprocess_book(book)

        chapter = rendered["sections"][0]["Chapter"]
        self.assertEqual(chapter["content"], "\n# Page\n")
        self.assertEqual(
            chapter["sub_items"][0]["Chapter"]["content"], "\n# Page\n"
        )


if __name__ == "__main__":
    unittest.main()
