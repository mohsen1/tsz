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
WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"

HEAVY_READY_CONDITION = (
    "if: ${{ (github.event_name == 'schedule' || "
    "github.event_name == 'workflow_dispatch' || "
    "github.event_name == 'merge_group' || (github.event_name == 'pull_request' && "
    "github.event.pull_request.draft == false)) && "
    "inputs.refresh_tsc_cache != true }}"
)
EXACT_HEAD_REF = (
    "ref: ${{ github.event_name == 'pull_request' && "
    "github.event.pull_request.head.sha || github.sha }}"
)
HEAVY_READY_EVENT = (
    "HEAVY_READY_EVENT: ${{ github.event_name == 'schedule' || "
    "github.event_name == 'workflow_dispatch' || "
    "github.event_name == 'merge_group' || (github.event_name == 'pull_request' && "
    "github.event.pull_request.draft == false) }}"
)
UNIT_LANE_EVENT = (
    "UNIT_LANE_EVENT: ${{ github.event_name == 'schedule' || "
    "github.event_name == 'workflow_dispatch' }}"
)
ACTIVE_REWRITE_TEST_ROOTS = (
    ROOT / "crates" / "tsz-core" / "rewrite-tests",
    ROOT / "crates" / "tsz-cli" / "rewrite-tests",
)
ATTRIBUTE_PATTERN = re.compile(
    r"^[^\S\r\n]*#\s*\[(?P<body>[^]]*)\]",
    flags=re.MULTILINE,
)
IGNORE_TOKEN_PATTERN = re.compile(r"\bignore\b")


def function_body(source: str, name: str) -> str:
    match = re.search(
        rf"^{re.escape(name)}\(\) \{{\n(?P<body>.*?)^\}}$",
        source,
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"missing shell function {name}")
    return match.group("body")


def workflow_job(source: str, name: str) -> str:
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [a-z][a-z0-9-]*:\n|\Z)",
        source,
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"missing workflow job {name}")
    return match.group("body")


def workflow_named_step(source: str, job: str, step: str) -> str:
    body = workflow_job(source, job)
    match = re.search(
        rf"^      - name: {re.escape(step)}\n(?P<body>.*?)(?=^      - (?:name:|uses:)|\Z)",
        body,
        flags=re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"missing workflow step {job}: {step}")
    return match.group("body")


def blank_non_newlines(chars: list[str], start: int, end: int) -> None:
    for index in range(start, end):
        if chars[index] not in "\r\n":
            chars[index] = " "


def char_literal_end(source: str, start: int) -> int | None:
    index = start + 1
    if index >= len(source) or source[index] in "\r\n":
        return None
    if source[index] == "\\":
        index += 1
        if index >= len(source):
            return None
        if source[index] == "u" and source[index + 1 : index + 2] == "{":
            closing_brace = source.find("}", index + 2)
            if closing_brace == -1:
                return None
            index = closing_brace + 1
        elif source[index] == "x":
            index += 3
        else:
            index += 1
    else:
        index += 1
    if index < len(source) and source[index] == "'":
        return index + 1
    return None


def mask_rust_comments_and_literals(source: str) -> str:
    chars = list(source)
    index = 0
    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index + 2)
            end = len(source) if end == -1 else end
            blank_non_newlines(chars, index, end)
            index = end
            continue
        if source.startswith("/*", index):
            start = index
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            blank_non_newlines(chars, start, index)
            continue
        if source[index] == "r":
            opening_quote = index + 1
            while opening_quote < len(source) and source[opening_quote] == "#":
                opening_quote += 1
            if opening_quote < len(source) and source[opening_quote] == '"':
                terminator = '"' + "#" * (opening_quote - index - 1)
                end = source.find(terminator, opening_quote + 1)
                end = len(source) if end == -1 else end + len(terminator)
                blank_non_newlines(chars, index, end)
                index = end
                continue
        if source[index] == '"':
            start = index
            index += 1
            while index < len(source):
                if source[index] == "\\":
                    index = min(index + 2, len(source))
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            blank_non_newlines(chars, start, index)
            continue
        if source[index] == "'":
            end = char_literal_end(source, index)
            if end is not None:
                blank_non_newlines(chars, index, end)
                index = end
                continue
        index += 1
    return "".join(chars)


