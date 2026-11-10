#!/usr/bin/env python3
import json
import os
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer

from argparse import Namespace

from assets import expected_asset_names
from github_api import GitHubReader, next_page_path
from prepare_release import cmd_verify_assets, cmd_workflow_config
from decision import GitHubReleaseState


class _Handler(BaseHTTPRequestHandler):
    routes: dict[str, tuple] = {}
    mutations: list[tuple[str, str]] = []
    gets: list[str] = []

    def log_message(self, format: str, *args) -> None:  # noqa: A003
        return

    def _send(
        self, code: int, payload: dict | list | None, extra: dict[str, str] | None = None
    ) -> None:
        body = b"" if payload is None else json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        if extra:
            for key, value in extra.items():
                self.send_header(key, value)
        self.end_headers()
        if body:
            self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        _Handler.gets.append(self.path)
        if self.path in self.routes:
            spec = self.routes[self.path]
            if len(spec) == 2:
                code, payload = spec
                extra = None
            else:
                code, payload, extra = spec
            self._send(code, payload, extra)
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
        _Handler.gets = []
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
            "/repos/portable-pipelines/portable-pipelines/releases?per_page=100"
        ] = (
            200,
            [
                {
                    "tag_name": "v0.1.7",
                    "draft": True,
                    "prerelease": False,
                    "target_commitish": "master",
                    "html_url": (
                        "https://github.com/portable-pipelines/"
                        "portable-pipelines/releases/tag/untagged-cef0d67be6fb1628e732"
                    ),
                    "assets": [{"name": "pipe-linux-x86_64-v0.1.7.tar.gz"}],
                }
            ],
        )
        release = self.reader.release_state("v0.1.7", prefer_draft=True)
        assert release is not None
        self.assertTrue(release.draft)
        self.assertEqual(
            release.asset_names, ("pipe-linux-x86_64-v0.1.7.tar.gz",)
        )
        self.assertFalse(release.complete)
        self.assertEqual(_Handler.mutations, [])
        self.assertTrue(
            any(path.startswith("/repos/") and "/releases?" in path for path in _Handler.gets)
        )
        self.assertFalse(any("/releases/tags/" in path for path in _Handler.gets))

    def test_complete_release_matches_expected_names(self) -> None:
        names = expected_asset_names("v0.1.6")
        self.assertEqual(len(names), 16)
        _Handler.routes[
            "/repos/portable-pipelines/portable-pipelines/releases?per_page=100"
        ] = (
            200,
            [
                {
                    "tag_name": "v0.1.6",
                    "draft": False,
                    "prerelease": False,
                    "assets": [{"name": name} for name in names],
                }
            ],
        )
        release = self.reader.release_state("v0.1.6")
        assert release is not None
        self.assertTrue(release.complete)
        self.assertFalse(any("/releases/tags/" in path for path in _Handler.gets))

    def test_untagged_draft_url_is_found_when_tags_endpoint_404s(self) -> None:
        names = expected_asset_names("v0.2.1")
        html = (
            "https://github.com/portable-pipelines/portable-pipelines/"
            "releases/tag/untagged-cef0d67be6fb1628e732"
        )
        _Handler.routes[
            "/repos/portable-pipelines/portable-pipelines/releases?per_page=100"
        ] = (
            200,
            [
                {
                    "tag_name": "v0.2.0",
                    "draft": False,
                    "html_url": (
                        "https://github.com/portable-pipelines/"
                        "portable-pipelines/releases/tag/v0.2.0"
                    ),
                    "assets": [{"name": n} for n in expected_asset_names("v0.2.0")],
                },
                {
                    "tag_name": "v0.2.1",
                    "draft": True,
                    "prerelease": False,
                    "html_url": html,
                    "assets": [{"name": n} for n in names],
                },
            ],
        )
        _Handler.routes[
            "/repos/portable-pipelines/portable-pipelines/releases/tags/v0.2.1"
        ] = (404, {"message": "Not Found"})
        release = self.reader.release_state("v0.2.1", prefer_draft=True)
        assert release is not None
        self.assertTrue(release.draft)
        self.assertTrue(release.complete)
        self.assertTrue("untagged-" in html)
        self.assertFalse(any("/releases/tags/" in path for path in _Handler.gets))
        args = Namespace(tag="v0.2.1", require_draft=True, reader=self.reader)
        self.assertEqual(cmd_verify_assets(args), 0)

    def test_list_pagination_finds_draft_on_later_page(self) -> None:
        names = expected_asset_names("v0.2.1")
        host, port = self.server.server_address
        page1 = f"/repos/portable-pipelines/portable-pipelines/releases?per_page=100"
        page2 = f"/repos/portable-pipelines/portable-pipelines/releases?per_page=100&page=2"
        _Handler.routes[page1] = (
            200,
            [{"tag_name": "v0.1.6", "draft": False, "assets": []}],
            {
                "Link": (
                    f"<http://{host}:{port}{page2}>; rel=\"next\", "
                    f"<http://{host}:{port}{page2}>; rel=\"last\""
                )
            },
        )
        _Handler.routes[page2] = (
            200,
            [
                {
                    "tag_name": "v0.2.1",
                    "draft": True,
                    "html_url": (
                        "https://github.com/portable-pipelines/"
                        "portable-pipelines/releases/tag/untagged-abc123"
                    ),
                    "assets": [{"name": n} for n in names],
                }
            ],
        )
        release = self.reader.release_state("v0.2.1", prefer_draft=True)
        assert release is not None
        self.assertTrue(release.draft)
        self.assertTrue(release.complete)
        self.assertIn(page1, _Handler.gets)
        self.assertIn(page2, _Handler.gets)

    def test_prefer_draft_over_published_same_tag(self) -> None:
        _Handler.routes[
            "/repos/portable-pipelines/portable-pipelines/releases?per_page=100"
        ] = (
            200,
            [
                {
                    "tag_name": "v0.2.1",
                    "draft": False,
                    "assets": [{"name": "pipe-windows-x64.zip"}],
                },
                {
                    "tag_name": "v0.2.1",
                    "draft": True,
                    "html_url": (
                        "https://github.com/portable-pipelines/"
                        "portable-pipelines/releases/tag/untagged-draftonly"
                    ),
                    "assets": [{"name": "pipe-linux-x86_64.tar.gz"}],
                },
            ],
        )
        published = self.reader.release_state("v0.2.1", prefer_draft=False)
        draft = self.reader.release_state("v0.2.1", prefer_draft=True)
        assert published is not None and draft is not None
        self.assertFalse(published.draft)
        self.assertEqual(published.asset_names, ("pipe-windows-x64.zip",))
        self.assertTrue(draft.draft)
        self.assertEqual(draft.asset_names, ("pipe-linux-x86_64.tar.gz",))

    def test_next_page_path_strips_api_prefix(self) -> None:
        header = (
            '<https://api.github.com/repos/o/r/releases?page=2>; rel="next", '
            '<https://api.github.com/repos/o/r/releases?page=5>; rel="last"'
        )
        self.assertEqual(
            next_page_path(header, "https://api.github.com"),
            "/repos/o/r/releases?page=2",
        )

    def test_workflow_config_skips_complete_published_release(self) -> None:
        names = expected_asset_names("v0.2.1")
        sha = "c" * 40

        class _Complete:
            def release_state(self, tag: str, *, prefer_draft: bool = False):
                return GitHubReleaseState(
                    tag=tag,
                    sha=sha,
                    exists=True,
                    draft=False,
                    prerelease=False,
                    asset_names=tuple(names),
                )

        args = Namespace(
            event_name="release",
            sha=sha,
            release_tag="v0.2.1",
            github_output=None,
            reader=_Complete(),
        )
        import io
        from contextlib import redirect_stdout

        buf = io.StringIO()
        with redirect_stdout(buf):
            self.assertEqual(cmd_workflow_config(args), 0)
        out = buf.getvalue()
        self.assertIn("skip_package=true", out)
        self.assertIn("upload_release=false", out)
        self.assertIn("run_unix=false", out)
        self.assertIn("skipping rebuild", out)

    def test_workflow_config_does_not_skip_incomplete_published(self) -> None:
        sha = "c" * 40

        class _Incomplete:
            def release_state(self, tag: str, *, prefer_draft: bool = False):
                return GitHubReleaseState(
                    tag=tag,
                    sha=sha,
                    exists=True,
                    draft=False,
                    prerelease=False,
                    asset_names=("pipe-windows-x64.zip",),
                )

        args = Namespace(
            event_name="release",
            sha=sha,
            release_tag="v0.2.1",
            github_output=None,
            reader=_Incomplete(),
        )
        import io
        from contextlib import redirect_stdout

        buf = io.StringIO()
        with redirect_stdout(buf):
            self.assertEqual(cmd_workflow_config(args), 0)
        out = buf.getvalue()
        self.assertIn("skip_package=false", out)
        self.assertIn("upload_release=true", out)

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
