"""Draft a notes skeleton from collected commits. This is not the public story."""

from __future__ import annotations

from collect import ChangeList
from intent import DRAFT_MARKER


def skeleton_notes(version: str, changes: ChangeList) -> str:
    """Return a marked skeleton the editor or assistant must rewrite."""
    commit_lines = "\n".join(
        f"- {commit.sha[:7]} {commit.subject}" for commit in changes.commits
    )
    if not commit_lines:
        commit_lines = "- (no commits since the previous tag)"
    tag = f"v{version}"
    return f"""# Portable Pipelines {version}

{DRAFT_MARKER}

This file is a skeleton. The helper listed commits since {changes.previous_tag}.
It did not write the public story. Replace this whole file with reviewed notes
in complete sentences before anyone approves the release-intent pull request.
GitHub will publish this file as written. It will not regenerate the notes
after approval.

Commits since {changes.previous_tag}:

{commit_lines}

## What's new

- Replace this list with the public story.

## Downloads

| File | Platform |
|---|---|
| [`pipe-windows-x64-{tag}.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/download/{tag}/pipe-windows-x64-{tag}.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64-{tag}.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/{tag}/pipe-linux-x86_64-{tag}.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64-{tag}.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/{tag}/pipe-macos-arm64-{tag}.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64-{tag}.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/download/{tag}/pipe-macos-x86_64-{tag}.tar.gz) | macOS on Intel (x86_64) |

The same bytes are also published without a version in the name:

| File | Platform |
|---|---|
| [`pipe-windows-x64.zip`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-windows-x64.zip) | Windows x64 (includes Regina REXX) |
| [`pipe-linux-x86_64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-linux-x86_64.tar.gz) | Linux x86_64 |
| [`pipe-macos-arm64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-macos-arm64.tar.gz) | macOS on Apple silicon (arm64) |
| [`pipe-macos-x86_64.tar.gz`](https://github.com/portable-pipelines/portable-pipelines/releases/latest/download/pipe-macos-x86_64.tar.gz) | macOS on Intel (x86_64) |

Each archive has a matching `.sha256` file so you can check your download. The checksum names the file it belongs to. GitHub also records a build provenance attestation for each archive.

## Quick start

A small tip first: in `literal hello| console` there is no space before the `|`. Any space after the literal text becomes part of the record.

To check which version you have, run `pipe --version` (`.\\pipe.exe --version` on Windows). It prints `pipe {version}`.
"""
