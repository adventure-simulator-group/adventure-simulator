#!/usr/bin/env python3
"""Parse and hide page metadata front matter during mdBook builds."""

from __future__ import annotations

from dataclasses import dataclass
import json
import re
import sys
from typing import Any


FIELD_RE = re.compile(r"^(?P<key>[a-z][a-z0-9_]*)\s*:\s*(?P<value>.*)$")


class FrontMatterError(ValueError):
    """Invalid page metadata with its source line."""

    def __init__(self, message: str, line: int = 1) -> None:
        super().__init__(message)
        self.line = line


@dataclass(frozen=True)
class PageFrontMatter:
    """Parsed scalar metadata and the Markdown remaining after it."""

    metadata: dict[str, str]
    lines: dict[str, int]
    body: str
    end_line: int


def _parse_scalar(raw: str, line: int) -> str:
    value = raw.strip()
    if not value:
        return ""
    if value.startswith('"'):
        try:
            decoded = json.loads(value)
        except json.JSONDecodeError as error:
            raise FrontMatterError(f"invalid quoted value: {error.msg}", line) from error
        if not isinstance(decoded, str):
            raise FrontMatterError("metadata values must be strings", line)
        return decoded
    if value.startswith("'"):
        if len(value) < 2 or not value.endswith("'"):
            raise FrontMatterError("unterminated single-quoted value", line)
        return value[1:-1].replace("''", "'")
    return value


def split_page_front_matter(text: str) -> PageFrontMatter | None:
    """Split a leading, scalar YAML front-matter block from Markdown."""

    source_lines = text.splitlines(keepends=True)
    if not source_lines or source_lines[0].strip() != "---":
        return None

    closing_index = next(
        (
            index
            for index, line in enumerate(source_lines[1:], start=1)
            if line.strip() == "---"
        ),
        None,
    )
    if closing_index is None:
        raise FrontMatterError("metadata front matter is missing its closing '---'")

    metadata: dict[str, str] = {}
    metadata_lines: dict[str, int] = {}
    for index, raw_line in enumerate(source_lines[1:closing_index], start=2):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        match = FIELD_RE.match(line)
        if not match:
            raise FrontMatterError("expected a 'field: value' metadata entry", index)
        key = match.group("key")
        if key in metadata:
            raise FrontMatterError(f"metadata field {key!r} appears more than once", index)
        metadata[key] = _parse_scalar(match.group("value"), index)
        metadata_lines[key] = index

    return PageFrontMatter(
        metadata=metadata,
        lines=metadata_lines,
        body="".join(source_lines[closing_index + 1 :]),
        end_line=closing_index + 1,
    )


def _strip_chapter_metadata(items: list[dict[str, Any]]) -> None:
    for item in items:
        chapter = item.get("Chapter")
        if not isinstance(chapter, dict):
            continue
        front_matter = split_page_front_matter(chapter.get("content", ""))
        if front_matter is not None:
            chapter["content"] = front_matter.body
        sub_items = chapter.get("sub_items", [])
        if isinstance(sub_items, list):
            _strip_chapter_metadata(sub_items)


def preprocess_book(book: dict[str, Any]) -> dict[str, Any]:
    """Remove metadata front matter from every chapter before rendering."""

    sections = book.get("sections", [])
    if isinstance(sections, list):
        _strip_chapter_metadata(sections)
    return book


def main() -> int:
    if len(sys.argv) > 1:
        if sys.argv[1] == "supports":
            return 0
        raise SystemExit(f"unsupported argument: {sys.argv[1]}")

    _, book = json.load(sys.stdin)
    json.dump(preprocess_book(book), sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
