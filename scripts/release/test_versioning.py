#!/usr/bin/env python3
import unittest
from datetime import date

from errors import ReleaseError
from versioning import (
    assert_versions_consistent,
    move_unreleased_changelog,
    set_lockfile_workspace_version,
    set_workspace_version,
    workspace_members,
    workspace_version,
)

CARGO_TOML = """[workspace]
resolver = "2"
members = [
    "crates/pipe-core",
    "crates/pipe-cli",
]

[workspace.package]
version = "0.1.6"
edition = "2021"
"""

CARGO_LOCK = """[[package]]
name = "anyhow"
version = "1.0.104"
source = "registry"

[[package]]
name = "pipe-cli"
version = "0.1.6"
dependencies = [
 "anyhow",
]

[[package]]
name = "pipe-core"
version = "0.1.6"
"""

CHANGELOG = """# Changelog

## [Unreleased]

Added:

- A new thing.

## [0.1.6] - 2026-11-05

Old news.

[0.1.6]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.5...v0.1.6
"""


class VersioningTests(unittest.TestCase):
    def test_workspace_version_and_members(self) -> None:
        self.assertEqual(workspace_version(CARGO_TOML), "0.1.6")
        self.assertEqual(workspace_members(CARGO_TOML), ["pipe-core", "pipe-cli"])

    def test_bump_toml_and_lock_leaves_third_party(self) -> None:
        toml = set_workspace_version(CARGO_TOML, "0.1.7")
        self.assertEqual(workspace_version(toml), "0.1.7")
        lock = set_lockfile_workspace_version(
            CARGO_LOCK, ["pipe-core", "pipe-cli"], "0.1.7"
        )
        self.assertIn('name = "pipe-cli"\nversion = "0.1.7"', lock)
        self.assertIn('name = "pipe-core"\nversion = "0.1.7"', lock)
        self.assertIn('name = "anyhow"\nversion = "1.0.104"', lock)

    def test_changelog_moves_unreleased(self) -> None:
        updated = move_unreleased_changelog(
            CHANGELOG, "0.1.7", date(2026, 11, 7), "0.1.6"
        )
        self.assertIn("## [Unreleased]\n\n## [0.1.7] - 2026-11-07\n\nAdded:", updated)
        self.assertIn("- A new thing.", updated.split("## [0.1.7]")[1])
        self.assertIn(
            "[0.1.7]: https://github.com/portable-pipelines/portable-pipelines/compare/v0.1.6...v0.1.7",
            updated,
        )
        self.assertTrue(updated.index("[0.1.7]:") < updated.index("[0.1.6]:"))

    def test_consistency_ok_after_bump(self) -> None:
        toml = set_workspace_version(CARGO_TOML, "0.1.7")
        lock = set_lockfile_workspace_version(
            CARGO_LOCK, ["pipe-core", "pipe-cli"], "0.1.7"
        )
        changelog = move_unreleased_changelog(
            CHANGELOG, "0.1.7", date(2026, 11, 7), "0.1.6"
        )
        assert_versions_consistent(
            cargo_toml=toml,
            cargo_lock=lock,
            changelog=changelog,
            version="0.1.7",
        )

    def test_consistency_rejects_lock_mismatch(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "Cargo.lock"):
            assert_versions_consistent(
                cargo_toml=set_workspace_version(CARGO_TOML, "0.1.7"),
                cargo_lock=CARGO_LOCK,
                changelog=move_unreleased_changelog(
                    CHANGELOG, "0.1.7", date(2026, 11, 7), "0.1.6"
                ),
                version="0.1.7",
            )


if __name__ == "__main__":
    unittest.main()
