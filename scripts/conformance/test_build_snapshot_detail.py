import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("build-snapshot-detail.py")
SPEC = importlib.util.spec_from_file_location("build_snapshot_detail", SCRIPT_PATH)
build_snapshot_detail = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = build_snapshot_detail
SPEC.loader.exec_module(build_snapshot_detail)


class BuildSnapshotDetailTests(unittest.TestCase):
    def test_partitions_candidates_and_preserves_unsupported_reason(self):
        tests = {
            "pass.ts": {
                "status": "PASS",
                "expected": [],
                "actual": [],
            },
            "fail.ts": {
                "status": "FAIL",
                "expected": ["TS2322"],
                "actual": [],
            },
            "unsupported.ts": {
                "status": "UNSUPPORTED",
                "expected": [],
                "actual": [],
                "unsupported_reason": "typescript-7-unsupported-configuration",
            },
            "skip.ts": {
                "status": "SKIP",
                "expected": [],
                "actual": [],
            },
        }

        detail = build_snapshot_detail.build_snapshot_detail(tests)

        self.assertEqual(
            detail["summary"],
            {
                "candidates": 4,
                "total": 2,
                "runnable": 2,
                "passed": 1,
                "failed": 1,
                "unsupported": 1,
                "skipped": 1,
                "crashed": 0,
                "timeout": 0,
                "known_failures": 0,
            },
        )
        self.assertEqual(
            detail["unsupported"],
            {
                "unsupported.ts": {
                    "reason": "typescript-7-unsupported-configuration"
                }
            },
        )
        self.assertEqual(set(detail["failures"]), {"fail.ts"})
        self.assertEqual(detail["terminal_failures"], {})

    def test_preserves_crash_and_timeout_identities(self):
        tests = {
            "crash.ts": {"status": "CRASH", "expected": [], "actual": []},
            "slow.ts": {"status": "TIMEOUT", "expected": [], "actual": []},
        }
        provenance = {"git": {"dirty": False}}
        detail = build_snapshot_detail.build_snapshot_detail(
            tests, provenance=provenance
        )

        self.assertEqual(detail["summary"]["crashed"], 1)
        self.assertEqual(detail["summary"]["timeout"], 1)
        self.assertEqual(
            detail["terminal_failures"],
            {
                "crash.ts": {"status": "CRASH"},
                "slow.ts": {"status": "TIMEOUT"},
            },
        )
        self.assertEqual(detail["provenance"], provenance)

    def test_git_sha_is_stamped_when_provided(self):
        tests = {"pass.ts": {"status": "PASS", "expected": [], "actual": []}}

        detail = build_snapshot_detail.build_snapshot_detail(
            tests, git_sha="0123456789abcdef0123456789abcdef01234567"
        )

        self.assertEqual(
            detail["git_sha"], "0123456789abcdef0123456789abcdef01234567"
        )

    def test_unknown_git_sha_is_not_stamped(self):
        tests = {"pass.ts": {"status": "PASS", "expected": [], "actual": []}}

        for value in (None, "unknown", "UNKNOWN", ""):
            detail = build_snapshot_detail.build_snapshot_detail(
                tests, git_sha=value
            )
            self.assertNotIn("git_sha", detail)


if __name__ == "__main__":
    unittest.main()
