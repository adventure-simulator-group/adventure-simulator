#!/usr/bin/env python3
"""Validate the structure and local references of the active Fabelgeist wiki."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import html
from pathlib import Path
import re
import tomllib
from urllib.parse import unquote, urlsplit

try:
    from scripts.mdbook_page_metadata import (
        FrontMatterError,
        split_page_front_matter,
    )
except ModuleNotFoundError:
    from mdbook_page_metadata import FrontMatterError, split_page_front_matter


REPOSITORY = Path(__file__).resolve().parent.parent
WIKI = REPOSITORY / "wiki"
MANIFEST = WIKI / "navigation.toml"
SUMMARY = WIKI / "SUMMARY.md"

METADATA_FIELDS = ("status", "scope", "content_type")
HEADING_RE = re.compile(r"^\s{0,3}(#{1,6})(?:[ \t]+|$)(.*)$")
FENCE_RE = re.compile(r"^\s{0,3}(`{3,}|~{3,})(.*)$")
REFERENCE_LINK_RE = re.compile(
    r"^\s{0,3}\[([^\]]+)\]:\s*(?:<([^>]+)>|([^\s]+))"
)
CODE_ANCHOR_RE = re.compile(
    r"^\s*<!-- code-anchor: ([^\r\n]+?) :: ([^\r\n]+?) -->\s*$"
)


@dataclass(frozen=True)
class Diagnostic:
    """One deterministic, user-facing validation failure."""

    path: str
    line: int
    message: str

    def __str__(self) -> str:
        location = f"{self.path}:{self.line}" if self.line else self.path
        return f"{location}: {self.message}"


def _display_path(repository: Path, path: Path) -> str:
    try:
        return path.relative_to(repository).as_posix()
    except ValueError:
        return str(path)


def _diagnostic(
    repository: Path, path: Path, message: str, line: int = 0
) -> Diagnostic:
    return Diagnostic(_display_path(repository, path), line, message)


def _active_pages(wiki: Path) -> list[Path]:
    if not wiki.is_dir():
        return []
    pages = []
    for path in wiki.rglob("*.md"):
        relative = path.relative_to(wiki)
        if relative.as_posix() == "SUMMARY.md" or "assets" in relative.parts:
            continue
        if path.is_file():
            pages.append(path)
    return sorted(pages, key=lambda path: path.relative_to(wiki).as_posix())


def _visible_markdown_lines(text: str) -> list[tuple[int, str]]:
    """Return lines outside fenced code and HTML comments, preserving numbers."""

    visible: list[tuple[int, str]] = []
    fence_character: str | None = None
    fence_length = 0
    in_comment = False

    for line_number, raw_line in enumerate(text.splitlines(), start=1):
        if fence_character is not None:
            closing = re.match(
                rf"^\s{{0,3}}{re.escape(fence_character)}{{{fence_length},}}\s*$",
                raw_line,
            )
            if closing:
                fence_character = None
                fence_length = 0
            continue

        fence = FENCE_RE.match(raw_line)
        if fence:
            marker = fence.group(1)
            fence_character = marker[0]
            fence_length = len(marker)
            continue

        remaining = raw_line
        pieces: list[str] = []
        while remaining:
            if in_comment:
                comment_end = remaining.find("-->")
                if comment_end < 0:
                    remaining = ""
                    break
                remaining = remaining[comment_end + 3 :]
                in_comment = False
                continue

            comment_start = remaining.find("<!--")
            if comment_start < 0:
                pieces.append(remaining)
                break
            pieces.append(remaining[:comment_start])
            remaining = remaining[comment_start + 4 :]
            in_comment = True

        visible.append((line_number, "".join(pieces)))

    return visible


def _headings(lines: list[tuple[int, str]]) -> list[tuple[int, int, str]]:
    headings: list[tuple[int, int, str]] = []
    for line_number, line in lines:
        match = HEADING_RE.match(line)
        if not match:
            continue
        title = re.sub(r"[ \t]+#+[ \t]*$", "", match.group(2)).strip()
        headings.append((line_number, len(match.group(1)), title))
    return headings


def _has_lead_prose(
    lines: list[tuple[int, str]], h1_line: int, first_h2_line: int
) -> bool:
    for line_number, line in lines:
        if line_number <= h1_line or line_number >= first_h2_line:
            continue
        stripped = line.strip()
        if not stripped or HEADING_RE.match(line):
            continue
        if stripped.startswith(">"):
            continue
        if re.match(r"^(?: {4}|\t)", line):
            continue
        if "|" in stripped:
            continue
        if re.match(r"^(?:[-+*]|\d+[.)])\s+", stripped):
            continue
        if re.match(r"^(?:-{3,}|_{3,}|\*{3,})$", stripped):
            continue
        if re.match(r"^\[[^\]]+\]:", stripped):
            continue

        candidate = re.sub(r"!\[[^\]]*\]\([^)]*\)", "", stripped)
        candidate = re.sub(r"<[^>]+>", "", candidate)
        candidate = candidate.replace("`", "").replace("*", "").replace("_", "")
        if any(character.isalnum() for character in candidate):
            return True
    return False


def _validate_page_shape(
    repository: Path, page: Path, text: str
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    try:
        front_matter = split_page_front_matter(text)
    except FrontMatterError as error:
        diagnostics.append(
            _diagnostic(
                repository,
                page,
                f"invalid page metadata: {error}",
                error.line,
            )
        )
        front_matter = None

    visible_text = text
    if front_matter is not None:
        visible_text = ("\n" * front_matter.end_line) + front_matter.body

    lines = _visible_markdown_lines(visible_text)
    headings = _headings(lines)
    h1s = [heading for heading in headings if heading[1] == 1]

    if len(h1s) != 1:
        diagnostics.append(
            _diagnostic(
                repository,
                page,
                f"expected exactly one H1, found {len(h1s)}",
                h1s[1][0] if len(h1s) > 1 else 0,
            )
        )

    first_h2_line = next(
        (line_number for line_number, level, _ in headings if level == 2),
        len(text.splitlines()) + 1,
    )
    for field in METADATA_FIELDS:
        value = front_matter.metadata.get(field) if front_matter else None
        if value is None:
            diagnostics.append(
                _diagnostic(
                    repository,
                    page,
                    f"missing non-empty '{field}' page metadata",
                )
            )
            continue
        if not value.strip():
            diagnostics.append(
                _diagnostic(
                    repository,
                    page,
                    f"'{field}' metadata must not be empty",
                    front_matter.lines[field],
                )
            )

    h1_line = h1s[0][0] if h1s else 0
    if not _has_lead_prose(lines, h1_line, first_h2_line):
        diagnostics.append(
            _diagnostic(
                repository,
                page,
                "missing non-empty lead prose before the first H2",
            )
        )

    return diagnostics


def _validate_directory_indexes(
    repository: Path, wiki: Path, pages: list[Path]
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    page_set = set(pages)
    directories: set[Path] = {wiki}
    for page in pages:
        directory = page.parent
        while directory != wiki:
            directories.add(directory)
            directory = directory.parent

    for directory in sorted(directories, key=lambda path: path.relative_to(wiki).as_posix()):
        index = directory / "index.md"
        if index not in page_set:
            diagnostics.append(
                _diagnostic(
                    repository,
                    index,
                    "every active wiki directory containing pages must have an index.md",
                )
            )
    return diagnostics


def _manifest_page(
    repository: Path,
    wiki: Path,
    manifest: Path,
    path_text: object,
    item_index: int,
) -> tuple[Path | None, Diagnostic | None]:
    if not isinstance(path_text, str) or not path_text.strip():
        return None, _diagnostic(
            repository,
            manifest,
            f"navigation item {item_index} has no non-empty page path",
        )
    relative = Path(path_text)
    if relative.is_absolute() or relative.suffix.lower() != ".md":
        return None, _diagnostic(
            repository,
            manifest,
            f"navigation item {item_index} is not a relative Markdown path: {path_text!r}",
        )
    resolved = (wiki / relative).resolve()
    try:
        resolved.relative_to(wiki.resolve())
    except ValueError:
        return None, _diagnostic(
            repository,
            manifest,
            f"navigation item {item_index} escapes wiki/: {path_text!r}",
        )
    return resolved, None


def _validate_navigation(
    repository: Path,
    wiki: Path,
    manifest: Path,
    pages: list[Path],
) -> list[Diagnostic]:
    try:
        data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        return [_diagnostic(repository, manifest, f"cannot read navigation manifest: {error}")]

    diagnostics: list[Diagnostic] = []
    excluded = data.get("excluded", [])
    if excluded != []:
        diagnostics.append(
            _diagnostic(
                repository,
                manifest,
                "navigation exclusions must be the empty array; every active page belongs in navigation",
            )
        )

    items = data.get("item", [])
    if not isinstance(items, list):
        diagnostics.append(
            _diagnostic(repository, manifest, "navigation 'item' must be an array of tables")
        )
        items = []

    listed: dict[Path, list[int]] = {}
    active_set = {page.resolve() for page in pages}
    for item_index, item in enumerate(items, start=1):
        if not isinstance(item, dict) or item.get("kind") != "page":
            continue
        resolved, error = _manifest_page(
            repository, wiki, manifest, item.get("path"), item_index
        )
        if error:
            diagnostics.append(error)
            continue
        assert resolved is not None
        listed.setdefault(resolved, []).append(item_index)
        if resolved not in active_set:
            path_text = item.get("path")
            diagnostics.append(
                _diagnostic(
                    repository,
                    manifest,
                    f"navigation item {item_index} has no active page: {path_text!r}",
                )
            )

    for resolved, item_indexes in sorted(
        listed.items(), key=lambda entry: _display_path(repository, entry[0])
    ):
        if len(item_indexes) > 1:
            diagnostics.append(
                _diagnostic(
                    repository,
                    manifest,
                    f"active page {_display_path(repository, resolved)!r} is listed more than once "
                    f"(items {', '.join(str(index) for index in item_indexes)})",
                )
            )

    for page in pages:
        resolved = page.resolve()
        count = len(listed.get(resolved, []))
        if count == 0:
            diagnostics.append(
                _diagnostic(
                    repository,
                    manifest,
                    f"active page {_display_path(repository, page)!r} is missing from navigation",
                )
            )

    return diagnostics


def _strip_inline_code(line: str) -> str:
    return re.sub(r"(`+)(?:(?!\1).)*\1", "", line)


def _inline_link_destinations(line: str) -> list[tuple[int, str]]:
    """Extract inline Markdown link and image destinations from one line."""

    line = _strip_inline_code(line)
    destinations: list[tuple[int, str]] = []
    cursor = 0
    while True:
        opener = line.find("](", cursor)
        if opener < 0:
            break
        destination_start = opener + 2
        while destination_start < len(line) and line[destination_start].isspace():
            destination_start += 1
        if destination_start >= len(line):
            break

        if line[destination_start] == "<":
            end = line.find(">", destination_start + 1)
            if end >= 0:
                destinations.append(
                    (destination_start + 1, line[destination_start + 1 : end])
                )
                cursor = end + 1
                continue

        nested_parentheses = 0
        escaped = False
        end = destination_start
        while end < len(line):
            character = line[end]
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == "(":
                nested_parentheses += 1
            elif character == ")":
                if nested_parentheses == 0:
                    break
                nested_parentheses -= 1
            elif character.isspace() and nested_parentheses == 0:
                break
            end += 1
        if end > destination_start:
            destination = re.sub(r"\\(.)", r"\1", line[destination_start:end])
            destinations.append((destination_start + 1, destination))
        cursor = max(end + 1, opener + 2)
    return destinations


def _heading_slug(title: str) -> str:
    title = html.unescape(title)
    title = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", title)
    title = re.sub(r"<[^>]+>", "", title)
    title = re.sub(r"[`*_~]", "", title).lower()
    title = re.sub(r"[^\w\s-]", "", title, flags=re.UNICODE)
    return re.sub(r"\s+", "-", title.strip())


def _markdown_anchors(text: str) -> set[str]:
    anchors: set[str] = set()
    occurrences: dict[str, int] = {}
    for _, _, title in _headings(_visible_markdown_lines(text)):
        base = _heading_slug(title)
        duplicate = occurrences.get(base, 0)
        anchor = base if duplicate == 0 else f"{base}-{duplicate}"
        occurrences[base] = duplicate + 1
        anchors.add(anchor)
    return anchors


def _validate_link(
    repository: Path,
    source: Path,
    line_number: int,
    destination: str,
) -> Diagnostic | None:
    parsed = urlsplit(destination)
    if parsed.scheme or parsed.netloc:
        return None

    decoded_path = unquote(parsed.path)
    fragment = unquote(parsed.fragment)
    if not decoded_path:
        target = source
    else:
        path = Path(decoded_path)
        if path.is_absolute():
            return _diagnostic(
                repository,
                source,
                f"local Markdown link must be repository-relative: {destination!r}",
                line_number,
            )
        target = (source.parent / path).resolve()
        try:
            target.relative_to(repository.resolve())
        except ValueError:
            return _diagnostic(
                repository,
                source,
                f"local Markdown link escapes the repository: {destination!r}",
                line_number,
            )

    if not target.exists():
        return _diagnostic(
            repository,
            source,
            f"local Markdown link target does not exist: {destination!r}",
            line_number,
        )
    if fragment and target.is_file() and target.suffix.lower() == ".md":
        try:
            anchors = _markdown_anchors(target.read_text(encoding="utf-8"))
        except (OSError, UnicodeError) as error:
            return _diagnostic(
                repository,
                source,
                f"cannot inspect local Markdown link target {destination!r}: {error}",
                line_number,
            )
        if fragment not in anchors:
            return _diagnostic(
                repository,
                source,
                f"local Markdown link fragment does not exist: {destination!r}",
                line_number,
            )
    return None


def _validate_local_links(
    repository: Path, pages: list[Path], page_text: dict[Path, str]
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    for page in pages:
        for line_number, line in _visible_markdown_lines(page_text[page]):
            destinations = _inline_link_destinations(line)
            reference = REFERENCE_LINK_RE.match(line)
            if reference and not reference.group(1).startswith("^"):
                destination = reference.group(2) or reference.group(3)
                destinations.append((reference.start() + 1, destination))
            for _, destination in destinations:
                error = _validate_link(repository, page, line_number, destination)
                if error:
                    diagnostics.append(error)
    return diagnostics


def _validate_code_anchors(
    repository: Path,
    wiki: Path,
    pages: list[Path],
    page_text: dict[Path, str],
) -> list[Diagnostic]:
    diagnostics: list[Diagnostic] = []
    valid_anchors = 0
    target_cache: dict[Path, str | None] = {}

    for page in pages:
        fence_character: str | None = None
        fence_length = 0
        for line_number, line in enumerate(page_text[page].splitlines(), start=1):
            if fence_character is not None:
                closing = re.match(
                    rf"^\s{{0,3}}{re.escape(fence_character)}{{{fence_length},}}\s*$",
                    line,
                )
                if closing:
                    fence_character = None
                    fence_length = 0
                continue
            fence = FENCE_RE.match(line)
            if fence:
                marker = fence.group(1)
                fence_character = marker[0]
                fence_length = len(marker)
                continue
            if "<!-- code-anchor:" not in line:
                continue
            match = CODE_ANCHOR_RE.match(line)
            if not match:
                diagnostics.append(
                    _diagnostic(
                        repository,
                        page,
                        "malformed code anchor; expected '<!-- code-anchor: relative/path :: symbol text -->'",
                        line_number,
                    )
                )
                continue

            path_text, symbol = (part.strip() for part in match.groups())
            path = Path(path_text)
            if not path_text or path.is_absolute():
                diagnostics.append(
                    _diagnostic(
                        repository,
                        page,
                        f"code anchor path must be repository-relative: {path_text!r}",
                        line_number,
                    )
                )
                continue
            if not symbol:
                diagnostics.append(
                    _diagnostic(
                        repository,
                        page,
                        "code anchor symbol text must not be empty",
                        line_number,
                    )
                )
                continue

            target = (repository / path).resolve()
            try:
                target.relative_to(repository.resolve())
            except ValueError:
                diagnostics.append(
                    _diagnostic(
                        repository,
                        page,
                        f"code anchor escapes the repository: {path_text!r}",
                        line_number,
                    )
                )
                continue
            if not target.is_file():
                diagnostics.append(
                    _diagnostic(
                        repository,
                        page,
                        f"code anchor target does not exist: {path_text!r}",
                        line_number,
                    )
                )
                continue

            if target not in target_cache:
                try:
                    target_cache[target] = target.read_text(encoding="utf-8")
                except (OSError, UnicodeError):
                    target_cache[target] = None
            target_text = target_cache[target]
            if target_text is None:
                diagnostics.append(
                    _diagnostic(
                        repository,
                        page,
                        f"code anchor target is not readable UTF-8 text: {path_text!r}",
                        line_number,
                    )
                )
                continue
            if symbol not in target_text:
                diagnostics.append(
                    _diagnostic(
                        repository,
                        page,
                        f"code anchor symbol text was not found in {path_text!r}: {symbol!r}",
                        line_number,
                    )
                )
                continue
            valid_anchors += 1

    if valid_anchors == 0:
        diagnostics.append(
            _diagnostic(
                repository,
                wiki,
                "no valid code anchor found; at least one active page must contain a resolvable code anchor",
            )
        )
    return diagnostics


def validate_wiki(
    repository: Path = REPOSITORY,
    wiki: Path | None = None,
    manifest: Path | None = None,
) -> list[Diagnostic]:
    """Return all active-wiki structural failures in deterministic order."""

    repository = repository.resolve()
    wiki = (wiki or repository / "wiki").resolve()
    manifest = (manifest or wiki / "navigation.toml").resolve()
    pages = _active_pages(wiki)
    diagnostics: list[Diagnostic] = []

    if not wiki.is_dir():
        return [_diagnostic(repository, wiki, "active wiki directory does not exist")]

    page_text: dict[Path, str] = {}
    readable_pages: list[Path] = []
    for page in pages:
        try:
            page_text[page] = page.read_text(encoding="utf-8")
            readable_pages.append(page)
        except (OSError, UnicodeError) as error:
            diagnostics.append(
                _diagnostic(repository, page, f"cannot read active wiki page: {error}")
            )

    for page in readable_pages:
        diagnostics.extend(_validate_page_shape(repository, page, page_text[page]))
    diagnostics.extend(_validate_directory_indexes(repository, wiki, pages))
    diagnostics.extend(_validate_navigation(repository, wiki, manifest, pages))
    diagnostics.extend(_validate_local_links(repository, readable_pages, page_text))
    diagnostics.extend(
        _validate_code_anchors(repository, wiki, readable_pages, page_text)
    )

    return sorted(
        diagnostics,
        key=lambda diagnostic: (
            diagnostic.path,
            diagnostic.line,
            diagnostic.message,
        ),
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repository",
        type=Path,
        default=REPOSITORY,
        help="repository root to validate (defaults to this script's repository)",
    )
    arguments = parser.parse_args()
    diagnostics = validate_wiki(arguments.repository)
    if diagnostics:
        for diagnostic in diagnostics:
            print(diagnostic)
        print(f"Wiki structure check failed with {len(diagnostics)} error(s).")
        return 1
    print("Wiki structure check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
