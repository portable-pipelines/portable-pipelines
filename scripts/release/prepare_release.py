#!/usr/bin/env python3
"""Maintainer helpers for a chat-driven Portable Pipelines release.

These commands edit local files and read GitHub. They never create a tag,
publish a GitHub Release, or upload assets. GitHub Actions does that after
an approved release-intent pull request is merged to master.
"""

from __future__ import annotations

import argparse
import os
import sys
from datetime import date
from pathlib import Path

from assets import expected_asset_names
from collect import (
    commits_since,
    format_change_list,
    latest_published_tag,
)
from decision import GitHubReleaseState, decide
from errors import ReleaseError
from github_api import GitHubReader, reader_from_env
from intent import (
    INTENT_RELATIVE_PATH,
    SCHEMA_VERSION,
    ReleaseIntent,
    dump_intent,
    load_intent,
    validate_notes_file,
)
from notes import skeleton_notes
from outputs import write_github_output
from versioning import (
    assert_versions_consistent,
    move_unreleased_changelog,
    parse_semver,
    read_repo_version_files,
    set_lockfile_workspace_version,
    set_workspace_version,
    utc_today,
    workspace_members,
    workspace_version,
)
from workflow_config import resolve_packaging_mode

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent.parent


def _repo_root(args: argparse.Namespace) -> Path:
    return Path(args.repo_root).resolve()


def cmd_collect(args: argparse.Namespace) -> int:
    root = _repo_root(args)
    previous = args.since or latest_published_tag(root)
    changes = commits_since(root, previous)
    sys.stdout.write(format_change_list(changes))
    return 0


def cmd_prepare(args: argparse.Namespace) -> int:
    root = _repo_root(args)
    version = args.version
    parse_semver(version)
    cargo_toml, cargo_lock, changelog = read_repo_version_files(root)
    current = workspace_version(cargo_toml)
    if parse_semver(version) <= parse_semver(current):
        raise ReleaseError(
            f"new version {version} must be greater than the current workspace "
            f"version {current}"
        )
    previous_tag = args.since or latest_published_tag(root)
    previous_version = previous_tag[1:]
    if previous_version != current:
        raise ReleaseError(
            f"latest git tag is {previous_tag}, but Cargo.toml is {current}. "
            "They must match before you prepare a newer version."
        )
    release_date = date.fromisoformat(args.date) if args.date else utc_today()
    changes = commits_since(root, previous_tag)
    members = workspace_members(cargo_toml)
    new_toml = set_workspace_version(cargo_toml, version)
    new_lock = set_lockfile_workspace_version(cargo_lock, members, version)
    new_changelog = move_unreleased_changelog(
        changelog, version, release_date, previous_version
    )
    intent = ReleaseIntent(
        schema_version=SCHEMA_VERSION,
        publish=True,
        version=version,
        tag=f"v{version}",
        notes_file=f"docs/release-notes-v{version}.md",
    )
    notes_path = root / intent.notes_file
    intent_path = root / INTENT_RELATIVE_PATH
    if args.dry_run:
        sys.stdout.write(
            f"Would bump {current} -> {version} on {release_date.isoformat()}\n"
        )
        sys.stdout.write(f"Would write {intent_path.relative_to(root)}\n")
        sys.stdout.write(f"Would write {notes_path.relative_to(root)}\n")
        sys.stdout.write("\n")
        sys.stdout.write(format_change_list(changes))
        return 0
    (root / "Cargo.toml").write_text(new_toml, encoding="utf-8")
    (root / "Cargo.lock").write_text(new_lock, encoding="utf-8")
    (root / "CHANGELOG.md").write_text(new_changelog, encoding="utf-8")
    intent_path.parent.mkdir(parents=True, exist_ok=True)
    intent_path.write_text(dump_intent(intent), encoding="utf-8")
    if notes_path.exists() and not args.force_notes:
        sys.stdout.write(
            f"Left existing notes in place: {notes_path.relative_to(root)}\n"
        )
    else:
        notes_path.write_text(skeleton_notes(version, changes), encoding="utf-8")
        sys.stdout.write(
            f"Wrote a notes skeleton with {DRAFT_MARKER_NOTICE}\n"
        )
    sys.stdout.write(f"Wrote {intent_path.relative_to(root)} with publish=true\n")
    sys.stdout.write(
        "Rewrite the notes, then open a dedicated release-intent pull request. "
        "Merging that pull request is what publishes.\n"
    )
    sys.stdout.write("\n")
    sys.stdout.write(format_change_list(changes))
    return 0


DRAFT_MARKER_NOTICE = "the draft marker still in the file"


