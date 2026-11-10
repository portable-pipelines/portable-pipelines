#!/usr/bin/env python3
"""Safety checks on the workflow files that do not require YAML libraries."""

from __future__ import annotations

import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
RELEASE = (REPO / ".github/workflows/release.yml").read_text(encoding="utf-8").replace(
    "\r\n", "\n"
)
INTENT = (
    (REPO / ".github/workflows/release-from-intent.yml")
    .read_text(encoding="utf-8")
    .replace("\r\n", "\n")
)
CI = (REPO / ".github/workflows/ci.yml").read_text(encoding="utf-8").replace(
    "\r\n", "\n"
)


class WorkflowSafetyTests(unittest.TestCase):
    def test_no_live_intent_file_in_tree(self) -> None:
        path = REPO / ".github/release-intent.json"
        if path.is_file():
            # A dedicated release-intent PR commits this file on purpose.
            # Ordinary work must not. `prepare_release.py check` is what
            # CI runs to prove the live file is consistent.
            from prepare_release import main

            self.assertEqual(main(["--repo-root", str(REPO), "check"]), 0)
            return
        self.assertFalse(path.exists())

    def test_intent_workflow_only_watches_intent_path(self) -> None:
        self.assertIn(".github/release-intent.json", INTENT)
        self.assertIn("branches:\n      - master", INTENT)
        self.assertIn("pull_request:", INTENT)
        self.assertNotIn("workflow_dispatch:", INTENT)
        self.assertNotIn("repository_dispatch:", INTENT)

    def test_intent_workflow_does_not_publish_on_pull_request(self) -> None:
        self.assertIn("github.event_name == 'push'", INTENT)
        self.assertIn("refs/heads/master", INTENT)
        self.assertIn("needs.validate.outputs.do_release == 'true'", INTENT)

    def test_github_token_recursion_is_documented_and_reusable_call_exists(self) -> None:
        self.assertIn("GITHUB_TOKEN", INTENT)
        self.assertIn("workflow_call", RELEASE)
        self.assertIn("uses: ./.github/workflows/release.yml", INTENT)
        self.assertIn("caller: intent", INTENT)
        self.assertIn("CALL_CALLER", RELEASE)
        self.assertIn("caller:", RELEASE)
        self.assertIn("gh release edit \"${TAG}\" --draft=false --latest", INTENT)
        self.assertIn("skip_package", RELEASE)
        self.assertIn("untagged-*", INTENT)
        api = (REPO / "scripts/release/github_api.py").read_text(encoding="utf-8")
        self.assertNotIn("/releases/tags/{encoded}", api)
        self.assertIn("list_releases", api)

    def test_signing_environment_and_attestations_stay_on_packaging_workflow(self) -> None:
        self.assertIn("environment: signing", RELEASE)
        self.assertIn("azure/artifact-signing-action@v2", RELEASE)
        self.assertIn("actions/attest-build-provenance@v3", RELEASE)
        self.assertIn("id-token: write", RELEASE)
        self.assertIn("attestations: write", RELEASE)
        self.assertIn("pipe-windows-x64.zip", RELEASE)
        self.assertIn("pipe-linux-x86_64.tar.gz", RELEASE)
        self.assertEqual(RELEASE.count("if: needs.config.outputs.sign == 'true'"), 3)
        self.assertEqual(
            RELEASE.count("if: needs.config.outputs.upload_release == 'true'"), 2
        )
        self.assertNotIn("if: github.event_name != 'pull_request'", RELEASE)
        self.assertNotIn("if: github.event_name == 'release'", RELEASE)

    def test_pull_request_and_dispatch_still_do_not_upload_in_release_yml(self) -> None:
        self.assertIn("pull_request:", RELEASE)
        self.assertIn("workflow_dispatch:", RELEASE)
        self.assertIn("release:\n    types: [published]", RELEASE)

    def test_concurrency_does_not_cancel_in_progress_release(self) -> None:
        self.assertIn("cancel-in-progress: false", INTENT)
        self.assertIn("cancel-in-progress: false", RELEASE)

    def test_ci_runs_release_helper_tests(self) -> None:
        self.assertIn("scripts/release", CI)
        self.assertIn("prepare_release.py", CI)

    def test_no_force_push_or_tag_delete(self) -> None:
        for text in (RELEASE, INTENT):
            self.assertNotIn("git push --force", text)
            self.assertNotIn("git tag -d", text)
            self.assertNotIn("gh release delete", text)
            self.assertNotIn("--force", text)


if __name__ == "__main__":
    unittest.main()
