#!/usr/bin/env python3
import unittest

from errors import ReleaseError
from workflow_config import resolve_packaging_mode

SHA = "c" * 40


class WorkflowConfigTests(unittest.TestCase):
    def test_pull_request_is_unsigned_windows_only_no_upload(self) -> None:
        mode = resolve_packaging_mode(event_name="pull_request", sha=SHA)
        self.assertFalse(mode.upload_release)
        self.assertFalse(mode.sign)
        self.assertFalse(mode.stable_aliases)
        self.assertFalse(mode.run_unix)
        self.assertEqual(mode.tag, "")

    def test_workflow_dispatch_signs_but_does_not_publish(self) -> None:
        mode = resolve_packaging_mode(
            event_name="workflow_dispatch",
            sha=SHA,
            dispatch_windows_only="false",
        )
        self.assertFalse(mode.upload_release)
        self.assertTrue(mode.sign)
        self.assertFalse(mode.stable_aliases)
        self.assertTrue(mode.run_unix)

    def test_workflow_dispatch_windows_only(self) -> None:
        mode = resolve_packaging_mode(
            event_name="workflow_dispatch",
            sha=SHA,
            dispatch_windows_only="true",
        )
        self.assertFalse(mode.run_unix)
        self.assertTrue(mode.sign)
        self.assertFalse(mode.upload_release)

    def test_published_release_uploads_stable_names(self) -> None:
        mode = resolve_packaging_mode(
            event_name="release",
            sha=SHA,
            release_tag="v0.1.6",
        )
        self.assertEqual(mode.tag, "v0.1.6")
        self.assertTrue(mode.upload_release)
        self.assertTrue(mode.sign)
        self.assertTrue(mode.stable_aliases)
        self.assertTrue(mode.run_unix)

    def test_workflow_call_from_intent(self) -> None:
        mode = resolve_packaging_mode(
            event_name="workflow_call",
            sha=SHA,
            call_tag="v0.1.7",
            call_checkout_ref=SHA,
            call_upload_release="true",
            call_sign="true",
            call_stable_aliases="true",
            call_windows_only="false",
        )
        self.assertEqual(mode.tag, "v0.1.7")
        self.assertEqual(mode.checkout_ref, SHA)
        self.assertTrue(mode.upload_release)
        self.assertTrue(mode.sign)
        self.assertTrue(mode.stable_aliases)
        self.assertTrue(mode.run_unix)

    def test_workflow_call_rejects_non_sha_checkout(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "40-character"):
            resolve_packaging_mode(
                event_name="workflow_call",
                sha=SHA,
                call_tag="v0.1.7",
                call_checkout_ref="master",
                call_upload_release="true",
            )

    def test_workflow_call_upload_requires_tag(self) -> None:
        with self.assertRaisesRegex(ReleaseError, "requires a vX.Y.Z tag"):
            resolve_packaging_mode(
                event_name="workflow_call",
                sha=SHA,
                call_upload_release="true",
            )


if __name__ == "__main__":
    unittest.main()
