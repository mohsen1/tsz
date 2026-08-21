#!/usr/bin/env python3
"""Static contracts for the clean-slate full-ci rewrite gate."""

from __future__ import annotations

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "ci" / "full-ci.sh"
UNIT_CONTRACT = ROOT / "scripts" / "ci" / "check-unit-gate-contracts.sh"
PROJECT_STATS = ROOT / "scripts" / "bench" / "project-file-stats.mjs"
PROJECT_STATS_TEST = ROOT / "scripts" / "bench" / "test-project-file-stats.mjs"


def function_body(source: str, name: str) -> str:
    match = re.search(
        rf"^{re.escape(name)}\(\) \{{\n(?P<body>.*?)^\}}$",
        source,
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"missing shell function {name}")
    return match.group("body")


class FullCiRewriteGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = SCRIPT.read_text(encoding="utf-8")
        cls.unit_contract = UNIT_CONTRACT.read_text(encoding="utf-8")
        cls.project_stats = PROJECT_STATS.read_text(encoding="utf-8")
        cls.project_stats_test = PROJECT_STATS_TEST.read_text(encoding="utf-8")

    def test_unit_packages_are_exact_clean_slate_workspace(self) -> None:
        match = re.search(
            r"_UNIT_TEST_PACKAGES=\(\n(?P<body>.*?)\n\)",
            self.source,
            flags=re.DOTALL,
        )
        self.assertIsNotNone(match)
        packages = [
            line.strip()
            for line in match.group("body").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        ]
        self.assertEqual(packages, ["tsz-core", "tsz-cli", "tsz-conformance"])

    def test_strict_unit_runner_has_no_failure_baseline(self) -> None:
        body = function_body(self.source, "run_unit_tests")
        self.assertIn("scripts/ci/unit-nextest.sh", body)
        self.assertIn('--packages "$packages"', body)
        self.assertNotIn("--gate", body)
        self.assertNotIn("known-failures", body)

    def test_lint_matches_rewrite_foundation_contract(self) -> None:
        body = function_body(self.source, "run_lint")
        for command in (
            "cargo fmt --all --check",
            "cargo check --workspace --all-targets",
            "cargo clippy --profile ci-lint --workspace",
            "python3 scripts/arch/arch_guard.py",
            "python3 scripts/reset/verify-legacy-inline.py",
            "scripts/ci/check-unit-gate-contracts.sh",
        ):
            self.assertIn(command, body)
        self.assertNotIn("check-checker-boundaries", body)
        self.assertNotIn("check-clippy-warn-ratchet", body)

    def test_retired_packages_are_not_active_ci_inputs(self) -> None:
        retired = (
            "tsz-common",
            "tsz-scanner",
            "tsz-parser",
            "tsz-binder",
            "tsz-solver",
            "tsz-checker",
            "tsz-emitter",
            "tsz-lowering",
            "tsz-wasm",
        )
        for package in retired:
            self.assertNotIn(package, self.source, package)

    def test_dist_bundle_contains_all_native_process_contracts(self) -> None:
        body = function_body(self.source, "build_test_binaries")
        for binary in ("tsz", "tsz-server", "tsz-lsp", "try-tsz"):
            self.assertIn(f".target/dist-fast/{binary}", body)
            self.assertIn(f"--bin {binary}", body)

    def test_project_stats_prepares_and_selects_the_pinned_typescript(self) -> None:
        setup = "./scripts/setup/ensure-pinned-typescript.sh scripts"
        stats_test = "node scripts/bench/test-project-file-stats.mjs"
        self.assertIn(setup, self.unit_contract)
        self.assertIn(stats_test, self.unit_contract)
        self.assertLess(self.unit_contract.index(setup), self.unit_contract.index(stats_test))
        self.assertIn('TSC_TOOL_DIR_VALUE="$ROOT_DIR/scripts"', self.unit_contract)
        self.assertIn('TSC_BIN_VALUE="$ROOT_DIR/scripts/node_modules/typescript/bin/tsc"', self.unit_contract)
        self.assertNotIn('require.resolve("typescript/package.json")', self.project_stats)
        self.assertNotIn("[skip] project-file-stats.mjs", self.project_stats_test)


if __name__ == "__main__":
    unittest.main()
