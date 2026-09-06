"""Release authorization failures must stop before publication can begin."""

import copy
import json
import unittest
from unittest.mock import patch

import publish
import release

COMMIT = "a" * 40
OTHER = "b" * 40


class PublicationGate(unittest.TestCase):
    def setUp(self) -> None:
        self.responses = {
            f"repos/{release.REPOSITORY}/git/ref/heads/shrek": {
                "object": {"sha": COMMIT}
            },
            f"repos/{release.REPOSITORY}/actions/runs/123": {
                "head_sha": COMMIT,
                "conclusion": "success",
                "path": ".github/workflows/release-candidate.yml",
                "head_repository": {"full_name": release.REPOSITORY},
                "event": "push",
            },
            f"repos/{release.REPOSITORY}/immutable-releases": {"enabled": True},
            f"repos/{release.REPOSITORY}/git/ref/tags/v1.0.0": {
                "object": {"type": "tag", "sha": "c" * 40}
            },
            f"repos/{release.REPOSITORY}/git/tags/{'c' * 40}": {
                "object": {"type": "commit", "sha": COMMIT},
                "verification": {"verified": True},
            },
        }
        self.checks = [{"conclusion": "success", "event": "push", "headSha": COMMIT}]
        self.status = ""
        self.addCleanup(patch.stopall)
        patch.object(release, "version", return_value="1.0.0").start()
        patch.object(release, "run", side_effect=self.git).start()
        patch.object(publish, "gh", side_effect=self.github).start()

    def git(self, *args: str, **_kwargs: object) -> str:
        if args == ("git", "status", "--porcelain"):
            return self.status
        if args == ("git", "rev-parse", "HEAD"):
            return COMMIT
        self.fail(f"unexpected command: {args}")

    def github(self, *args: str) -> str:
        if args[:2] == ("run", "list"):
            return json.dumps(self.checks)
        self.assertEqual(args[0], "api")
        return json.dumps(self.responses[args[1]])

    def test_exact_checked_commit_and_signed_tag_pass(self) -> None:
        publish.preflight("v1.0.0", COMMIT, "123", publishing=True)

    def test_wrong_candidate_commit_workflow_event_or_outcome_is_rejected(self) -> None:
        key = f"repos/{release.REPOSITORY}/actions/runs/123"
        original = copy.deepcopy(self.responses[key])
        for field, value in (
            ("head_sha", OTHER),
            ("conclusion", "failure"),
            ("path", ".github/workflows/ci.yml"),
            ("event", "pull_request"),
            ("head_repository", {"full_name": "someone/fork"}),
        ):
            with self.subTest(field=field):
                self.responses[key] = {**original, field: value}
                with self.assertRaisesRegex(ValueError, "candidate run did not verify"):
                    publish.preflight("v1.0.0", COMMIT, "123", publishing=True)

    def test_branch_drift_dirty_checkout_and_failed_ci_stop_publication(self) -> None:
        self.status = " M Cargo.toml"
        with self.assertRaisesRegex(ValueError, "checkout must be clean"):
            publish.preflight("v1.0.0", COMMIT, "123", publishing=True)
        self.status = ""
        key = f"repos/{release.REPOSITORY}/git/ref/heads/shrek"
        self.responses[key]["object"]["sha"] = OTHER
        with self.assertRaisesRegex(ValueError, "shrek moved"):
            publish.preflight("v1.0.0", COMMIT, "123", publishing=True)
        self.responses[key]["object"]["sha"] = COMMIT
        self.checks = [{"conclusion": "failure", "event": "push", "headSha": COMMIT}]
        with self.assertRaisesRegex(ValueError, "CI did not pass"):
            publish.preflight("v1.0.0", COMMIT, "123", publishing=True)

    def test_mutable_release_setting_or_unverified_tag_is_rejected(self) -> None:
        key = f"repos/{release.REPOSITORY}/immutable-releases"
        self.responses[key]["enabled"] = False
        with self.assertRaisesRegex(ValueError, "immutability must be enabled"):
            publish.preflight("v1.0.0", COMMIT, "123", publishing=True)
        self.responses[key]["enabled"] = True
        self.responses[f"repos/{release.REPOSITORY}/git/tags/{'c' * 40}"][
            "verification"
        ]["verified"] = False
        with self.assertRaisesRegex(ValueError, "tag signature or commit"):
            publish.preflight("v1.0.0", COMMIT, "123", publishing=True)


if __name__ == "__main__":
    unittest.main()
