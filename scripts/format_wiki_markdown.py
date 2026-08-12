#!/usr/bin/env python3
"""Wrap ordinary Markdown prose without canonicalizing authored markup."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import textwrap


REPOSITORY = Path(__file__).resolve().parent.parent
DEFAULT_FILES = (REPOSITORY / "README.md", REPOSITORY / "AGENTS.md")
SPECIAL_LINE = re.compile(
    r"^(?:\s*$|\s{0,3}(?:#{1,6}\s|>|```|~~~|[-*_](?:\s*[-*_]){2,}\s*$|"
    r"[-+*]\s|\d+[.)]\s|\[[^]]+\]:|<[/!A-Za-z])|\s*\|)"
)
INLINE_ATOM = re.compile(
    r"!?\[[^]\n]*\]\([^\n]*?\)|`+[^`\n]+`+|<https?://[^>\n]+>|https?://\S+"
)


def _wiki_files() -> list[Path]:
    files = list(DEFAULT_FILES)
    files.extend(
        path
        for path in sorted((REPOSITORY / "wiki").rglob("*.md"))
        if path.name != "SUMMARY.md" and "generated" not in path.parts
    )
    return files


def _protect_inline_atoms(text: str) -> tuple[str, list[str]]:
    atoms: list[str] = []

    def replace(match: re.Match[str]) -> str:
        atom = match.group(0)
        atoms.append(atom)
        identifier = f"\ue000{len(atoms) - 1:06d}"
        return identifier + ("x" * max(0, len(atom) - len(identifier) - 1)) + "\ue001"

    return INLINE_ATOM.sub(replace, text), atoms


def _restore_inline_atoms(text: str, atoms: list[str]) -> str:
    for index, atom in enumerate(atoms):
        identifier = f"\ue000{index:06d}"
        placeholder = identifier + ("x" * max(0, len(atom) - len(identifier) - 1)) + "\ue001"
        text = text.replace(placeholder, atom)
    return text


def _wrap_paragraph(lines: list[str], width: int) -> list[str]:
    if all(len(line) <= width or INLINE_ATOM.search(line) for line in lines):
        return lines
    if any(line.endswith("  ") for line in lines):
        return lines
    paragraph = " ".join(line.strip() for line in lines)
    protected, atoms = _protect_inline_atoms(paragraph)
    wrapped = textwrap.wrap(
        protected,
        width=width,
        break_long_words=False,
        break_on_hyphens=False,
        replace_whitespace=True,
        drop_whitespace=True,
    )
    return [_restore_inline_atoms(line, atoms) for line in wrapped]


def format_markdown(text: str, *, width: int = 80) -> str:
    source_lines = text.splitlines()
    output: list[str] = []
    paragraph: list[str] = []
    fence: str | None = None
    in_comment = False

    def flush() -> None:
        nonlocal paragraph
        if paragraph:
            output.extend(_wrap_paragraph(paragraph, width))
            paragraph = []

    for line in source_lines:
        stripped = line.lstrip()

        if in_comment:
            flush()
            output.append(line)
            if "-->" in line:
                in_comment = False
            continue
        if stripped.startswith("<!--"):
            flush()
            output.append(line)
            in_comment = "-->" not in line
            continue

        fence_match = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if fence is not None:
            flush()
            output.append(line)
            if fence_match and fence_match.group(1).startswith(fence[0]):
                fence = None
            continue
        if fence_match:
            flush()
            output.append(line)
            fence = fence_match.group(1)
            continue

        if SPECIAL_LINE.match(line) or line.startswith("  ") or "$$" in line:
            flush()
            output.append(line)
            continue
        paragraph.append(line)

    flush()
    rendered = "\n".join(output) + ("\n" if text.endswith("\n") or output else "")
    if text.split() != rendered.split():
        raise RuntimeError("formatter changed non-whitespace Markdown tokens")
    return rendered


def _line_exceeds_width(line: str, width: int) -> bool:
    if len(line) <= width:
        return False
    stripped = line.lstrip()
    if (
        not stripped
        or stripped.startswith(("#", "|", "```", "~~~", "<!--", "<http"))
        or "$$" in line
        or re.match(r"^\s*\[[^]]+\]:\s*\S+", line)
    ):
        return False
    return len(INLINE_ATOM.sub("x", line)) > width


def _changed_added_lines(reference: str) -> list[tuple[Path, int, str]]:
    result = subprocess.run(
        [
            "git",
            "diff",
            "--unified=0",
            "--no-color",
            reference,
            "--",
            "*.md",
        ],
        cwd=REPOSITORY,
        check=True,
        capture_output=True,
        text=True,
    )
    current_path: Path | None = None
    current_line = 0
    changed: list[tuple[Path, int, str]] = []
    for line in result.stdout.splitlines():
        if line.startswith("+++ b/"):
            current_path = (REPOSITORY / line.removeprefix("+++ b/")).resolve()
            continue
        hunk = re.match(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
        if hunk:
            current_line = int(hunk.group(1))
            continue
        if current_path is None or line.startswith(("---", "+++", "@@")):
            continue
        if line.startswith("+"):
            changed.append((current_path, current_line, line[1:]))
            current_line += 1
        elif not line.startswith("-"):
            current_line += 1
    return changed


def _fenced_code_lines(path: Path) -> set[int]:
    fenced: set[int] = set()
    marker: str | None = None
    for line_number, line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        match = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if marker is not None:
            fenced.add(line_number)
            if match and match.group(1).startswith(marker[0]):
                marker = None
            continue
        if match:
            marker = match.group(1)
            fenced.add(line_number)
    return fenced


def check_changed_lines(reference: str, *, width: int = 80) -> bool:
    fenced_by_path: dict[Path, set[int]] = {}
    failures = [
        (path, line_number, line)
        for path, line_number, line in _changed_added_lines(reference)
        if path.name != "SUMMARY.md"
        and "generated" not in path.parts
        and line_number
        not in fenced_by_path.setdefault(path, _fenced_code_lines(path))
        and _line_exceeds_width(line, width)
    ]
    if not failures:
        print("Changed Markdown lines respect the wiki wrapping policy.")
        return True
    for path, line_number, line in failures:
        print(
            f"{path.relative_to(REPOSITORY)}:{line_number}: "
            f"line exceeds {width} columns ({len(line)})"
        )
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", type=Path)
    parser.add_argument("--check", action="store_true")
    parser.add_argument(
        "--changed-from",
        metavar="GIT_REF",
        help="format only Markdown files changed from this Git reference",
    )
    parser.add_argument("--width", type=int, default=80)
    arguments = parser.parse_args()

    if arguments.paths and arguments.changed_from:
        parser.error("paths and --changed-from are mutually exclusive")
    if arguments.changed_from:
        if not arguments.check:
            parser.error("--changed-from requires --check")
        return 0 if check_changed_lines(arguments.changed_from, width=arguments.width) else 1

    files = [path.resolve() for path in arguments.paths] or _wiki_files()
    changed: list[Path] = []
    for path in files:
        original = path.read_text(encoding="utf-8")
        rendered = format_markdown(original, width=arguments.width)
        if original == rendered:
            continue
        changed.append(path)
        if not arguments.check:
            path.write_text(rendered, encoding="utf-8", newline="\n")

    if changed:
        action = "would rewrap" if arguments.check else "rewrapped"
        for path in changed:
            print(f"{action}: {path.relative_to(REPOSITORY)}")
        return 1 if arguments.check else 0
    print("Wiki prose wrapping is current.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
