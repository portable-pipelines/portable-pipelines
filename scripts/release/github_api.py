"""Read-only GitHub REST helpers. These functions never create or edit a release."""

from __future__ import annotations

import json
import os
import re
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any
from urllib.parse import quote

from decision import GitHubReleaseState, TagState
from errors import ReleaseError

DEFAULT_API = "https://api.github.com"
USER_AGENT = "portable-pipelines-release-helper"
_NEXT_REL = re.compile(r'rel\s*=\s*"?next"?', re.IGNORECASE)
_MAX_RELEASE_PAGES = 20
_RELEASES_PER_PAGE = 100


def next_page_path(link_header: str | None, api_url: str) -> str | None:
    """Return the path (or URL) of Link rel=next, or None."""
    if not link_header:
        return None
    for part in link_header.split(","):
        if not _NEXT_REL.search(part):
            continue
        start = part.find("<")
        end = part.find(">", start + 1)
        if start < 0 or end < 0:
            continue
        url = part[start + 1 : end].strip()
        prefix = api_url.rstrip("/")
        if url.startswith(prefix):
            rest = url[len(prefix) :]
            if not rest:
                return "/"
            return rest if rest.startswith("/") else "/" + rest
        return url
    return None


@dataclass
class GitHubReader:
    repository: str
    token: str | None = None
    api_url: str = DEFAULT_API
    dry_run: bool = False

    def _headers(self) -> dict[str, str]:
        headers = {
            "Accept": "application/vnd.github+json",
            "User-Agent": USER_AGENT,
            "X-GitHub-Api-Version": "2022-11-28",
        }
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        return headers

    def _request(self, path_or_url: str) -> tuple[Any | None, dict[str, str]]:
        if self.dry_run:
            raise ReleaseError(
                "GitHubReader.dry_run is set; refusing even a GET so tests "
                "can inject fake state"
            )
        if path_or_url.startswith("http://") or path_or_url.startswith("https://"):
            url = path_or_url
        else:
            url = self.api_url.rstrip("/") + path_or_url
        request = urllib.request.Request(url, headers=self._headers(), method="GET")
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                payload = response.read()
                headers = {k: v for k, v in response.headers.items()}
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None, {}
            detail = exc.read().decode("utf-8", errors="replace")
            raise ReleaseError(
                f"GitHub GET {path_or_url} failed with HTTP {exc.code}: {detail}"
            ) from exc
        except urllib.error.URLError as exc:
            raise ReleaseError(f"GitHub GET {path_or_url} failed: {exc}") from exc
        if not payload:
            return None, headers
        try:
            return json.loads(payload.decode("utf-8")), headers
        except json.JSONDecodeError as exc:
            raise ReleaseError(
                f"GitHub GET {path_or_url} returned invalid JSON"
            ) from exc

    def get_json(self, path: str) -> Any | None:
        payload, _headers = self._request(path)
        return payload

    def list_releases(self) -> list[dict[str, Any]]:
        """All Releases, including drafts. Never uses GET /releases/tags."""
        items: list[dict[str, Any]] = []
        path: str | None = (
            f"/repos/{self.repository}/releases?per_page={_RELEASES_PER_PAGE}"
        )
        for _ in range(_MAX_RELEASE_PAGES):
            if not path:
                break
            payload, headers = self._request(path)
            if payload is None:
                break
            if not isinstance(payload, list):
                raise ReleaseError("GitHub releases list is not an array")
            for entry in payload:
                if isinstance(entry, dict):
                    items.append(entry)
            path = next_page_path(
                headers.get("Link") or headers.get("link"), self.api_url
            )
        return items

    def tag_state(self, tag: str) -> TagState:
        encoded = quote(tag, safe="")
        payload = self.get_json(f"/repos/{self.repository}/git/ref/tags/{encoded}")
        if payload is None:
            return TagState(name=tag, sha=None, exists=False)
        obj = payload.get("object") or {}
        obj_type = obj.get("type")
        sha = obj.get("sha")
        if obj_type == "tag" and isinstance(sha, str):
            tag_obj = self.get_json(f"/repos/{self.repository}/git/tags/{sha}")
            if tag_obj and isinstance(tag_obj.get("object"), dict):
                sha = tag_obj["object"].get("sha")
        if not isinstance(sha, str):
            raise ReleaseError(f"tag {tag} has no commit SHA")
        return TagState(name=tag, sha=sha, exists=True)

    def release_state(
        self, tag: str, *, prefer_draft: bool = False
    ) -> GitHubReleaseState | None:
        """Find a Release by tag_name, including drafts.

        GET /releases/tags/{tag} 404s for drafts (their html_url is often
        /releases/tag/untagged-*). Listing /releases and matching tag_name
        is what finds them. When prefer_draft is true, a draft with that
        tag wins over a published Release of the same tag.
        """
        matches: list[dict[str, Any]] = []
        for payload in self.list_releases():
            if payload.get("tag_name") == tag:
                matches.append(payload)
        if not matches:
            return None
        if prefer_draft:
            chosen = next((item for item in matches if item.get("draft")), matches[0])
        else:
            chosen = next(
                (item for item in matches if not item.get("draft")), matches[0]
            )
        return _release_from_payload(tag, chosen)


def _release_from_payload(tag: str, payload: dict[str, Any]) -> GitHubReleaseState:
    assets = payload.get("assets") or []
    names = tuple(
        str(asset["name"])
        for asset in assets
        if isinstance(asset, dict) and "name" in asset
    )
    sha = None
    target = payload.get("target_commitish")
    # target_commitish is often a branch name. Prefer the tag object's SHA
    # when the caller already resolved it.
    if isinstance(target, str) and len(target) == 40 and all(
        ch in "0123456789abcdef" for ch in target.lower()
    ):
        sha = target.lower()
    return GitHubReleaseState(
        tag=tag,
        sha=sha,
        exists=True,
        draft=bool(payload.get("draft")),
        prerelease=bool(payload.get("prerelease")),
        asset_names=names,
    )


def reader_from_env(*, dry_run: bool = False) -> GitHubReader:
    repository = os.environ.get("GITHUB_REPOSITORY", "portable-pipelines/portable-pipelines")
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    api_url = os.environ.get("GITHUB_API_URL", DEFAULT_API)
    return GitHubReader(
        repository=repository,
        token=token,
        api_url=api_url,
        dry_run=dry_run,
    )
