"""Unit tests for ``scripts/conformance/link-regression-issues.py``.

These tests build a fake conformance snapshot directory in a temp location
and exercise the helper through its public functions. They do not require
the real checked-in artifacts.
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path


HELPER_PATH = Path(__file__).resolve().parent / "link-regression-issues.py"


def _load_helper():
    spec = importlib.util.spec_from_file_location("link_regression_issues", HELPER_PATH)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules["link_regression_issues"] = module
    spec.loader.exec_module(module)
    return module


helper = _load_helper()


def _build_fake_conformance_dir(
    base: Path,
    *,
    baseline_lines: list[str],
    accepted_lines: list[str],
    failures: dict[str, dict],
    timestamp: str = "2026-01-01T00:00:00Z",
    git_sha: str = "abcdef0",
    summary: dict | None = None,
) -> Path:
    base.mkdir(parents=True, exist_ok=True)
    (base / "conformance-baseline.txt").write_text("\n".join(baseline_lines) + "\n", encoding="utf-8")
    (base / "conformance-accepted-regressions.txt").write_text(
        "\n".join(accepted_lines) + "\n", encoding="utf-8"
    )
    (base / "conformance-detail.json").write_text(
        json.dumps({"failures": failures}), encoding="utf-8"
    )
    (base / "conformance-snapshot.json").write_text(
        json.dumps(
            {
                "timestamp": timestamp,
                "git_sha": git_sha,
                "summary": summary or {"total_tests": 3, "passed": 2, "failed": 1},
            }
        ),
        encoding="utf-8",
    )
    return base


class TestSnapshotIndex(unittest.TestCase):
    def test_baseline_pass_and_fail_indexed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = _build_fake_conformance_dir(
                Path(tmp),
                baseline_lines=[
                    "PASS TypeScript/tests/cases/compiler/passingExampleAlpha.ts",
                    "FAIL TypeScript/tests/cases/compiler/failingExampleBeta.tsx | expected:[TS2322] actual:[TS2345]",
                ],
                accepted_lines=[],
                failures={
                    "TypeScript/tests/cases/compiler/failingExampleBeta.tsx": {
                        "e": ["TS2322"],
                        "a": ["TS2345"],
                    }
                },
            )
            index = helper.load_snapshot_index(base)
            self.assertIn(
                "TypeScript/tests/cases/compiler/passingExampleAlpha.ts", index.baseline_pass
            )
            self.assertIn(
                "TypeScript/tests/cases/compiler/failingExampleBeta.tsx", index.baseline_fail
            )
            self.assertIn("passingExampleAlpha", index.basename_to_path)
            self.assertIn("failingExampleBeta", index.basename_to_path)

    def test_accepted_strips_comments_and_blanks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = _build_fake_conformance_dir(
                Path(tmp),
                baseline_lines=["PASS TypeScript/tests/cases/compiler/exampleAlpha.ts"],
                accepted_lines=[
                    "# header comment",
                    "",
                    "TypeScript/tests/cases/compiler/exampleAlpha.ts",
                    "  # indented comment",
                ],
                failures={},
            )
            index = helper.load_snapshot_index(base)
            self.assertEqual(
                index.accepted,
                {"TypeScript/tests/cases/compiler/exampleAlpha.ts"},
            )

    def test_missing_artifact_raises_system_exit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            # Intentionally empty: no artifacts present.
            with self.assertRaises(SystemExit):
                helper.load_snapshot_index(base)


class TestClassifyPath(unittest.TestCase):
    def _index(self, **kwargs):
        defaults = {
            "timestamp": "",
            "git_sha": "",
            "summary": {},
            "failures": set(),
            "accepted": set(),
            "baseline_pass": set(),
            "baseline_fail": set(),
            "basename_to_path": {},
        }
        defaults.update(kwargs)
        return helper.SnapshotIndex(**defaults)

    def test_failing_when_in_detail_failures(self) -> None:
        index = self._index(failures={"path/a.ts"})
        self.assertEqual(helper._classify_path("path/a.ts", index), "failing")

    def test_accepted_regression_when_in_both(self) -> None:
        index = self._index(failures={"path/a.ts"}, accepted={"path/a.ts"})
        self.assertEqual(helper._classify_path("path/a.ts", index), "accepted-regression")

    def test_stale_accepted_when_accepted_but_not_failing(self) -> None:
        index = self._index(accepted={"path/a.ts"})
        self.assertEqual(helper._classify_path("path/a.ts", index), "stale-accepted")

    def test_passing_when_in_baseline_pass(self) -> None:
        index = self._index(baseline_pass={"path/a.ts"})
        self.assertEqual(helper._classify_path("path/a.ts", index), "passing")

    def test_failing_fallback_for_baseline_fail_only(self) -> None:
        # Baseline marks the test as failing but the detail snapshot did not
        # record it (artifacts disagree). The conservative classification is
        # failing so an issue inspector still treats it as live.
        index = self._index(baseline_fail={"path/a.ts"})
        self.assertEqual(helper._classify_path("path/a.ts", index), "failing")

    def test_unknown_when_no_artifact_mentions_it(self) -> None:
        index = self._index()
        self.assertEqual(helper._classify_path("path/x.ts", index), "unknown")


class TestLooksLikeTestToken(unittest.TestCase):
    def test_test_extension_always_matches(self) -> None:
        self.assertTrue(helper._looks_like_test_token("emit.ts"))
        self.assertTrue(helper._looks_like_test_token("foo.tsx"))

    def test_typescript_path_always_matches(self) -> None:
        self.assertTrue(
            helper._looks_like_test_token("TypeScript/tests/cases/compiler/x.ts")
        )

    def test_mixed_case_long_token_matches(self) -> None:
        self.assertTrue(helper._looks_like_test_token("tsxGenericAttributesType6"))
        self.assertTrue(
            helper._looks_like_test_token("excessPropertyCheckIntersectionWithRecursiveType")
        )

    def test_short_or_lowercase_token_rejected(self) -> None:
        self.assertFalse(helper._looks_like_test_token("emit"))
        self.assertFalse(helper._looks_like_test_token("index"))
        self.assertFalse(helper._looks_like_test_token("failures"))
        self.assertFalse(helper._looks_like_test_token("PR"))


class TestResolveInputs(unittest.TestCase):
    def _index(self) -> "helper.SnapshotIndex":
        return helper.SnapshotIndex(
            timestamp="2026-01-01T00:00:00Z",
            git_sha="deadbee",
            summary={"total_tests": 4, "passed": 3, "failed": 1},
            failures={"TypeScript/tests/cases/compiler/failingGenericExample.ts"},
            accepted={
                "TypeScript/tests/cases/conformance/jsx/tsxGenericAttributesType6.tsx"
            },
            baseline_pass={
                "TypeScript/tests/cases/compiler/passingGenericExample.ts",
            },
            baseline_fail={
                "TypeScript/tests/cases/compiler/failingGenericExample.ts",
            },
            basename_to_path={
                "tsxGenericAttributesType6": (
                    "TypeScript/tests/cases/conformance/jsx/tsxGenericAttributesType6.tsx"
                ),
                "passingGenericExample": (
                    "TypeScript/tests/cases/compiler/passingGenericExample.ts"
                ),
                "failingGenericExample": (
                    "TypeScript/tests/cases/compiler/failingGenericExample.ts"
                ),
            },
        )

    def test_bare_camel_case_name(self) -> None:
        results = helper.resolve_inputs(["tsxGenericAttributesType6"], self._index())
        self.assertEqual(len(results), 1)
        self.assertEqual(len(results[0].resolved), 1)
        self.assertEqual(results[0].resolved[0].status, "stale-accepted")

    def test_typescript_path_verbatim(self) -> None:
        results = helper.resolve_inputs(
            ["TypeScript/tests/cases/compiler/passingGenericExample.ts"],
            self._index(),
        )
        self.assertEqual(results[0].resolved[0].status, "passing")

    def test_basename_with_extension(self) -> None:
        results = helper.resolve_inputs(
            ["passingGenericExample.ts is fine"], self._index()
        )
        self.assertEqual(results[0].resolved[0].status, "passing")

    def test_aggregate_input_resolves_nothing(self) -> None:
        results = helper.resolve_inputs(
            ["Burn down JSX/react emit failures"], self._index()
        )
        self.assertTrue(results[0].is_aggregate)
        self.assertEqual(results[0].resolved, [])

    def test_duplicate_token_resolved_once_per_input(self) -> None:
        results = helper.resolve_inputs(
            ["tsxGenericAttributesType6 and tsxGenericAttributesType6.tsx again"],
            self._index(),
        )
        self.assertEqual(len(results[0].resolved), 1)

    def test_multiple_inputs_keep_order(self) -> None:
        results = helper.resolve_inputs(
            [
                "failingGenericExample.ts",
                "passingGenericExample.ts",
            ],
            self._index(),
        )
        statuses = [r.resolved[0].status for r in results]
        self.assertEqual(statuses, ["failing", "passing"])

    def test_renamed_basename_still_matches(self) -> None:
        # The matching rule must be structural (camelCase basename), not a
        # hardcoded literal. Renaming both ends preserves resolution.
        index = helper.SnapshotIndex(
            timestamp="",
            git_sha="",
            summary={},
            failures=set(),
            accepted=set(),
            baseline_pass={"TypeScript/tests/cases/compiler/freshlyRenamedToken.ts"},
            baseline_fail=set(),
            basename_to_path={
                "freshlyRenamedToken": (
                    "TypeScript/tests/cases/compiler/freshlyRenamedToken.ts"
                ),
            },
        )
        results = helper.resolve_inputs(["freshlyRenamedToken"], index)
        self.assertEqual(results[0].resolved[0].status, "passing")


class TestRendering(unittest.TestCase):
    def _index(self) -> "helper.SnapshotIndex":
        return helper.SnapshotIndex(
            timestamp="2026-01-01T00:00:00Z",
            git_sha="d718764",
            summary={"total_tests": 12582, "passed": 12582, "failed": 0},
            failures=set(),
            accepted={
                "TypeScript/tests/cases/conformance/jsx/tsxGenericAttributesType6.tsx"
            },
            baseline_pass=set(),
            baseline_fail=set(),
            basename_to_path={
                "tsxGenericAttributesType6": (
                    "TypeScript/tests/cases/conformance/jsx/tsxGenericAttributesType6.tsx"
                ),
            },
        )

    def test_markdown_contains_snapshot_metadata(self) -> None:
        results = helper.resolve_inputs(["tsxGenericAttributesType6"], self._index())
        md = helper.render_markdown(results, self._index())
        self.assertIn("2026-01-01T00:00:00Z", md)
        self.assertIn("d718764", md)
        self.assertIn("12582 / 12582 passing", md)

    def test_markdown_table_row_per_resolved_test(self) -> None:
        results = helper.resolve_inputs(["tsxGenericAttributesType6"], self._index())
        md = helper.render_markdown(results, self._index())
        self.assertIn("`tsxGenericAttributesType6.tsx`", md)
        self.assertIn(helper.STATUS_LABELS["stale-accepted"], md)

    def test_markdown_includes_dashboard_hint_only_when_aggregate(self) -> None:
        index = self._index()
        aggregate_results = helper.resolve_inputs(["No test mentioned here"], index)
        named_results = helper.resolve_inputs(["tsxGenericAttributesType6"], index)
        self.assertIn(helper.DASHBOARD_HINT, helper.render_markdown(aggregate_results, index))
        self.assertNotIn(
            helper.DASHBOARD_HINT, helper.render_markdown(named_results, index)
        )

    def test_markdown_escapes_pipe_in_input(self) -> None:
        results = helper.resolve_inputs(["weird|input"], self._index())
        md = helper.render_markdown(results, self._index())
        self.assertIn("weird\\|input", md)

    def test_markdown_includes_closure_pattern_section(self) -> None:
        results = helper.resolve_inputs(["tsxGenericAttributesType6"], self._index())
        md = helper.render_markdown(results, self._index())
        self.assertIn("Closure pattern for stale regression issues", md)
        for label in helper.STATUS_LABELS.values():
            self.assertIn(label, md)

    def test_json_render_payload_shape(self) -> None:
        results = helper.resolve_inputs(
            ["tsxGenericAttributesType6", "aggregate title"], self._index()
        )
        payload = json.loads(helper.render_json(results, self._index()))
        self.assertEqual(payload["timestamp"], "2026-01-01T00:00:00Z")
        self.assertEqual(payload["git_sha"], "d718764")
        self.assertEqual(len(payload["results"]), 2)
        self.assertTrue(payload["results"][1]["aggregate"])
        self.assertFalse(payload["results"][0]["aggregate"])


class TestCLI(unittest.TestCase):
    def _build_dir(self, tmp: Path) -> Path:
        return _build_fake_conformance_dir(
            tmp,
            baseline_lines=[
                "PASS TypeScript/tests/cases/compiler/passingExampleAlpha.ts",
                "PASS TypeScript/tests/cases/compiler/failingExampleBeta.ts",
            ],
            accepted_lines=[
                "# header",
                "TypeScript/tests/cases/compiler/failingExampleBeta.ts",
            ],
            failures={},
            timestamp="2026-04-01T00:00:00Z",
            git_sha="cafef00",
            summary={"total_tests": 2, "passed": 2, "failed": 0},
        )

    def test_main_emits_markdown(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = self._build_dir(Path(tmp))
            from io import StringIO

            stdout = sys.stdout
            sys.stdout = captured = StringIO()
            try:
                helper.main(
                    [
                        "--conformance-dir",
                        str(base),
                        "passingExampleAlpha",
                    ]
                )
            finally:
                sys.stdout = stdout
            output = captured.getvalue()
            self.assertIn("Conformance regression issue", output)
            self.assertIn("passingExampleAlpha", output)
            self.assertIn("cafef00", output)

    def test_main_emits_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = self._build_dir(Path(tmp))
            from io import StringIO

            stdout = sys.stdout
            sys.stdout = captured = StringIO()
            try:
                helper.main(
                    [
                        "--conformance-dir",
                        str(base),
                        "--json",
                        "passingExampleAlpha",
                    ]
                )
            finally:
                sys.stdout = stdout
            payload = json.loads(captured.getvalue())
            self.assertEqual(payload["git_sha"], "cafef00")
            self.assertEqual(len(payload["results"]), 1)

    def test_main_requires_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = self._build_dir(Path(tmp))
            with self.assertRaises(SystemExit):
                helper.main(["--conformance-dir", str(base)])

    def test_main_reads_from_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = self._build_dir(Path(tmp))
            inputs_path = Path(tmp) / "inputs.txt"
            inputs_path.write_text("passingExampleAlpha\nfailingExampleBeta\n", encoding="utf-8")
            from io import StringIO

            stdout = sys.stdout
            sys.stdout = captured = StringIO()
            try:
                helper.main(
                    [
                        "--conformance-dir",
                        str(base),
                        "--from-file",
                        str(inputs_path),
                    ]
                )
            finally:
                sys.stdout = stdout
            output = captured.getvalue()
            self.assertIn("passingExampleAlpha", output)
            self.assertIn("failingExampleBeta", output)


if __name__ == "__main__":
    unittest.main()
