#!/usr/bin/env python3
"""Fail closed on missing repository-local links in tracked Markdown files."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import urllib.parse
from dataclasses import dataclass
from pathlib import Path

LINK_RE = re.compile(r"!?\[[^\]\n]*\]\(([^)\n]+)\)")
FENCE_RE = re.compile(r"^\s*(`{3,}|~{3,})")
SCHEME_RE = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


@dataclass(frozen=True)
class MissingLink:
    markdown_path: str
    line: int
    target: str
    reason: str


def _tracked_markdown(root: Path) -> list[Path]:
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z", "*.md"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        raise RuntimeError("could not list tracked Markdown files") from exc
    paths = []
    for raw in result.stdout.split(b"\0"):
        if not raw:
            continue
        try:
            relative = raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise RuntimeError("tracked Markdown path is not UTF-8") from exc
        paths.append(root / relative)
    return sorted(paths)


def _extract_destination(raw: str) -> str | None:
    value = raw.strip()
    if not value:
        return None
    if value.startswith("<"):
        end = value.find(">", 1)
        if end < 0:
            return value
        return value[1:end].strip()
    # This repository does not use title-bearing local links. Splitting here still
    # handles the common Markdown form `(path "title")` conservatively.
    return value.split(maxsplit=1)[0]


def _is_external_or_anchor(target: str) -> bool:
    return (
        not target
        or target.startswith("#")
        or target.startswith("//")
        or target.startswith("/")
        or bool(SCHEME_RE.match(target))
    )


def check_markdown_links(root: Path, markdown_paths: list[Path]) -> tuple[int, list[MissingLink]]:
    repo_root = root.resolve()
    checked_links = 0
    missing: list[MissingLink] = []
    for path in markdown_paths:
        try:
            relative_path = path.resolve().relative_to(repo_root).as_posix()
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError, ValueError) as exc:
            raise RuntimeError(f"could not read tracked Markdown file: {path}") from exc

        fence: str | None = None
        for line_number, line in enumerate(text.splitlines(), start=1):
            fence_match = FENCE_RE.match(line)
            if fence_match:
                marker = fence_match.group(1)
                marker_char = marker[0]
                if fence is None:
                    fence = marker_char
                elif fence == marker_char:
                    fence = None
                continue
            if fence is not None:
                continue

            for match in LINK_RE.finditer(line):
                destination = _extract_destination(match.group(1))
                if destination is None or _is_external_or_anchor(destination):
                    continue
                checked_links += 1
                decoded = urllib.parse.unquote(destination)
                path_part = decoded.split("#", 1)[0].split("?", 1)[0]
                if not path_part:
                    continue
                candidate = (path.parent / path_part).resolve()
                try:
                    candidate.relative_to(repo_root)
                except ValueError:
                    missing.append(
                        MissingLink(relative_path, line_number, destination, "escapes repository root")
                    )
                    continue
                if not candidate.exists():
                    missing.append(MissingLink(relative_path, line_number, destination, "target missing"))
    return checked_links, missing


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Check tracked Markdown repository-local links.")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    root = args.root.resolve()
    try:
        markdown = _tracked_markdown(root)
        checked_links, missing = check_markdown_links(root, markdown)
    except RuntimeError as exc:
        print(f"markdown link check failed: {exc}", file=sys.stderr)
        return 2
    print(
        f"Markdown local links: files={len(markdown)} local_links={checked_links} missing={len(missing)}"
    )
    for item in missing:
        print(
            f"{item.markdown_path}:{item.line}: {item.target!r}: {item.reason}",
            file=sys.stderr,
        )
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
