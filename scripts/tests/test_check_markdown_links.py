from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts import check_markdown_links as checker


class MarkdownLinkTests(unittest.TestCase):
    def test_local_links_exist_and_external_or_fenced_links_are_ignored(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "docs").mkdir()
            (root / "docs" / "other.md").write_text("ok\n", encoding="utf-8")
            source = root / "README.md"
            source.write_text(
                "[local](docs/other.md#section)\n"
                "[web](https://example.com/x)\n"
                "[anchor](#local)\n"
                "```markdown\n[example](missing.md)\n```\n",
                encoding="utf-8",
            )
            count, missing = checker.check_markdown_links(root, [source, root / "docs" / "other.md"])
            self.assertEqual(count, 1)
            self.assertEqual(missing, [])

    def test_missing_and_escape_are_reported(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "README.md"
            source.write_text("[missing](nope.md)\n[escape](../outside.md)\n", encoding="utf-8")
            count, missing = checker.check_markdown_links(root, [source])
            self.assertEqual(count, 2)
            self.assertEqual([item.reason for item in missing], ["target missing", "escapes repository root"])

    def test_percent_encoded_local_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "a b.md").write_text("ok\n", encoding="utf-8")
            source = root / "README.md"
            source.write_text("[encoded](a%20b.md)\n", encoding="utf-8")
            count, missing = checker.check_markdown_links(root, [source])
            self.assertEqual(count, 1)
            self.assertEqual(missing, [])


if __name__ == "__main__":
    unittest.main()
