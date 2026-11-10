#!/usr/bin/env python3
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from assets import expected_asset_names
from intent import DRAFT_MARKER, INTENT_RELATIVE_PATH, load_intent
from prepare_release import main
from versioning import (
    parse_semver,
    set_lockfile_workspace_version,
    set_workspace_version,
    workspace_members,
    workspace_version,
)


def _workspace_version() -> str:
    return workspace_version((REPO / "Cargo.toml").read_text(encoding="utf-8"))


def _next_patch(version: str) -> str:
    parts = parse_semver(version)
    return f"{parts.major}.{parts.minor}.{parts.patch + 1}"

SCRIPT = Path(__file__).resolve()
REPO = SCRIPT.parent.parent.parent
HELPER = SCRIPT.parent / "prepare_release.py"


class PrepareCliTests(unittest.TestCase):
    def test_collect_since_v0_1_6(self) -> None:
        code = main(["--repo-root", str(REPO), "collect", "--since", "v0.1.6"])
        self.assertEqual(code, 0)

    def test_check_without_intent_is_ok(self) -> None:
        # No intent: ordinary commits do not publish. A live intent file on a
        # release-intent PR must still pass check.
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
        if (REPO / INTENT_RELATIVE_PATH).is_file():
            self.assertEqual(code, 0)
        else:
            self.assertEqual(code, 1)

    def test_prepare_dry_run_does_not_write(self) -> None:
        from collect import latest_published_tag

        # Live intent / a v0.2.0 git tag makes `prepare --since v0.1.6` refuse
        # because Cargo.toml is already 0.2.0.
        if (REPO / INTENT_RELATIVE_PATH).is_file():
            self.skipTest("workspace already bumped for a live release intent")
        current = _workspace_version()
        if f"v{current}" != latest_published_tag(REPO):
            self.skipTest("workspace already bumped for a live release intent")
        cargo_before = (REPO / "Cargo.toml").read_text(encoding="utf-8")
        code = main(
            [
                "--repo-root",
                str(REPO),
                "prepare",
                "--version",
                _next_patch(_workspace_version()),
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
        would = _next_patch(_workspace_version())
        self.assertFalse((REPO / f"docs/release-notes-v{would}.md").exists())

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

    def test_workflow_config_cli_push_from_intent_call(self) -> None:
        sha = "a" * 40
        env = os.environ.copy()
        env["EVENT_NAME"] = "push"
        env["GITHUB_SHA_VALUE"] = sha
        env["CALL_CALLER"] = "intent"
        env["CALL_TAG"] = "v0.2.0"
        env["CALL_CHECKOUT_REF"] = sha
        env["CALL_UPLOAD_RELEASE"] = "true"
        env["CALL_SIGN"] = "true"
        env["CALL_STABLE_ALIASES"] = "true"
        env["CALL_WINDOWS_ONLY"] = "false"
        completed = subprocess.run(
            ["python3", str(HELPER), "workflow-config"],
            cwd=REPO,
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        self.assertIn("tag=v0.2.0", completed.stdout)
        self.assertIn("upload_release=true", completed.stdout)
        self.assertIn("sign=true", completed.stdout)
        self.assertIn(f"checkout_ref={sha}", completed.stdout)

    def test_current_workspace_is_still_0_1_6(self) -> None:
        toml = (REPO / "Cargo.toml").read_text(encoding="utf-8")
        ver = workspace_version(toml)
        intent_path = REPO / INTENT_RELATIVE_PATH
        if intent_path.is_file():
            self.assertEqual(ver, load_intent(intent_path).version)
        else:
            self.assertEqual(ver, "0.1.6")

    def test_plan_offline_with_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for relative in ("Cargo.toml", "Cargo.lock", "CHANGELOG.md"):
                (root / relative).write_text(
                    (REPO / relative).read_text(encoding="utf-8"), encoding="utf-8"
                )
            # Intent version must match the copied workspace files.
            ver = workspace_version((root / "Cargo.toml").read_text(encoding="utf-8"))
            notes_name = f"release-notes-v{ver}.md"
            notes_src = REPO / "docs" / notes_name
            if not notes_src.is_file():
                notes_src = REPO / "docs/release-notes-v0.1.6.md"
            intent = {
                "schema_version": 1,
                "publish": True,
                "version": ver,
                "tag": f"v{ver}",
                "notes_file": f"docs/{notes_name}",
            }
            (root / ".github").mkdir()
            (root / ".github" / "release-intent.json").write_text(
                json.dumps(intent, indent=2) + "\n", encoding="utf-8"
            )
            (root / "docs").mkdir()
            (root / "docs" / notes_name).write_text(
                notes_src.read_text(encoding="utf-8"),
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
            ver = workspace_version((root / "Cargo.toml").read_text(encoding="utf-8"))
            intent = {
                "schema_version": 1,
                "publish": True,
                "version": ver,
                "tag": f"v{ver}",
                "notes_file": f"docs/release-notes-v{ver}.md",
            }
            (root / ".github").mkdir()
            (root / ".github" / "release-intent.json").write_text(
                json.dumps(intent) + "\n", encoding="utf-8"
            )
            (root / "docs").mkdir()
            (root / "docs" / f"release-notes-v{ver}.md").write_text(
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
            # This fixture prepares 0.1.7 from a v0.1.6 tag, so the copied
            # workspace files must be 0.1.6 even when the live tree is newer.
            toml = (root / "Cargo.toml").read_text(encoding="utf-8")
            lock = (root / "Cargo.lock").read_text(encoding="utf-8")
            members = workspace_members(toml)
            (root / "Cargo.toml").write_text(
                set_workspace_version(toml, "0.1.6"), encoding="utf-8"
            )
            (root / "Cargo.lock").write_text(
                set_lockfile_workspace_version(lock, members, "0.1.6"),
                encoding="utf-8",
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
