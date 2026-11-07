"""Map a GitHub Actions event to packaging flags. No GitHub writes."""

from __future__ import annotations

from dataclasses import dataclass

import re

from errors import ReleaseError

SHA_RE = re.compile(r"^[0-9a-f]{40}$")
TAG_RE = re.compile(r"^v[0-9]+\.[0-9]+\.[0-9]+$")


def _as_bool(value: str | None, default: bool = False) -> bool:
    if value is None or value == "":
        return default
    lowered = value.strip().lower()
    if lowered in {"true", "1", "yes"}:
        return True
    if lowered in {"false", "0", "no"}:
        return False
    raise ReleaseError(f"not a boolean: {value!r}")


@dataclass(frozen=True)
class PackagingMode:
    tag: str
    checkout_ref: str
    upload_release: bool
    sign: bool
    stable_aliases: bool
    run_unix: bool

    def as_outputs(self) -> dict[str, str]:
        return {
            "tag": self.tag,
            "checkout_ref": self.checkout_ref,
            "upload_release": "true" if self.upload_release else "false",
            "sign": "true" if self.sign else "false",
            "stable_aliases": "true" if self.stable_aliases else "false",
            "run_unix": "true" if self.run_unix else "false",
        }


def resolve_packaging_mode(
    *,
    event_name: str,
    sha: str,
    release_tag: str = "",
    call_tag: str = "",
    call_checkout_ref: str = "",
    call_upload_release: str = "",
    call_sign: str = "",
    call_stable_aliases: str = "",
    call_windows_only: str = "",
    dispatch_windows_only: str = "",
) -> PackagingMode:
    if not SHA_RE.fullmatch(sha):
        raise ReleaseError(f"github.sha is not a 40-character commit: {sha!r}")

    if event_name == "pull_request":
        return PackagingMode(
            tag="",
            checkout_ref=sha,
            upload_release=False,
            sign=False,
            stable_aliases=False,
            run_unix=False,
        )
    if event_name == "workflow_dispatch":
        windows_only = _as_bool(dispatch_windows_only, default=False)
        return PackagingMode(
            tag="",
            checkout_ref=sha,
            upload_release=False,
            sign=True,
            stable_aliases=False,
            run_unix=not windows_only,
        )
    if event_name == "release":
        if not TAG_RE.fullmatch(release_tag):
            raise ReleaseError(
                f"published release tag is not vX.Y.Z: {release_tag!r}"
            )
        return PackagingMode(
            tag=release_tag,
            checkout_ref=sha,
            upload_release=True,
            sign=True,
            stable_aliases=True,
            run_unix=True,
        )
    if event_name == "workflow_call":
        tag = call_tag.strip()
        checkout_ref = call_checkout_ref.strip() or sha
        if tag and not TAG_RE.fullmatch(tag):
            raise ReleaseError(f"workflow_call tag is not vX.Y.Z: {tag!r}")
        if not SHA_RE.fullmatch(checkout_ref):
            raise ReleaseError(
                f"checkout_ref must be a 40-character commit SHA, not {checkout_ref!r}"
            )
        upload = _as_bool(call_upload_release, default=False)
        sign = _as_bool(call_sign, default=False)
        stable = _as_bool(call_stable_aliases, default=False)
        windows_only = _as_bool(call_windows_only, default=False)
        if upload and not tag:
            raise ReleaseError("upload_release requires a vX.Y.Z tag")
        if stable and not tag:
            raise ReleaseError("stable_aliases require a vX.Y.Z tag")
        return PackagingMode(
            tag=tag,
            checkout_ref=checkout_ref,
            upload_release=upload,
            sign=sign,
            stable_aliases=stable,
            run_unix=not windows_only,
        )
    raise ReleaseError(f"unsupported packaging event: {event_name!r}")
