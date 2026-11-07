#!/usr/bin/env python3
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from assets import expected_asset_names
from intent import DRAFT_MARKER, INTENT_RELATIVE_PATH
from prepare_release import main
from versioning import workspace_version

SCRIPT = Path(__file__).resolve()
REPO = SCRIPT.parent.parent.parent
HELPER = SCRIPT.parent / "prepare_release.py"


class PrepareCliTests(unittest.TestCase):
    def test_collect_since_v0_1_6(self) -> None:
        code = main(["--repo-root", str(REPO), "collect", "--since", "v0.1.6"])
        self.assertEqual(code, 0)

    def test_check_without_intent_is_ok(self) -> None:
        self.assertFalse((REPO / INTENT_RELATIVE_PATH).exists())
        code = main(["--repo-root", str(REPO), "check"])
        self.assertEqual(code, 0)

    def test_plan_without_intent_is_no_op(self) -> None:
        code = main(
            [
                "--repo-root",
                str(REPO),
                "plan",
                "--offline",
                "--sha",
                "a" * 40,
                "--event-name",
                "push",
                "--ref",
                "refs/heads/master",
            ]
        )
        self.assertEqual(code, 0)

    def test_check_require_intent_fails_on_master_tree(self) -> None:
        code = main(["--repo-root", str(REPO), "check", "--require-intent"])
        self.assertEqual(code, 1)

    def test_prepare_dry_run_does_not_write(self) -> None:
        cargo_before = (REPO / "Cargo.toml").read_text(encoding="utf-8")
        code = main(
            [
                "--repo-root",
                str(REPO),
                "prepare",
                "--version",
                "0.1.7",
                "--since",
                "v0.1.6",
                "--date",
                "2026-11-07",
                "--dry-run",
            ]
        )
        self.assertEqual(code, 0)
        self.assertEqual(
            (REPO / "Cargo.toml").read_text(encoding="utf-8"), cargo_before
        )
        self.assertFalse((REPO / INTENT_RELATIVE_PATH).exists())
        self.assertFalse((REPO / "docs/release-notes-v0.1.7.md").exists())

    def test_prepare_and_plan_offline_in_temp_clone_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for relative in ("Cargo.toml", "Cargo.lock", "CHANGELOG.md"):
                target = root / relative
                target.write_text((REPO / relative).read_text(encoding="utf-8"), encoding="utf-8")
            (root / "docs").mkdir()
            (root / ".github").mkdir()
            code = main(
                [
                    "--repo-root",
                    str(root),
                    "prepare",
                    "--version",
                    "0.1.7",
                    "--since",
                    "v0.1.6",
                    "--date",
                    "2026-11-07",
                ]
            )
            # prepare needs git tags for --since default, but we passed --since.
            # It also calls commits_since which needs git. Expect failure without git.
            self.assertEqual(code, 1)

    def test_expected_assets_v0_1_6_shape(self) -> None:
        names = expected_asset_names("v0.1.6")
        self.assertIn("pipe-windows-x64-v0.1.6.zip", names)
        self.assertIn("pipe-windows-x64.zip", names)
        self.assertIn("pipe-linux-x86_64-v0.1.6.tar.gz", names)
        self.assertIn("pipe-linux-x86_64.tar.gz", names)
        self.assertIn("pipe-macos-arm64.tar.gz", names)
        self.assertIn("pipe-macos-x86_64.tar.gz.sha256", names)
        self.assertEqual(len(names), 16)

    def test_workflow_config_cli(self) -> None:
        env = os.environ.copy()
        env["EVENT_NAME"] = "pull_request"
        env["GITHUB_SHA_VALUE"] = "e" * 40
        completed = subprocess.run(
            ["python3", str(HELPER), "workflow-config"],
            cwd=REPO,
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertIn("upload_release=false", completed.stdout)
        self.assertIn("sign=false", completed.stdout)
        self.assertIn("run_unix=false", completed.stdout)

    def test_current_workspace_is_still_0_1_6(self) -> None:
        toml = (REPO / "Cargo.toml").read_text(encoding="utf-8")
        self.assertEqual(workspace_version(toml), "0.1.6")

    def test_plan_offline_with_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for relative in ("Cargo.toml", "Cargo.lock", "CHANGELOG.md"):
                (root / relative).write_text(
                    (REPO / relative).read_text(encoding="utf-8"), encoding="utf-8"
                )
            # Intentionally leave version at 0.1.6 so plan fails consistency
            # unless we write a matching intent for 0.1.6, which is already
            # published. Use a matching 0.1.6 intent plus notes.
            intent = {
                "schema_version": 1,
                "publish": True,
                "version": "0.1.6",
                "tag": "v0.1.6",
                "notes_file": "docs/release-notes-v0.1.6.md",
            }
            (root / ".github").mkdir()
            (root / ".github" / "release-intent.json").write_text(
                json.dumps(intent, indent=2) + "\n", encoding="utf-8"
            )
            (root / "docs").mkdir()
            (root / "docs" / "release-notes-v0.1.6.md").write_text(
                (REPO / "docs/release-notes-v0.1.6.md").read_text(encoding="utf-8"),
                encoding="utf-8",
            )
            code = main(
                [
                    "--repo-root",
                    str(root),
                    "plan",
                    "--offline",
                    "--sha",
                    "f" * 40,
                    "--event-name",
                    "pull_request",
                    "--ref",
                    "refs/pull/1/merge",
                ]
            )
            self.assertEqual(code, 0)

    def test_plan_refuses_draft_marker(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for relative in ("Cargo.toml", "Cargo.lock", "CHANGELOG.md"):
                (root / relative).write_text(
                    (REPO / relative).read_text(encoding="utf-8"), encoding="utf-8"
                )
            intent = {
                "schema_version": 1,
                "publish": True,
                "version": "0.1.6",
                "tag": "v0.1.6",
                "notes_file": "docs/release-notes-v0.1.6.md",
            }
            (root / ".github").mkdir()
            (root / ".github" / "release-intent.json").write_text(
                json.dumps(intent) + "\n", encoding="utf-8"
            )
            (root / "docs").mkdir()
            (root / "docs" / "release-notes-v0.1.6.md").write_text(
                f"{DRAFT_MARKER}\n", encoding="utf-8"
            )
            code = main(
                [
                    "--repo-root",
                    str(root),
                    "plan",
                    "--offline",
                    "--sha",
                    "f" * 40,
                    "--event-name",
                    "push",
                    "--ref",
                    "refs/heads/master",
                ]
            )
            self.assertEqual(code, 1)


class PrepareWritesTests(unittest.TestCase):
    def test_prepare_writes_intent_and_leaves_draft_marker(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(["git", "init"], cwd=root, check=True, capture_output=True)
            subprocess.run(
                ["git", "config", "user.email", "release-test@example.com"],
                cwd=root,
                check=True,
                capture_output=True,
            )
            subprocess.run(
                ["git", "config", "user.name", "Release Test"],
                cwd=root,
                check=True,
                capture_output=True,
            )
            for relative in ("Cargo.toml", "Cargo.lock", "CHANGELOG.md"):
                (root / relative).write_text(
                    (REPO / relative).read_text(encoding="utf-8"), encoding="utf-8"
                )
            (root / "docs").mkdir()
            (root / "README.md").write_text("test\n", encoding="utf-8")
            subprocess.run(["git", "add", "."], cwd=root, check=True, capture_output=True)
            subprocess.run(
                ["git", "commit", "-m", "base"],
                cwd=root,
                check=True,
                capture_output=True,
            )
            subprocess.run(["git", "tag", "v0.1.6"], cwd=root, check=True, capture_output=True)
            (root / "extra.txt").write_text("change\n", encoding="utf-8")
            subprocess.run(["git", "add", "extra.txt"], cwd=root, check=True, capture_output=True)
            subprocess.run(
                ["git", "commit", "-m", "A follow-up (#999)"],
                cwd=root,
                check=True,
                capture_output=True,
            )
            code = main(
                [
                    "--repo-root",
                    str(root),
                    "prepare",
                    "--version",
                    "0.1.7",
                    "--since",
                    "v0.1.6",
                    "--date",
                    "2026-11-07",
                ]
            )
            self.assertEqual(code, 0)
            intent_path = root / INTENT_RELATIVE_PATH
            self.assertTrue(intent_path.is_file())
            notes = (root / "docs/release-notes-v0.1.7.md").read_text(encoding="utf-8")
            self.assertIn(DRAFT_MARKER, notes)
            self.assertIn("#999", notes)
            changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
            self.assertIn("## [0.1.7] - 2026-11-07", changelog)
            from versioning import workspace_version

            self.assertEqual(
                workspace_version((root / "Cargo.toml").read_text(encoding="utf-8")),
                "0.1.7",
            )
            # Publishing must wait until the draft marker is gone.
            self.assertEqual(
                main(["--repo-root", str(root), "check", "--require-intent"]),
                1,
            )


class CollectGitTests(unittest.TestCase):
    def test_real_repo_commits_since_v0_1_6(self) -> None:
        completed = subprocess.run(
            [
                "python3",
                str(HELPER),
                "--repo-root",
                str(REPO),
                "collect",
                "--since",
                "v0.1.6",
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        self.assertIn("#114", completed.stdout)
        self.assertIn("#113", completed.stdout)
        self.assertIn("#112", completed.stdout)


if __name__ == "__main__":
    unittest.main()
