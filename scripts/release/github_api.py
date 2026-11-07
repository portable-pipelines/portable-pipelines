"""Read-only GitHub REST helpers. These functions never create or edit a release."""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any
from urllib.parse import quote

from decision import GitHubReleaseState, TagState
from errors import ReleaseError

DEFAULT_API = "https://api.github.com"
USER_AGENT = "portable-pipelines-release-helper"


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

    def get_json(self, path: str) -> Any | None:
        if self.dry_run:
            raise ReleaseError(
                "GitHubReader.dry_run is set; refusing even a GET so tests "
                "can inject fake state"
            )
        url = self.api_url.rstrip("/") + path
        request = urllib.request.Request(url, headers=self._headers(), method="GET")
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                payload = response.read()
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None
            detail = exc.read().decode("utf-8", errors="replace")
            raise ReleaseError(
                f"GitHub GET {path} failed with HTTP {exc.code}: {detail}"
            ) from exc
        except urllib.error.URLError as exc:
            raise ReleaseError(f"GitHub GET {path} failed: {exc}") from exc
        try:
            return json.loads(payload.decode("utf-8"))
        except json.JSONDecodeError as exc:
            raise ReleaseError(f"GitHub GET {path} returned invalid JSON") from exc

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

    def release_state(self, tag: str) -> GitHubReleaseState | None:
        encoded = quote(tag, safe="")
        payload = self.get_json(f"/repos/{self.repository}/releases/tags/{encoded}")
        if payload is None:
            return None
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
