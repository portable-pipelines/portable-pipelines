"""Decide whether a validated intent may open or resume a draft release."""

from __future__ import annotations

from dataclasses import dataclass

from assets import expected_asset_names
from errors import ReleaseError
from intent import ReleaseIntent

ACTION_NO_OP = "no_op"
ACTION_CREATE_DRAFT = "create_draft"
ACTION_RESUME_DRAFT = "resume_draft"
ACTION_REFUSE = "refuse"


@dataclass(frozen=True)
class GitHubReleaseState:
    tag: str
    sha: str | None
    exists: bool
    draft: bool
    prerelease: bool
    asset_names: tuple[str, ...]

    @property
    def complete(self) -> bool:
        needed = set(expected_asset_names(self.tag))
        return needed.issubset(set(self.asset_names))


@dataclass(frozen=True)
class TagState:
    name: str
    sha: str | None
    exists: bool


@dataclass(frozen=True)
class Decision:
    action: str
    do_release: bool
    reason: str


def decide(
    *,
    intent: ReleaseIntent,
    commit_sha: str,
    event_name: str,
    ref: str,
    default_branch: str,
    tag: TagState,
    release: GitHubReleaseState | None,
) -> Decision:
    """Return a safe action for this intent and GitHub state.

    File contents are the only source of version and notes. Pull request
    titles and bodies are ignored. This function never mutates GitHub.
    """
    if not intent.publish:
        return Decision(
            action=ACTION_NO_OP,
            do_release=False,
            reason="intent.publish is false, so this is a dry check only",
        )

    on_default_push = (
        event_name == "push" and ref == f"refs/heads/{default_branch}"
    )
    on_pull_request = event_name == "pull_request"

    if event_name not in {"push", "pull_request"}:
        return Decision(
            action=ACTION_REFUSE,
            do_release=False,
            reason=f"event {event_name!r} is not allowed to publish",
        )

    commit_sha = commit_sha.lower()
    tag_sha = tag.sha.lower() if tag.sha else None
    release_sha = release.sha.lower() if release is not None and release.sha else None

    if tag.exists and tag_sha and tag_sha != commit_sha:
        raise ReleaseError(
            f"tag {intent.tag} already points at {tag.sha}, not {commit_sha}. "
            "Refusing to move or overwrite it."
        )

    if release is not None and not release.draft:
        if release_sha and release_sha != commit_sha:
            raise ReleaseError(
                f"published release {intent.tag} is for {release.sha}, not {commit_sha}"
            )
        if release.complete:
            return Decision(
                action=ACTION_NO_OP,
                do_release=False,
                reason=(
                    f"{intent.tag} is already published with the expected assets; "
                    "nothing to do"
                ),
            )
        raise ReleaseError(
            f"{intent.tag} is already published but is missing assets. "
            "Refusing to change a published release."
        )

    if release is not None and release.draft:
        if release_sha and release_sha != commit_sha:
            raise ReleaseError(
                f"draft {intent.tag} is for {release.sha}, not {commit_sha}. "
                "Refusing to retarget it."
            )
        action = ACTION_RESUME_DRAFT
        reason = f"draft {intent.tag} already exists at this commit; resume upload"
    elif tag.exists:
        action = ACTION_RESUME_DRAFT
        reason = (
            f"tag {intent.tag} already exists at this commit; "
            "create or resume the draft release"
        )
    else:
        action = ACTION_CREATE_DRAFT
        reason = f"create tag {intent.tag} at {commit_sha} and open a draft release"

    if on_pull_request:
        return Decision(
            action=action,
            do_release=False,
            reason=reason + " (pull request: validate only, do not publish)",
        )
    if not on_default_push:
        return Decision(
            action=ACTION_NO_OP,
            do_release=False,
            reason=(
                f"ref {ref} is not the default branch {default_branch}; "
                "ordinary branches do not publish"
            ),
        )
    return Decision(action=action, do_release=True, reason=reason)
