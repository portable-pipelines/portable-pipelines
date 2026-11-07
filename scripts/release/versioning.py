"""Workspace version, lockfile, and changelog helpers."""

from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import date, datetime, timezone
from functools import total_ordering
from pathlib import Path

from errors import ReleaseError

VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")
WORKSPACE_VERSION_RE = re.compile(
    r'(?m)^(\[workspace\.package\][^\[]*?^version = ")([^"]+)(")',
    re.DOTALL,
)
CHANGELOG_UNRELEASED_RE = re.compile(
    r"(?ms)^## \[Unreleased\]\n\n(.*?)(?=^## \[)",
)
CHANGELOG_VERSION_HEADING_RE = re.compile(
    r"^## \[([0-9]+\.[0-9]+\.[0-9]+)\] - ([0-9]{4}-[0-9]{2}-[0-9]{2})\s*$",
    re.MULTILINE,
)
LOCK_PACKAGE_RE = re.compile(
    r"(?ms)^\[\[package\]\]\nname = \"([^\"]+)\"\nversion = \"([^\"]+)\"",
)


@total_ordering
@dataclass(frozen=True)
class Semver:
    major: int
    minor: int
    patch: int

    def __str__(self) -> str:
        return f"{self.major}.{self.minor}.{self.patch}"

    def __lt__(self, other: "Semver") -> bool:
        if not isinstance(other, Semver):
            return NotImplemented
        return (self.major, self.minor, self.patch) < (
            other.major,
            other.minor,
            other.patch,
        )


def parse_semver(text: str) -> Semver:
    if not VERSION_RE.fullmatch(text):
        raise ReleaseError(f"not a three-part version: {text!r}")
    major, minor, patch = (int(part) for part in text.split("."))
    return Semver(major, minor, patch)


def utc_today() -> date:
    return datetime.now(timezone.utc).date()


def workspace_version(cargo_toml: str) -> str:
    match = re.search(
        r'(?ms)^\[workspace\.package\].*?^version = "([^"]+)"',
        cargo_toml,
    )
    if not match:
        raise ReleaseError("could not find [workspace.package] version in Cargo.toml")
    return match.group(1)


def workspace_members(cargo_toml: str) -> list[str]:
    match = re.search(r"(?ms)^\[workspace\].*?^members = \[([^\]]*)\]", cargo_toml)
    if not match:
        raise ReleaseError("could not find workspace members in Cargo.toml")
    names: list[str] = []
    for item in re.findall(r'"([^"]+)"', match.group(1)):
        names.append(Path(item).name)
    if not names:
        raise ReleaseError("workspace members list is empty")
    return names


def set_workspace_version(cargo_toml: str, new_version: str) -> str:
    parse_semver(new_version)
    if not WORKSPACE_VERSION_RE.search(cargo_toml):
        raise ReleaseError("could not find [workspace.package] version in Cargo.toml")
    return WORKSPACE_VERSION_RE.sub(rf"\g<1>{new_version}\3", cargo_toml, count=1)


def lockfile_workspace_versions(lock_text: str, members: list[str]) -> dict[str, str]:
    found: dict[str, str] = {}
    for name, version in LOCK_PACKAGE_RE.findall(lock_text):
        if name in members:
            found[name] = version
    missing = [name for name in members if name not in found]
    if missing:
        raise ReleaseError(
            "Cargo.lock is missing workspace packages: " + ", ".join(missing)
        )
    return found


def set_lockfile_workspace_version(
    lock_text: str, members: list[str], new_version: str
) -> str:
    parse_semver(new_version)
    member_set = set(members)

    def replace(match: re.Match[str]) -> str:
        name = match.group(1)
        if name not in member_set:
            return match.group(0)
        return f'[[package]]\nname = "{name}"\nversion = "{new_version}"'

    updated, count = LOCK_PACKAGE_RE.subn(replace, lock_text)
    if count < len(members):
        raise ReleaseError("Cargo.lock did not contain every workspace package")
    return updated


def move_unreleased_changelog(
    changelog: str, version: str, release_date: date, previous_version: str
) -> str:
    parse_semver(version)
    parse_semver(previous_version)
    match = CHANGELOG_UNRELEASED_RE.search(changelog)
    if not match:
        raise ReleaseError("CHANGELOG.md has no ## [Unreleased] section")
    body = match.group(1)
    if f"## [{version}]" in changelog:
        raise ReleaseError(f"CHANGELOG.md already has a [{version}] section")
    date_text = release_date.isoformat()
    new_section = f"## [{version}] - {date_text}\n\n{body}"
    updated = CHANGELOG_UNRELEASED_RE.sub(
        "## [Unreleased]\n\n" + new_section, changelog, count=1
    )
    link = (
        f"[{version}]: https://github.com/portable-pipelines/portable-pipelines/"
        f"compare/v{previous_version}...v{version}\n"
    )
    marker = f"[{previous_version}]:"
    if marker not in updated:
        raise ReleaseError(
            f"CHANGELOG.md is missing the compare link for [{previous_version}]"
        )
    if f"[{version}]:" not in updated:
        updated = updated.replace(marker, link + marker, 1)
    return updated


def changelog_version_heading(changelog: str, version: str) -> tuple[str, str]:
    for found_version, found_date in CHANGELOG_VERSION_HEADING_RE.findall(changelog):
        if found_version == version:
            return found_version, found_date
    raise ReleaseError(
        f"CHANGELOG.md has no dated heading for [{version}] "
        f"(expected ## [{version}] - YYYY-MM-DD)"
    )


def changelog_section_body(changelog: str, version: str) -> str:
    pattern = re.compile(
        rf"(?ms)^## \[{re.escape(version)}\] - [0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}\n\n(.*?)(?=^## \[|^\[)",
    )
    match = pattern.search(changelog)
    if not match:
        raise ReleaseError(f"CHANGELOG.md has no section body for [{version}]")
    body = match.group(1).strip()
    if not body:
        raise ReleaseError(f"CHANGELOG.md section [{version}] is empty")
    return body


def assert_versions_consistent(
    *,
    cargo_toml: str,
    cargo_lock: str,
    changelog: str,
    version: str,
) -> None:
    actual = workspace_version(cargo_toml)
    if actual != version:
        raise ReleaseError(
            f"Cargo.toml workspace version is {actual}, expected {version}"
        )
    members = workspace_members(cargo_toml)
    lock_versions = lockfile_workspace_versions(cargo_lock, members)
    wrong = {
        name: found
        for name, found in lock_versions.items()
        if found != version
    }
    if wrong:
        details = ", ".join(f"{name}={found}" for name, found in sorted(wrong.items()))
        raise ReleaseError(
            f"Cargo.lock workspace packages are not {version}: {details}"
        )
    changelog_version_heading(changelog, version)
    changelog_section_body(changelog, version)


def read_repo_version_files(repo_root: Path) -> tuple[str, str, str]:
    cargo_toml = (repo_root / "Cargo.toml").read_text(encoding="utf-8")
    cargo_lock = (repo_root / "Cargo.lock").read_text(encoding="utf-8")
    changelog = (repo_root / "CHANGELOG.md").read_text(encoding="utf-8")
    return cargo_toml, cargo_lock, changelog