def cmd_check(args: argparse.Namespace) -> int:
    root = _repo_root(args)
    intent_path = Path(args.intent) if args.intent else root / INTENT_RELATIVE_PATH
    if not intent_path.is_file():
        if args.require_intent:
            raise ReleaseError(f"release intent file not found: {intent_path}")
        sys.stdout.write(
            "No release intent file is present. Ordinary commits do not publish.\n"
        )
        return 0
    intent = load_intent(intent_path)
    cargo_toml, cargo_lock, changelog = read_repo_version_files(root)
    if intent.publish:
        assert_versions_consistent(
            cargo_toml=cargo_toml,
            cargo_lock=cargo_lock,
            changelog=changelog,
            version=intent.version,
        )
        validate_notes_file(root, intent)
    else:
        sys.stdout.write("intent.publish is false; skipping version and notes checks.\n")
    sys.stdout.write(
        f"Intent is valid: tag {intent.tag}, publish={str(intent.publish).lower()}\n"
    )
    return 0


def _release_with_sha(
    release: GitHubReleaseState | None, tag_sha: str | None
) -> GitHubReleaseState | None:
    if release is None:
        return None
    if release.sha:
        return release
    if tag_sha:
        return GitHubReleaseState(
            tag=release.tag,
            sha=tag_sha,
            exists=release.exists,
            draft=release.draft,
            prerelease=release.prerelease,
            asset_names=release.asset_names,
        )
    return release


def cmd_plan(args: argparse.Namespace) -> int:
    root = _repo_root(args)
    intent_path = Path(args.intent) if args.intent else root / INTENT_RELATIVE_PATH
    sha = args.sha or os.environ.get("GITHUB_SHA")
    if not sha:
        raise ReleaseError("pass --sha or set GITHUB_SHA")
    if not intent_path.is_file():
        sys.stdout.write(
            "No release intent file is present. Ordinary commits do not publish.\n"
        )
        values = {
            "action": "no_op",
            "do_release": "false",
            "tag": "",
            "version": "",
            "notes_file": "",
            "sha": sha,
        }
        if args.github_output:
            write_github_output(Path(args.github_output), values)
        return 0
    intent = load_intent(intent_path)
    cargo_toml, cargo_lock, changelog = read_repo_version_files(root)
    assert_versions_consistent(
        cargo_toml=cargo_toml,
        cargo_lock=cargo_lock,
        changelog=changelog,
        version=intent.version,
    )
    validate_notes_file(root, intent)
    event_name = args.event_name or os.environ.get("GITHUB_EVENT_NAME") or "push"
    ref = args.ref or os.environ.get("GITHUB_REF") or "refs/heads/master"
    default_branch = args.default_branch
    tag_state = None
    release_state = None
    if args.offline:
        from decision import TagState

        tag_state = TagState(name=intent.tag, sha=None, exists=False)
        release_state = None
    else:
        reader = getattr(args, "reader", None) or reader_from_env(dry_run=False)
        tag_state = reader.tag_state(intent.tag)
        release_state = _release_with_sha(
            reader.release_state(intent.tag, prefer_draft=True), tag_state.sha
        )
    decision = decide(
        intent=intent,
        commit_sha=sha,
        event_name=event_name,
        ref=ref,
        default_branch=default_branch,
        tag=tag_state,
        release=release_state,
    )
    sys.stdout.write(f"{decision.action}: {decision.reason}\n")
    values = {
        "action": decision.action,
        "do_release": "true" if decision.do_release else "false",
        "tag": intent.tag,
        "version": intent.version,
        "notes_file": intent.notes_file,
        "sha": sha,
    }
    if args.github_output:
        write_github_output(Path(args.github_output), values)
    else:
        for key, value in values.items():
            sys.stdout.write(f"{key}={value}\n")
    return 0


def cmd_verify_assets(args: argparse.Namespace) -> int:
    reader = getattr(args, "reader", None) or reader_from_env(dry_run=False)
    release = reader.release_state(
        args.tag, prefer_draft=bool(args.require_draft)
    )
    if release is None:
        raise ReleaseError(f"no GitHub Release named {args.tag}")
    needed = expected_asset_names(args.tag)
    have = set(release.asset_names)
    missing = [name for name in needed if name not in have]
    if missing:
        raise ReleaseError(
            f"{args.tag} is missing assets: " + ", ".join(missing)
        )
    extra_ok = have - set(needed)
    sys.stdout.write(
        f"{args.tag} has all {len(needed)} expected assets"
        + (f" (and {len(extra_ok)} extra)" if extra_ok else "")
        + "\n"
    )
    if not release.draft and args.require_draft:
        raise ReleaseError(f"{args.tag} is not a draft; refusing to treat it as one")
    return 0


