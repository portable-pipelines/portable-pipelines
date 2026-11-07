#!/usr/bin/env python3
import unittest

from assets import expected_asset_names
from decision import (
    ACTION_CREATE_DRAFT,
    ACTION_NO_OP,
    ACTION_RESUME_DRAFT,
    GitHubReleaseState,
    TagState,
    decide,
)
from errors import ReleaseError
from intent import parse_intent_text

SHA = "a" * 40
OTHER = "b" * 40
INTENT = parse_intent_text(
    """{
  "schema_version": 1,
  "publish": true,
  "version": "0.1.7",
  "tag": "v0.1.7",
  "notes_file": "docs/release-notes-v0.1.7.md"
}
"""
)
NO_PUBLISH = parse_intent_text(
    """{
  "schema_version": 1,
  "publish": false,
  "version": "0.1.7",
  "tag": "v0.1.7",
  "notes_file": "docs/release-notes-v0.1.7.md"
}
"""
)


def _decide(**kwargs):
    defaults = dict(
        intent=INTENT,
        commit_sha=SHA,
        event_name="push",
        ref="refs/heads/master",
        default_branch="master",
        tag=TagState(name="v0.1.7", sha=None, exists=False),
        release=None,
    )
    defaults.update(kwargs)
    return decide(**defaults)


class DecisionTests(unittest.TestCase):
    def test_create_draft_on_master_push(self) -> None:
        result = _decide()
        self.assertEqual(result.action, ACTION_CREATE_DRAFT)
        self.assertTrue(result.do_release)

    def test_pull_request_does_not_release(self) -> None:
        result = _decide(event_name="pull_request", ref="refs/pull/12/merge")
        self.assertEqual(result.action, ACTION_CREATE_DRAFT)
        self.assertFalse(result.do_release)

    def test_publish_false_never_releases(self) -> None:
        result = _decide(intent=NO_PUBLISH)
        self.assertEqual(result.action, ACTION_NO_OP)
        self.assertFalse(result.do_release)

    def test_feature_branch_push_does_not_release(self) -> None:
        result = _decide(ref="refs/heads/cursor/work")
        self.assertEqual(result.action, ACTION_NO_OP)
        self.assertFalse(result.do_release)

    def test_existing_tag_other_sha_refuses(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "already points"):
            _decide(tag=TagState(name="v0.1.7", sha=OTHER, exists=True))

    def test_published_complete_is_idempotent(self) -> None:
        names = tuple(expected_asset_names("v0.1.7"))
        result = _decide(
            tag=TagState(name="v0.1.7", sha=SHA, exists=True),
            release=GitHubReleaseState(
                tag="v0.1.7",
                sha=SHA,
                exists=True,
                draft=False,
                prerelease=False,
                asset_names=names,
            ),
        )
        self.assertEqual(result.action, ACTION_NO_OP)
        self.assertFalse(result.do_release)

    def test_published_incomplete_refuses(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "already published"):
            _decide(
                tag=TagState(name="v0.1.7", sha=SHA, exists=True),
                release=GitHubReleaseState(
                    tag="v0.1.7",
                    sha=SHA,
                    exists=True,
                    draft=False,
                    prerelease=False,
                    asset_names=("pipe-linux-x86_64-v0.1.7.tar.gz",),
                ),
            )

    def test_resume_draft_same_sha(self) -> None:
        result = _decide(
            tag=TagState(name="v0.1.7", sha=SHA, exists=True),
            release=GitHubReleaseState(
                tag="v0.1.7",
                sha=SHA,
                exists=True,
                draft=True,
                prerelease=False,
                asset_names=(),
            ),
        )
        self.assertEqual(result.action, ACTION_RESUME_DRAFT)
        self.assertTrue(result.do_release)

    def test_draft_other_sha_refuses(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "Refusing to retarget"):
            _decide(
                tag=TagState(name="v0.1.7", sha=SHA, exists=True),
                release=GitHubReleaseState(
                    tag="v0.1.7",
                    sha=OTHER,
                    exists=True,
                    draft=True,
                    prerelease=False,
                    asset_names=(),
                ),
            )


if __name__ == "__main__":
    unittest.main()