def active_ignore_attribute_lines(source: str) -> list[int]:
    masked = mask_rust_comments_and_literals(source)
    return [
        masked.count("\n", 0, match.start()) + 1
        for match in ATTRIBUTE_PATTERN.finditer(masked)
        if IGNORE_TOKEN_PATTERN.search(match.group("body"))
    ]


def active_rewrite_ignores() -> list[str]:
    offenders = []
    for root in ACTIVE_REWRITE_TEST_ROOTS:
        if not root.is_dir():
            raise AssertionError(f"missing active rewrite-test root: {root}")
        paths = sorted(root.rglob("*.rs"))
        if not paths:
            raise AssertionError(f"active rewrite-test root has no Rust tests: {root}")
        for path in paths:
            source = path.read_text(encoding="utf-8")
            for line in active_ignore_attribute_lines(source):
                offenders.append(f"{path.relative_to(ROOT)}:{line}")
    return offenders


class FullCiRewriteGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = SCRIPT.read_text(encoding="utf-8")
        cls.unit_contract = UNIT_CONTRACT.read_text(encoding="utf-8")
        cls.project_stats = PROJECT_STATS.read_text(encoding="utf-8")
        cls.project_stats_test = PROJECT_STATS_TEST.read_text(encoding="utf-8")
        cls.workflow = WORKFLOW.read_text(encoding="utf-8")

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

    def test_emit_binary_resolution_contract_runs_in_the_pr_gate(self) -> None:
        self.assertIn(
            "python3 scripts/emit/test_run_binary_resolution.py",
            self.unit_contract,
        )

    def test_active_rewrite_ignores_are_rejected_by_anchored_attribute(self) -> None:
        self.assertNotIn("rg ", self.unit_contract)
        self.assertEqual(
            tuple(path.relative_to(ROOT).as_posix() for path in ACTIVE_REWRITE_TEST_ROOTS),
            (
                "crates/tsz-core/rewrite-tests",
                "crates/tsz-cli/rewrite-tests",
            ),
        )

        matches = {
            "direct": "  #[ignore]\nfn skipped() {}\n",
            "whitespace": "# [ ignore ]\nfn skipped() {}\n",
            "cfg_attr": '#[cfg_attr(feature = "slow", ignore)]\nfn skipped() {}\n',
            "multiline_cfg_attr": (
                "# [\n"
                "  cfg_attr(\n"
                '    feature = "slow",\n'
                "    ignore\n"
                "  )\n"
                "]\n"
                "fn skipped() {}\n"
            ),
            "commented_cfg_attr": (
                '#[cfg_attr(feature = "slow", /* retained debt */ ignore)]\n'
                "fn skipped() {}\n"
            ),
        }
        for name, source in matches.items():
            with self.subTest(name=name):
                self.assertTrue(active_ignore_attribute_lines(source))

        misses = {
            "line_comment": "  // #[ignore]\nfn active() {}\n",
            "commented_whitespace": "// # [ ignore ]\nfn active() {}\n",
            "block_comment": "/*\n#[ignore]\n*/\nfn active() {}\n",
            "fixture_string": 'const ATTR: &str = "#[ignore]";\n',
            "multiline_raw_string": (
                'const ATTR: &str = r#"\n'
                "#[ignore]\n"
                '"#;\n'
            ),
            "ignore_in_cfg_string": (
                '#[cfg_attr(feature = "ignore", allow(dead_code))]\n'
                "fn active() {}\n"
            ),
            "other_attribute": "#[allow(dead_code)]\nfn active() {}\n",
        }
        for name, source in misses.items():
            with self.subTest(name=name):
                self.assertFalse(active_ignore_attribute_lines(source))

        offenders = active_rewrite_ignores()
        self.assertFalse(
            offenders,
            "active #[ignore] attributes are forbidden in rewrite tests:\n"
            + "\n".join(offenders),
        )

    def test_pull_request_transitions_trigger_a_fresh_workflow(self) -> None:
        preamble = self.workflow.partition("\njobs:\n")[0]
        self.assertIn(
            "types: [opened, reopened, synchronize, ready_for_review, "
            "converted_to_draft]",
            preamble,
        )

    def test_heavy_observations_run_on_ready_heads_and_merge_groups(self) -> None:
        for name in ("conformance", "emit", "fourslash"):
            body = workflow_job(self.workflow, name)
            self.assertIn(HEAVY_READY_CONDITION, body, name)
            self.assertIn("continue-on-error: true", body, name)

        for job, observe_step, upload_step in (
            (
                "conformance",
                "Observe conformance shard",
                "Upload conformance shard result",
            ),
            (
                "fourslash",
                "Observe fourslash shard",
                "Upload fourslash shard result",
            ),
        ):
            self.assertIn(
                "continue-on-error: true",
                workflow_named_step(self.workflow, job, observe_step),
                job,
            )
            self.assertIn(
                "if-no-files-found: error",
                workflow_named_step(self.workflow, job, upload_step),
                job,
            )

        unit = workflow_job(self.workflow, "unit")
        self.assertIn("github.event_name == 'schedule'", unit)
        self.assertIn("github.event_name == 'workflow_dispatch'", unit)
        self.assertNotIn("github.event_name == 'merge_group'", unit)
        self.assertNotIn("github.event.pull_request.draft", unit)

    def test_heavy_jobs_checkout_the_exact_event_head(self) -> None:
        heavy_jobs = (
            "conformance",
            "conformance-aggregate",
            "emit",
            "emit-aggregate",
            "fourslash",
            "fourslash-aggregate",
        )
        for name in heavy_jobs:
            self.assertIn(EXACT_HEAD_REF, workflow_job(self.workflow, name), name)

        for name in ("clippy", "arch-size"):
            self.assertNotIn(
                "github.event.pull_request.head.sha",
                workflow_job(self.workflow, name),
                name,
            )

    def test_heavy_aggregates_follow_their_leaf_job(self) -> None:
        for aggregate, leaf in (
            ("conformance-aggregate", "conformance"),
            ("emit-aggregate", "emit"),
            ("fourslash-aggregate", "fourslash"),
        ):
            body = workflow_job(self.workflow, aggregate)
            self.assertIn(f"needs: {leaf}", body, aggregate)
            self.assertIn(f"needs.{leaf}.result != 'skipped'", body, aggregate)
            self.assertIn(f"needs.{leaf}.result != 'cancelled'", body, aggregate)

        for job, aggregate_step, upload_step in (
            (
                "conformance-aggregate",
                "Aggregate conformance observation",
                "Upload conformance observation aggregate",
            ),
            (
                "fourslash-aggregate",
                "Aggregate fourslash observation",
                "Upload fourslash observation aggregate",
            ),
        ):
            self.assertNotIn(
                "continue-on-error: true",
                workflow_named_step(self.workflow, job, aggregate_step),
                job,
            )
            self.assertIn(
                "if-no-files-found: error",
                workflow_named_step(self.workflow, job, upload_step),
                job,
            )

        emit_observation = workflow_named_step(
            self.workflow,
            "emit-aggregate",
            "Aggregate emit observation",
        )
        self.assertIn("continue-on-error: true", emit_observation)
        emit_direction = workflow_named_step(
            self.workflow,
            "emit-aggregate",
            "Enforce rewrite emit direction",
        )
        self.assertNotIn("continue-on-error: true", emit_direction)

    def test_summary_distinguishes_heavy_ready_and_unit_lane_events(self) -> None:
        summary = workflow_job(self.workflow, "ci-summary")
        self.assertIn(HEAVY_READY_EVENT, summary)
        self.assertIn(UNIT_LANE_EVENT, summary)
        self.assertIn(
            'heavy_ready_event = os.environ.get("HEAVY_READY_EVENT") == "true"',
            summary,
        )
        self.assertIn(
            'unit_lane_event = os.environ.get("UNIT_LANE_EVENT") == "true"',
            summary,
        )
        self.assertIn("required.update(heavy)", summary)
        self.assertIn('required.add("unit")', summary)
        self.assertIn("if name in required:", summary)
        self.assertNotIn("heavy_lane_event", summary)
        heavy = re.search(
            r"^\s+heavy = \{\n(?P<body>.*?)^\s+\}\n",
            summary,
            flags=re.MULTILINE | re.DOTALL,
        )
        self.assertIsNotNone(heavy)
        self.assertEqual(
            set(re.findall(r'"([a-z-]+)"', heavy.group("body"))),
            {
                "conformance",
                "conformance-aggregate",
                "emit",
                "emit-aggregate",
                "fourslash",
                "fourslash-aggregate",
            },
        )
        self.assertIn("including unsupported cases", summary)
        self.assertIn("remain informational", summary)


if __name__ == "__main__":
    unittest.main()