def cmd_workflow_config(args: argparse.Namespace) -> int:
    event_name = args.event_name or os.environ.get("EVENT_NAME") or ""
    mode = resolve_packaging_mode(
        event_name=event_name,
        sha=args.sha or os.environ.get("GITHUB_SHA_VALUE") or os.environ.get("GITHUB_SHA") or "",
        release_tag=args.release_tag or os.environ.get("RELEASE_TAG_NAME") or "",
        call_tag=os.environ.get("CALL_TAG") or "",
        call_checkout_ref=os.environ.get("CALL_CHECKOUT_REF") or "",
        call_upload_release=os.environ.get("CALL_UPLOAD_RELEASE") or "",
        call_sign=os.environ.get("CALL_SIGN") or "",
        call_stable_aliases=os.environ.get("CALL_STABLE_ALIASES") or "",
        call_windows_only=os.environ.get("CALL_WINDOWS_ONLY") or "",
        call_caller=os.environ.get("CALL_CALLER") or "",
        dispatch_windows_only=os.environ.get("DISPATCH_WINDOWS_ONLY") or "",
    )
    values = mode.as_outputs()
    values["skip_package"] = "false"
    if event_name == "release" and mode.tag:
        release = None
        try:
            reader = getattr(args, "reader", None) or reader_from_env(dry_run=False)
            release = reader.release_state(mode.tag, prefer_draft=False)
        except ReleaseError as exc:
            sys.stdout.write(
                f"could not list releases ({exc}); packaging will run\n"
            )
        if (
            release is not None
            and not release.draft
            and release.complete
        ):
            values["skip_package"] = "true"
            values["upload_release"] = "false"
            values["sign"] = "false"
            values["stable_aliases"] = "false"
            values["run_unix"] = "false"
            sys.stdout.write(
                f"{mode.tag} already has all expected assets; skipping rebuild\n"
            )
    if args.github_output:
        write_github_output(Path(args.github_output), values)
    else:
        for key, value in values.items():
            sys.stdout.write(f"{key}={value}\n")
    return 0


def cmd_expected_assets(args: argparse.Namespace) -> int:
    for name in expected_asset_names(args.tag):
        sys.stdout.write(name + "\n")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Prepare and validate a Portable Pipelines release intent. "
            "This tool does not publish."
        )
    )
    parser.add_argument(
        "--repo-root",
        default=str(REPO_ROOT),
        help="Path to the repository root",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    collect = sub.add_parser("collect", help="List commits since the previous tag")
    collect.add_argument("--since", help="Previous tag, for example v0.1.6")
    collect.set_defaults(func=cmd_collect)

    prepare = sub.add_parser(
        "prepare",
        help="Bump version, changelog, intent, and a notes skeleton",
    )
    prepare.add_argument("--version", required=True, help="New version, for example 0.1.7")
    prepare.add_argument("--since", help="Previous tag, for example v0.1.6")
    prepare.add_argument("--date", help="Changelog date YYYY-MM-DD (UTC today if omitted)")
    prepare.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would change without writing files",
    )
    prepare.add_argument(
        "--force-notes",
        action="store_true",
        help="Overwrite an existing notes file with a new skeleton",
    )
    prepare.set_defaults(func=cmd_prepare)

    check = sub.add_parser("check", help="Validate the committed intent if present")
    check.add_argument("--intent", help="Path to the intent file")
    check.add_argument(
        "--require-intent",
        action="store_true",
        help="Fail if the intent file is missing",
    )
    check.set_defaults(func=cmd_check)

    plan = sub.add_parser(
        "plan",
        help="Validate intent and decide draft/resume/no-op (read-only GitHub)",
    )
    plan.add_argument("--intent", help="Path to the intent file")
    plan.add_argument("--sha", help="Commit SHA that would be tagged")
    plan.add_argument("--event-name", help="GitHub event name")
    plan.add_argument("--ref", help="GitHub ref, for example refs/heads/master")
    plan.add_argument("--default-branch", default="master")
    plan.add_argument("--offline", action="store_true", help="Do not call GitHub")
    plan.add_argument("--github-output", help="Append outputs for Actions")
    plan.set_defaults(func=cmd_plan)

    verify = sub.add_parser(
        "verify-assets",
        help="Check that a GitHub Release has the 16 expected files",
    )
    verify.add_argument("--tag", required=True)
    verify.add_argument(
        "--require-draft",
        action="store_true",
        help="Fail if the release is already published",
    )
    verify.set_defaults(func=cmd_verify_assets)

    config = sub.add_parser(
        "workflow-config",
        help="Map a GitHub Actions event to packaging flags",
    )
    config.add_argument("--event-name")
    config.add_argument("--sha")
    config.add_argument("--release-tag")
    config.add_argument("--github-output")
    config.set_defaults(func=cmd_workflow_config)

    names = sub.add_parser("expected-assets", help="Print the 16 expected asset names")
    names.add_argument("--tag", required=True)
    names.set_defaults(func=cmd_expected_assets)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except ReleaseError as exc:
        sys.stderr.write(f"error: {exc}\n")
        return 1


if __name__ == "__main__":
    sys.exit(main())
