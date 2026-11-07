#!/usr/bin/env python3
import tempfile
import unittest
from pathlib import Path

from errors import ReleaseError
from intent import (
    DRAFT_MARKER,
    dump_intent,
    load_intent,
    parse_intent_text,
    validate_notes_file,
)

EXAMPLE = Path(__file__).resolve().parent / "example-intent.json"


VALID = """{
  "schema_version": 1,
  "publish": true,
  "version": "0.1.7",
  "tag": "v0.1.7",
  "notes_file": "docs/release-notes-v0.1.7.md"
}
"""


class IntentTests(unittest.TestCase):
    def test_valid_intent(self) -> None:
        intent = parse_intent_text(VALID)
        self.assertEqual(intent.version, "0.1.7")
        self.assertEqual(intent.tag, "v0.1.7")
        self.assertTrue(intent.publish)
        self.assertEqual(dump_intent(intent).count("\n"), 7)

    def test_rejects_publish_string(self) -> None:
        text = VALID.replace("true", '"true"')
        with self.assertRaisesRegex(ReleaseError, "publish must be JSON true"):
            parse_intent_text(text)

    def test_rejects_duplicate_keys(self) -> None:
        text = VALID.replace(
            '"publish": true',
            '"publish": false,\n  "publish": true',
        )
        with self.assertRaisesRegex(ReleaseError, "duplicate key"):
            parse_intent_text(text)

    def test_rejects_unknown_keys(self) -> None:
        text = VALID.replace(
            '"notes_file":',
            '"title": "pwned",\n  "notes_file":',
        )
        with self.assertRaisesRegex(ReleaseError, "unknown keys"):
            parse_intent_text(text)

    def test_rejects_pr_title_injection_in_version(self) -> None:
        text = VALID.replace('"0.1.7"', '"0.1.7; rm -rf /"')
        with self.assertRaisesRegex(ReleaseError, "three numeric parts"):
            parse_intent_text(text)

    def test_rejects_tag_mismatch(self) -> None:
        text = VALID.replace('"tag": "v0.1.7"', '"tag": "v0.1.8"')
        with self.assertRaisesRegex(ReleaseError, "must be 'v' plus version"):
            parse_intent_text(text)

    def test_rejects_notes_path_escape(self) -> None:
        text = VALID.replace(
            "docs/release-notes-v0.1.7.md",
            "../evil.md",
        )
        with self.assertRaisesRegex(ReleaseError, "notes_file must be"):
            parse_intent_text(text)

    def test_rejects_schema_bool(self) -> None:
        text = VALID.replace('"schema_version": 1', '"schema_version": true')
        with self.assertRaisesRegex(ReleaseError, "integer 1"):
            parse_intent_text(text)

    def test_rejects_trailing_comma(self) -> None:
        text = VALID.replace('.md"\n}', '.md",\n}')
        with self.assertRaisesRegex(ReleaseError, "not valid JSON"):
            parse_intent_text(text)

    def test_rejects_missing_file(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "not found"):
            load_intent(Path("/tmp/does-not-exist-release-intent.json"))

    def test_notes_must_drop_draft_marker(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            notes = root / "docs" / "release-notes-v0.1.7.md"
            notes.parent.mkdir(parents=True)
            notes.write_text(f"hello {DRAFT_MARKER}\n", encoding="utf-8")
            intent = parse_intent_text(VALID)
            with self.assertRaisesRegex(ReleaseError, "draft marker"):
                validate_notes_file(root, intent)

    def test_example_intent_is_publish_false(self) -> None:
        intent = load_intent(EXAMPLE)
        self.assertFalse(intent.publish)
        self.assertEqual(intent.schema_version, 1)

    def test_notes_ok(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            notes = root / "docs" / "release-notes-v0.1.7.md"
            notes.parent.mkdir(parents=True)
            notes.write_text("# Portable Pipelines 0.1.7\n\nReady.\n", encoding="utf-8")
            intent = parse_intent_text(VALID)
            self.assertEqual(validate_notes_file(root, intent), notes.resolve())


if __name__ == "__main__":
    unittest.main()
