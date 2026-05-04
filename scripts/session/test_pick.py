"""Behavior-lock unit tests for scripts/session/pick.py helpers."""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from pick import Failure, resolve_test_source  # noqa: E402


def make_failure(path: str) -> Failure:
    return Failure(path=path, expected=[], actual=[], missing=[], extra=[])


class TestResolveTestSource(unittest.TestCase):
    """`resolve_test_source` finds the test on disk regardless of how the
    snapshot recorded the path. The snapshot stores absolute paths from the
    machine that produced it (e.g. `/tmp/tsz-snap-refresh/TypeScript/...`),
    so a naive `root / failure.path` join fails on every other machine."""

    def test_resolves_via_typescript_segment(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "TypeScript" / "tests" / "cases" / "compiler" / "x.ts"
            target.parent.mkdir(parents=True)
            target.write_text("export {};\n")

            failure = make_failure(
                "/tmp/tsz-snap-refresh/TypeScript/tests/cases/compiler/x.ts"
            )
            resolved = resolve_test_source(root, failure)
            self.assertEqual(resolved, target)

    def test_resolves_relative_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "TypeScript" / "tests" / "cases" / "conformance" / "y.ts"
            target.parent.mkdir(parents=True)
            target.write_text("\n")

            failure = make_failure("TypeScript/tests/cases/conformance/y.ts")
            resolved = resolve_test_source(root, failure)
            self.assertEqual(resolved, target)

    def test_resolves_absolute_existing_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "absolute.ts"
            target.write_text("\n")
            other = Path(tempfile.mkdtemp())
            failure = make_failure(str(target))
            resolved = resolve_test_source(other, failure)
            self.assertEqual(resolved, target)

    def test_returns_none_when_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            failure = make_failure(
                "/tmp/tsz-snap-refresh/TypeScript/tests/cases/compiler/missing.ts"
            )
            self.assertIsNone(resolve_test_source(Path(tmp), failure))


if __name__ == "__main__":
    unittest.main()
