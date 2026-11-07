#!/usr/bin/env python3
import json
import os
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer

from assets import expected_asset_names
from github_api import GitHubReader


class _Handler(BaseHTTPRequestHandler):
    routes: dict[str, tuple[int, dict]] = {}
    mutations: list[tuple[str, str]] = []

    def log_message(self, format: str, *args) -> None:  # noqa: A003
        return

    def _send(self, code: int, payload: dict | None) -> None:
        body = b"" if payload is None else json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        if self.path in self.routes:
            code, payload = self.routes[self.path]
            self._send(code, payload)
            return
        self._send(404, {"message": "Not Found"})

    def do_POST(self) -> None:  # noqa: N802
        self.mutations.append(("POST", self.path))
        self._send(500, {"message": "mutations are not allowed in this test server"})

    def do_PATCH(self) -> None:  # noqa: N802
        self.mutations.append(("PATCH", self.path))
        self._send(500, {"message": "mutations are not allowed in this test server"})

    def do_DELETE(self) -> None:  # noqa: N802
        self.mutations.append(("DELETE", self.path))
        self._send(500, {"message": "mutations are not allowed in this test server"})


class GitHubApiTests(unittest.TestCase):
    def setUp(self) -> None:
        _Handler.routes = {}
        _Handler.mutations = []
        self.server = HTTPServer(("127.0.0.1", 0), _Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        host, port = self.server.server_address
        self.reader = GitHubReader(
            repository="portable-pipelines/portable-pipelines",
            token="fake-token",
            api_url=f"http://{host}:{port}",
        )

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)

    def test_missing_tag_and_release(self) -> None:
        self.assertFalse(self.reader.tag_state("v0.1.7").exists)
        self.assertIsNone(self.reader.release_state("v0.1.7"))
        self.assertEqual(_Handler.mutations, [])

    def test_lightweight_tag(self) -> None:
        sha = "d" * 40
        _Handler.routes[
            "/repos/portable-pipelines/portable-pipelines/git/ref/tags/v0.1.7"
        ] = (
            200,
            {"object": {"type": "commit", "sha": sha}},
        )
        state = self.reader.tag_state("v0.1.7")
        self.assertTrue(state.exists)
        self.assertEqual(state.sha, sha)

    def test_draft_release_asset_names(self) -> None:
        _Handler.routes[
            "/repos/portable-pipelines/portable-pipelines/releases/tags/v0.1.7"
        ] = (
            200,
            {
                "draft": True,
                "prerelease": False,
                "target_commitish": "master",
                "assets": [{"name": "pipe-linux-x86_64-v0.1.7.tar.gz"}],
            },
        )
        release = self.reader.release_state("v0.1.7")
        assert release is not None
        self.assertTrue(release.draft)
        self.assertEqual(
            release.asset_names, ("pipe-linux-x86_64-v0.1.7.tar.gz",)
        )
        self.assertFalse(release.complete)
        self.assertEqual(_Handler.mutations, [])

    def test_complete_release_matches_expected_names(self) -> None:
        names = expected_asset_names("v0.1.6")
        self.assertEqual(len(names), 16)
        _Handler.routes[
            "/repos/portable-pipelines/portable-pipelines/releases/tags/v0.1.6"
        ] = (
            200,
            {
                "draft": False,
                "prerelease": False,
                "assets": [{"name": name} for name in names],
            },
        )
        release = self.reader.release_state("v0.1.6")
        assert release is not None
        self.assertTrue(release.complete)

    def test_live_v0_1_6_has_sixteen_assets_when_token_present(self) -> None:
        token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
        if not token:
            self.skipTest("no GitHub token for a read-only check")
        reader = GitHubReader(
            repository="portable-pipelines/portable-pipelines",
            token=token,
        )
        release = reader.release_state("v0.1.6")
        assert release is not None
        self.assertFalse(release.draft)
        self.assertTrue(release.complete)


if __name__ == "__main__":
    unittest.main()
