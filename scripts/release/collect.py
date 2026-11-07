"""Collect commits since the previous published tag."""

from __future__ import annotations

import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

from errors import ReleaseError
from versioning import parse_semver

TAG_RE = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")


@dataclass(frozen=True)
class Commit:
    sha: str
    subject: str


@dataclass(frozen=True)
class ChangeList:
    previous_tag: str
    previous_sha: str
    commits: tuple[Commit, ...]


def run_git(repo_root: Path, *args: str) -> str:
    try:
        completed = subprocess.run(
            ["git", *args],
            cwd=repo_root,
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as exc:
        detail = (exc.stderr or exc.stdout or "").strip()
        raise ReleaseError(f"git {' '.join(args)} failed: {detail}") from exc
    return completed.stdout


def published_tags(repo_root: Path) -> list[str]:
    raw = run_git(repo_root, "tag", "-l", "v*")
    tags = [line.strip() for line in raw.splitlines() if TAG_RE.fullmatch(line.strip())]
    tags.sort(key=lambda tag: parse_semver(tag[1:]), reverse=True)
    return tags


def latest_published_tag(repo_root: Path) -> str:
    tags = published_tags(repo_root)
    if not tags:
        raise ReleaseError("no vX.Y.Z tags found in this clone")
    return tags[0]


def tag_sha(repo_root: Path, tag: str) -> str:
    return run_git(repo_root, "rev-list", "-n", "1", tag).strip()


def commits_since(repo_root: Path, previous_tag: str) -> ChangeList:
    if not TAG_RE.fullmatch(previous_tag):
        raise ReleaseError(f"previous tag is not a vX.Y.Z tag: {previous_tag!r}")
    previous_sha = tag_sha(repo_root, previous_tag)
    raw = run_git(
        repo_root,
        "log",
        f"{previous_tag}..HEAD",
        "--format=%H%x09%s",
    )
    commits: list[Commit] = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        sha, subject = line.split("\t", 1)
        commits.append(Commit(sha=sha, subject=subject))
    return ChangeList(
        previous_tag=previous_tag,
        previous_sha=previous_sha,
        commits=tuple(commits),
    )


def format_change_list(changes: ChangeList) -> str:
    lines = [
        f"Changes since {changes.previous_tag} ({changes.previous_sha[:12]}):",
        "",
    ]
    if not changes.commits:
        lines.append("No commits since that tag.")
        return "\n".join(lines) + "\n"
    for commit in changes.commits:
        lines.append(f"- {commit.sha[:7]} {commit.subject}")
    return "\n".join(lines) + "\n"
