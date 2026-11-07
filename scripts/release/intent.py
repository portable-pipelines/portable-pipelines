"""Strict loader for the committed release intent file."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from errors import ReleaseError

INTENT_RELATIVE_PATH = ".github/release-intent.json"
NOTES_NAME_RE = re.compile(r"^docs/release-notes-v([0-9]+\.[0-9]+\.[0-9]+)\.md$")
VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
TAG_RE = re.compile(r"^v([0-9]+\.[0-9]+\.[0-9]+)$")
ALLOWED_KEYS = frozenset({"schema_version", "publish", "version", "tag", "notes_file"})
MAX_INTENT_BYTES = 8 * 1024
MAX_NOTES_BYTES = 512 * 1024
DRAFT_MARKER = "DRAFT-RELEASE-NOTES"
SCHEMA_VERSION = 1


@dataclass(frozen=True)
class ReleaseIntent:
    schema_version: int
    publish: bool
    version: str
    tag: str
    notes_file: str

    @property
    def notes_path(self) -> Path:
        return Path(self.notes_file)


def _reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    seen: dict[str, Any] = {}
    for key, value in pairs:
        if key in seen:
            raise ReleaseError(f"release intent has duplicate key {key!r}")
        seen[key] = value
    return seen


def parse_intent_text(text: str) -> ReleaseIntent:
    if "\x00" in text:
        raise ReleaseError("release intent must not contain NUL bytes")
    stripped = text.lstrip("\ufeff")
    if not stripped.strip():
        raise ReleaseError("release intent is empty")
    try:
        raw = json.loads(stripped, object_pairs_hook=_reject_duplicates)
    except json.JSONDecodeError as exc:
        raise ReleaseError(f"release intent is not valid JSON: {exc}") from exc
    if not isinstance(raw, dict):
        raise ReleaseError("release intent must be a JSON object")
    unknown = sorted(set(raw) - ALLOWED_KEYS)
    if unknown:
        raise ReleaseError(
            "release intent has unknown keys: " + ", ".join(unknown)
        )
    missing = sorted(ALLOWED_KEYS - set(raw))
    if missing:
        raise ReleaseError(
            "release intent is missing keys: " + ", ".join(missing)
        )
    schema_version = raw["schema_version"]
    if type(schema_version) is not int:
        raise ReleaseError("schema_version must be the integer 1")
    if schema_version != SCHEMA_VERSION:
        raise ReleaseError(
            f"schema_version must be {SCHEMA_VERSION}, not {schema_version!r}"
        )
    publish = raw["publish"]
    if not isinstance(publish, bool):
        raise ReleaseError("publish must be JSON true or false, not a string")
    version = raw["version"]
    if not isinstance(version, str) or not VERSION_RE.fullmatch(version):
        raise ReleaseError(
            "version must be a string like 0.1.7 with three numeric parts"
        )
    tag = raw["tag"]
    if not isinstance(tag, str) or not TAG_RE.fullmatch(tag):
        raise ReleaseError("tag must be a string like v0.1.7")
    if tag != f"v{version}":
        raise ReleaseError(f"tag {tag!r} must be 'v' plus version {version!r}")
    notes_file = raw["notes_file"]
    if not isinstance(notes_file, str):
        raise ReleaseError("notes_file must be a string")
    if "\\" in notes_file or notes_file.startswith("/") or ".." in Path(notes_file).parts:
        raise ReleaseError("notes_file must be a relative path without '..'")
    match = NOTES_NAME_RE.fullmatch(notes_file)
    if not match or match.group(1) != version:
        raise ReleaseError(
            f"notes_file must be docs/release-notes-v{version}.md"
        )
    return ReleaseIntent(
        schema_version=schema_version,
        publish=publish,
        version=version,
        tag=tag,
        notes_file=notes_file,
    )


def load_intent(path: Path) -> ReleaseIntent:
    if not path.is_file():
        raise ReleaseError(f"release intent file not found: {path}")
    data = path.read_bytes()
    if len(data) > MAX_INTENT_BYTES:
        raise ReleaseError(
            f"release intent is larger than {MAX_INTENT_BYTES} bytes"
        )
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ReleaseError("release intent must be UTF-8") from exc
    return parse_intent_text(text)


def dump_intent(intent: ReleaseIntent) -> str:
    payload = {
        "schema_version": intent.schema_version,
        "publish": intent.publish,
        "version": intent.version,
        "tag": intent.tag,
        "notes_file": intent.notes_file,
    }
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def validate_notes_file(repo_root: Path, intent: ReleaseIntent) -> Path:
    notes_path = (repo_root / intent.notes_file).resolve()
    repo_root_resolved = repo_root.resolve()
    try:
        notes_path.relative_to(repo_root_resolved)
    except ValueError as exc:
        raise ReleaseError("notes_file must stay inside the repository") from exc
    if not notes_path.is_file():
        raise ReleaseError(f"release notes file not found: {intent.notes_file}")
    data = notes_path.read_bytes()
    if len(data) > MAX_NOTES_BYTES:
        raise ReleaseError(
            f"release notes are larger than {MAX_NOTES_BYTES} bytes"
        )
    if b"\x00" in data:
        raise ReleaseError("release notes must not contain NUL bytes")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ReleaseError("release notes must be UTF-8") from exc
    if not text.strip():
        raise ReleaseError("release notes are empty")
    if DRAFT_MARKER in text:
        raise ReleaseError(
            "release notes still contain the draft marker "
            f"{DRAFT_MARKER}. Replace the skeleton with the reviewed notes "
            "before the intent pull request is merged."
        )
    return notes_path
