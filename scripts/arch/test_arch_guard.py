from __future__ import annotations

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest
import sys

sys.dont_write_bytecode = True

import arch_guard
import rewrite_architecture_metrics


ROOT_MANIFEST = """
[workspace]
members = ["crates/tsz-core", "crates/tsz-cli", "crates/conformance"]
resolver = "3"

[workspace.dependencies]
tsz = { path = "crates/tsz-core", package = "tsz-core" }
"""


class ArchGuardTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.write("Cargo.toml", ROOT_MANIFEST)
        self.write(
            "crates/tsz-core/Cargo.toml",
            """
            [package]
            name = "tsz-core"
            version = "0.0.0"
            """,
        )
        self.write("crates/tsz-core/src/lib.rs", "pub fn check() {}\n")
        self.write(
            "crates/tsz-cli/Cargo.toml",
            """
            [package]
            name = "tsz-cli"
            version = "0.0.0"

            [dependencies]
            tsz.workspace = true
            """,
        )
        self.write("crates/tsz-cli/src/lib.rs", "pub fn run() {}\n")
        self.write(
            "crates/conformance/Cargo.toml",
            """
            [package]
            name = "tsz-conformance"
            version = "0.0.0"
            """,
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, relative: str, content: str) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(textwrap.dedent(content).lstrip(), encoding="utf-8")

    def git(self, *arguments: str) -> str:
        result = subprocess.run(
            ["git", "-C", str(self.root), *arguments],
            check=True,
            capture_output=True,
            text=True,
        )
        return result.stdout.strip()

    def initialize_direction_repository(self, include_guard: bool = True) -> str:
        self.git("init", "--quiet")
        self.git("config", "user.email", "architecture@example.invalid")
        self.git("config", "user.name", "Architecture Guard")
        if include_guard:
            self.write(
                "scripts/arch/rewrite_architecture_metrics.py",
                "# direction checker\n",
            )
            self.write(
                arch_guard.ARCHITECTURE_RATCHET_PATH,
                json.dumps({"capability_owners": 4}),
            )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "base")
        return self.git("rev-parse", "HEAD")

    def codes(self) -> list[str]:
        return [violation.code for violation in arch_guard.check(self.root)]

    def install_rewrite_metric_sources(self) -> None:
        self.write("scripts/arch/arch_guard.py", "# architecture guard marker\n")
        self.install_rewrite_compiler_size_manifest()
        self.write(
            "crates/tsz-core/src/syntax/ast.rs",
            "struct SourceUnit { syntax_policy: bool }\n",
        )
        self.write(
            "crates/tsz-core/src/syntax/parser/modifiers.rs",
            "struct ProductCapabilities { emit_policy: bool }\n",
        )
        self.write(
            "crates/tsz-core/src/semantics/checker.rs",
            "struct Checker<'a> { queries: Vec<u8>, marker: &'a () }\n",
        )
        self.write(
            "crates/tsz-core/src/program.rs",
            "let result = if options.no_check || missing { CheckResult { } };\n",
        )

    def install_rewrite_compiler_size_manifest(self) -> None:
        self.write(
            arch_guard.REWRITE_COMPILER_SIZE_MANIFEST_PATH,
            json.dumps(
                {
                    "schema_version": 1,
                    "r0_physical_line_limit": 15_000,
                    "include": list(arch_guard.REWRITE_COMPILER_INCLUDE_PATTERNS),
                    "exclude": list(arch_guard.REWRITE_COMPILER_EXCLUDE_PATHS),
                },
                sort_keys=True,
            ),
        )
        for relative in arch_guard.REWRITE_COMPILER_EXCLUDE_PATHS:
            self.write(relative, "// test-only source\n")

    def test_clean_reset_layout_passes(self) -> None:
        self.assertEqual(arch_guard.check(self.root), [])

    def test_rewrite_compiler_size_manifest_is_required_with_the_guard(self) -> None:
        self.write("scripts/arch/arch_guard.py", "# architecture guard marker\n")
        self.assertIn("rewrite-compiler-size-manifest", self.codes())

    def test_rewrite_compiler_size_manifest_keeps_the_r0_limit_exact(self) -> None:
        self.install_rewrite_compiler_size_manifest()
        path = self.root / arch_guard.REWRITE_COMPILER_SIZE_MANIFEST_PATH
        value = json.loads(path.read_text(encoding="utf-8"))
        value["r0_physical_line_limit"] = 46_019
        path.write_text(json.dumps(value, sort_keys=True), encoding="utf-8")
        self.assertIn("rewrite-compiler-size-manifest", self.codes())

    def test_rewrite_compiler_size_manifest_cannot_hide_a_source_root(self) -> None:
        self.install_rewrite_compiler_size_manifest()
        path = self.root / arch_guard.REWRITE_COMPILER_SIZE_MANIFEST_PATH
        value = json.loads(path.read_text(encoding="utf-8"))
        value["include"] = value["include"][:1]
        path.write_text(json.dumps(value, sort_keys=True), encoding="utf-8")
        self.assertIn("rewrite-compiler-size-manifest", self.codes())

    def test_rewrite_compiler_size_uses_explicit_roots_and_exclusions(self) -> None:
        self.install_rewrite_compiler_size_manifest()
        size = arch_guard.rewrite_compiler_size(self.root)
        self.assertEqual(size.physical_lines, 2)
        self.assertEqual(
            size.included_paths,
            (
                "crates/tsz-cli/src/lib.rs",
                "crates/tsz-core/src/lib.rs",
            ),
        )
        self.assertEqual(
            size.excluded_paths,
            arch_guard.REWRITE_COMPILER_EXCLUDE_PATHS,
        )

    def test_r0_size_readiness_strict_mode_fails_without_a_manifest(self) -> None:
        output = io.StringIO()
        with redirect_stdout(output):
            status = arch_guard.main(
                ["--root", str(self.root), "--require-r0-ready"]
            )
        self.assertEqual(status, 1)
        self.assertIn("R0 size readiness: UNAVAILABLE", output.getvalue())
        self.assertIn("r0-compiler-size", output.getvalue())

    def test_r0_size_readiness_reports_debt_and_strict_mode_enforces_it(self) -> None:
        self.install_rewrite_compiler_size_manifest()
        for index in range(8):
            self.write(f"crates/tsz-core/src/shard_{index}.rs", "line\n" * 1_900)

        output = io.StringIO()
        with redirect_stdout(output):
            default_status = arch_guard.main(["--root", str(self.root)])
        self.assertEqual(default_status, 0)
        self.assertIn("R0 size readiness: NOT READY", output.getvalue())

        output = io.StringIO()
        with redirect_stdout(output):
            strict_status = arch_guard.main(
                ["--root", str(self.root), "--require-r0-ready"]
            )
        self.assertEqual(strict_status, 1)
        self.assertIn("r0-compiler-size", output.getvalue())

    def test_r0_size_readiness_strict_mode_accepts_a_small_rewrite(self) -> None:
        self.install_rewrite_compiler_size_manifest()
        output = io.StringIO()
        with redirect_stdout(output):
            status = arch_guard.main(
                ["--root", str(self.root), "--require-r0-ready"]
            )
        self.assertEqual(status, 0)
        self.assertIn("R0 size readiness: READY", output.getvalue())

    def test_r0_size_readiness_requires_strictly_fewer_than_fifteen_thousand(self) -> None:
        at_limit = arch_guard.RewriteCompilerSize(15_000, 15_000, (), ())
        below_limit = arch_guard.RewriteCompilerSize(14_999, 15_000, (), ())
        self.assertFalse(at_limit.r0_ready)
        self.assertTrue(below_limit.r0_ready)

    def test_rewrite_architecture_ratchet_is_required_with_the_guard(self) -> None:
        self.write("scripts/arch/arch_guard.py", "# architecture guard marker\n")
        self.assertIn("architecture-ratchet", self.codes())

    def test_rewrite_architecture_ratchet_requires_an_object_baseline(self) -> None:
        self.install_rewrite_metric_sources()
        for value in ("[]\n", "null\n", "true\n"):
            with self.subTest(value=value.strip()):
                self.write(arch_guard.ARCHITECTURE_RATCHET_PATH, value)
                self.assertIn("architecture-ratchet", self.codes())

    def test_rewrite_architecture_ratchet_blocks_growth(self) -> None:
        self.install_rewrite_metric_sources()
        baseline = arch_guard.rewrite_architecture_metrics(self.root)
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps(baseline, sort_keys=True),
        )
        self.assertNotIn("architecture-ratchet", self.codes())
        self.write(
            "crates/tsz-core/src/syntax/ast.rs",
            "struct SourceUnit { syntax_policy: bool, another_policy: bool }\n",
        )
        self.assertIn("architecture-ratchet", self.codes())

    def test_rewrite_architecture_improvement_must_lower_the_baseline(self) -> None:
        self.install_rewrite_metric_sources()
        baseline = arch_guard.rewrite_architecture_metrics(self.root)
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps(baseline, sort_keys=True),
        )
        self.write(
            "crates/tsz-core/src/syntax/ast.rs",
            "struct SourceUnit {}\n",
        )
        self.assertIn("architecture-ratchet", self.codes())

    def test_merge_base_ratchet_rejects_same_change_baseline_growth(self) -> None:
        base = {"capability_owners": 4, "forcing_entry_points": 3}
        current = {"capability_owners": 5, "forcing_entry_points": 3}
        self.assertEqual(
            rewrite_architecture_metrics.direction_violations(base, current),
            [
                "architecture metric 'capability_owners' grew across the "
                "merge base: base=4, current=5"
            ],
        )

    def test_merge_base_ratchet_allows_consolidation_and_new_metrics(self) -> None:
        base = {"capability_owners": 4, "forcing_entry_points": 3}
        current = {
            "capability_owners": 2,
            "forcing_entry_points": 3,
            "new_review_indicator": 7,
        }
        self.assertEqual(
            rewrite_architecture_metrics.direction_violations(base, current), []
        )

    def test_merge_base_ratchet_rejects_removing_an_existing_metric(self) -> None:
        self.assertEqual(
            rewrite_architecture_metrics.direction_violations(
                {"capability_owners": 4}, {}
            ),
            ["architecture metric 'capability_owners' was removed"],
        )

    def test_merge_base_ratchet_requires_nonnegative_integer_values(self) -> None:
        for value in ({"metric": -1}, {"metric": True}, {"metric": "1"}, None):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    rewrite_architecture_metrics.validate_baseline(value, "test")

    def test_merge_base_loader_accepts_only_a_real_ancestor(self) -> None:
        base = self.initialize_direction_repository()
        self.write("after.txt", "after\n")
        self.git("add", "after.txt")
        self.git("commit", "--quiet", "-m", "after")
        self.assertEqual(
            rewrite_architecture_metrics.load_baseline_at_ref(self.root, base),
            {"capability_owners": 4},
        )
        with self.assertRaisesRegex(ValueError, "must precede HEAD"):
            rewrite_architecture_metrics.load_baseline_at_ref(self.root, "HEAD")

    def test_merge_base_loader_allows_only_genuine_first_introduction(self) -> None:
        base = self.initialize_direction_repository(include_guard=False)
        self.write(
            "scripts/arch/rewrite_architecture_metrics.py",
            "# direction checker\n",
        )
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 4}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "introduce guard")
        self.assertEqual(
            rewrite_architecture_metrics.load_baseline_at_ref(self.root, base),
            {"capability_owners": 4},
        )

    def test_merge_base_loader_allows_uncommitted_first_introduction(self) -> None:
        base = self.initialize_direction_repository(include_guard=False)
        self.write("after.txt", "after\n")
        self.git("add", "after.txt")
        self.git("commit", "--quiet", "-m", "after base")
        self.write(
            "scripts/arch/rewrite_architecture_metrics.py",
            "# direction checker\n",
        )
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 4}),
        )
        self.assertIsNone(
            rewrite_architecture_metrics.load_baseline_at_ref(self.root, base)
        )

    def test_merge_base_loader_pins_later_pr_commits_to_first_introduction(self) -> None:
        base = self.initialize_direction_repository(include_guard=False)
        self.write(
            "scripts/arch/rewrite_architecture_metrics.py",
            "# direction checker\n",
        )
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 4}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "introduce guard")
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 5}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "try to raise guard")
        introduced = rewrite_architecture_metrics.load_baseline_at_ref(
            self.root, base
        )
        current = rewrite_architecture_metrics.load_current_baseline(self.root)
        self.assertEqual(introduced, {"capability_owners": 4})
        self.assertEqual(
            rewrite_architecture_metrics.direction_violations(introduced, current),
            [
                "architecture metric 'capability_owners' grew across the "
                "merge base: base=4, current=5"
            ],
        )

    def test_merge_base_loader_survives_delete_and_recreate_reset(self) -> None:
        self.initialize_direction_repository()
        (self.root / arch_guard.ARCHITECTURE_RATCHET_PATH).unlink()
        (self.root / "scripts/arch/rewrite_architecture_metrics.py").unlink()
        self.git("add", "-u")
        self.git("commit", "--quiet", "-m", "remove guard")
        base = self.git("rev-parse", "HEAD")
        self.write(
            "scripts/arch/rewrite_architecture_metrics.py",
            "# direction checker\n",
        )
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 99}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "restore guard too high")
        self.assertEqual(
            rewrite_architecture_metrics.load_baseline_at_ref(self.root, base),
            {"capability_owners": 4},
        )

    def test_merge_base_loader_uses_strictest_parallel_introduction(self) -> None:
        base = self.initialize_direction_repository(include_guard=False)
        self.git("checkout", "--quiet", "-b", "low", base)
        self.write("scripts/arch/rewrite_architecture_metrics.py", "# checker\n")
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 4}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "low introduction")
        self.git("checkout", "--quiet", "-b", "high", base)
        self.write("scripts/arch/rewrite_architecture_metrics.py", "# checker\n")
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 9}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "high introduction")
        self.git("merge", "--quiet", "--no-ff", "-X", "ours", "low", "-m", "merge")
        self.assertEqual(
            rewrite_architecture_metrics.load_baseline_at_ref(self.root, base),
            {"capability_owners": 4},
        )

    def test_merge_base_loader_keeps_feature_history_after_base_advances(self) -> None:
        base = self.initialize_direction_repository(include_guard=False)
        self.git("checkout", "--quiet", "-b", "feature", base)
        self.write("scripts/arch/rewrite_architecture_metrics.py", "# checker\n")
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 4}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "introduce")
        self.write(
            arch_guard.ARCHITECTURE_RATCHET_PATH,
            json.dumps({"capability_owners": 5}),
        )
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "raise")
        self.git("checkout", "--quiet", "-b", "mainline", base)
        self.write("main.txt", "advance\n")
        self.git("add", "main.txt")
        self.git("commit", "--quiet", "-m", "advance base")
        advanced_base = self.git("rev-parse", "HEAD")
        self.git("merge", "--quiet", "--no-ff", "feature", "-m", "merge feature")
        self.assertEqual(
            rewrite_architecture_metrics.load_baseline_at_ref(
                self.root, advanced_base
            ),
            {"capability_owners": 4},
        )

    def test_merge_base_loader_rejects_relevant_shallow_history(self) -> None:
        self.initialize_direction_repository()
        self.write("after.txt", "after\n")
        self.git("add", "after.txt")
        self.git("commit", "--quiet", "-m", "after")
        with tempfile.TemporaryDirectory() as clone_dir:
            clone = Path(clone_dir) / "shallow"
            subprocess.run(
                [
                    "git",
                    "clone",
                    "--quiet",
                    "--depth",
                    "1",
                    self.root.as_uri(),
                    str(clone),
                ],
                check=True,
            )
            with self.assertRaisesRegex(ValueError, "complete HEAD/base history"):
                rewrite_architecture_metrics.resolve_base_commit(clone, "HEAD")

    def test_arch_size_workflow_supplies_full_history_and_every_event_base(self) -> None:
        workflow = (
            Path(__file__).resolve().parents[2] / ".github/workflows/ci.yml"
        ).read_text(encoding="utf-8")
        arch_size = workflow.split("\n  arch-size:\n", 1)[1].split(
            "\n  refresh-tsc-cache:\n", 1
        )[0]
        self.assertIn("fetch-depth: 0", arch_size)
        self.assertIn("github.event.pull_request.base.sha", arch_size)
        self.assertIn("github.event.merge_group.base_sha", arch_size)
        self.assertIn("github.event.before", arch_size)
        self.assertIn("'HEAD^'", arch_size)
        self.assertIn('--check --base-ref "$TSZ_ARCH_BASE_SHA"', arch_size)
        self.assertIn("github.event_name == 'merge_group'", arch_size)
        self.assertIn("github.event_name == 'pull_request'", arch_size)
        self.assertIn("github.event.pull_request.draft == false", arch_size)
        self.assertIn(
            'if [[ "$TSZ_REQUIRE_R0_READY" == "true" ]]; then',
            arch_size,
        )
        self.assertIn(
            "python3 scripts/arch/arch_guard.py --require-r0-ready",
            arch_size,
        )

    def test_rewrite_architecture_metrics_cover_distributed_ownership(self) -> None:
        self.install_rewrite_metric_sources()
        baseline = arch_guard.rewrite_architecture_metrics(self.root)
        self.write(
            "crates/tsz-core/src/syntax/ast.rs",
            "struct SourceUnit { syntax_policy: bool, another_policy: bool }\n",
        )
        self.write(
            "crates/tsz-core/src/syntax/parser/modifiers.rs",
            "struct ProductCapabilities { emit_policy: bool, service_policy: bool }\n",
        )
        self.write(
            "crates/tsz-core/src/semantics/checker.rs",
            """
            struct Checker<'a> {
                queries: Vec<u8>, cache: FxHashMap<u8, u8>, marker: &'a ()
            }
            """,
        )
        self.write(
            "crates/tsz-core/src/program.rs",
            """
            let result = if options.no_check || missing || local_gap { CheckResult { } };
            let skipped = CheckResult {
                diagnostics: Vec::new(),
                type_count: 0,
                semantic_completion: SemanticCompletion::Complete,
            };
            diagnostics.extend(semantic_diagnostics);
            if first_gap || second_gap {
                semantic_completion = semantic_completion.combine(SemanticCompletion::Deferred);
            }
            """,
        )
        self.write(
            "crates/tsz-core/src/emit_paths.rs",
            """
            struct EmitPlan { incomplete_products: bool, another_policy: bool }
            incomplete_file_products.extend(files.iter().map(|file| file.source.id));
            plan.incomplete_products = true;
            """,
        )
        for path in (
            "crates/tsz-core/src/config.rs",
            "crates/tsz-core/src/emit.rs",
            "crates/tsz-core/src/syntax/parser.rs",
            "crates/tsz-core/src/semantics/checker/required_type.rs",
            "crates/tsz-core/rewrite-tests/foundation.rs",
            "crates/tsz-core/rewrite-tests/type_members.rs",
        ):
            self.write(path, "// central line ratchet\n")
        self.write(
            "crates/tsz-core/src/semantics/query.rs",
            """
            fn query(owner: &mut Owner, file: &File) {
                owner.force_type(
                    nested(value),
                    depth,
                );
                owner.force_deferred(
                    value,
                    deferred,
                    0,
                );
                let _stack = ReferenceExpansionStack::new(Demand::Shape);
                let _ = file.has_unmodeled_feature();
                let _ = file.feature_products_supported();
                owner.require_explicit_type_positions();
            }
            """,
        )
        measured = arch_guard.rewrite_architecture_metrics(self.root)
        for metric in (
            "caller_depth_force_call_sites",
            "capability_policy_mentions",
            "checker_collection_fields",
            "checker_rs_lines",
            "config_rs_lines",
            "emit_plan_boolean_fields",
            "emit_plan_incomplete_assignments",
            "emit_plan_program_wide_promotions",
            "emit_rs_lines",
            "force_deferred_call_sites",
            "force_type_call_sites",
            "foundation_rewrite_test_lines",
            "parser_rs_lines",
            "parser_product_capability_boolean_fields",
            "program_completion_deferred_assignments",
            "program_completion_gate_terms",
            "program_empty_check_result_sites",
            "program_whole_check_skip_terms",
            "r0_handwritten_compiler_rust_lines",
            "reference_stack_constructors",
            "required_type_rs_lines",
            "source_unit_boolean_fields",
            "type_members_rewrite_test_lines",
            "unmodeled_policy_mentions",
            "whole_required_type_prepass_call_sites",
            "zero_depth_force_call_sites",
        ):
            self.assertGreater(measured[metric], baseline[metric], metric)

    def test_semantic_metrics_ignore_nonproduction_text_but_total_size_counts_it(self) -> None:
        self.install_rewrite_metric_sources()
        baseline = arch_guard.rewrite_architecture_metrics(self.root)
        self.write(
            "crates/tsz-core/src/semantics/query.rs",
            r'''
            // owner.force_type(value, 0); file.has_unmodeled_comment();
            const EXAMPLE: &str = "feature_products_supported force_deferred";
            #[cfg(test)]
            mod tests {
                fn fixture(owner: &mut Owner) {
                    owner.force_type(value, depth);
                    let _stack = ReferenceExpansionStack::new(Demand::Shape);
                }
            }
            ''',
        )
        measured = arch_guard.rewrite_architecture_metrics(self.root)
        size_metric = "r0_handwritten_compiler_rust_lines"
        self.assertGreater(measured.pop(size_metric), baseline.pop(size_metric))
        self.assertEqual(measured, baseline)

    def test_workspace_member_is_exact_and_retired_crate_is_rejected(self) -> None:
        self.write(
            "Cargo.toml",
            ROOT_MANIFEST.replace(
                '"crates/conformance"]', '"crates/conformance", "crates/tsz-solver"]'
            ),
        )
        self.write(
            "crates/tsz-solver/Cargo.toml",
            """
            [package]
            name = "tsz-solver"
            version = "0.0.0"
            """,
        )
        codes = self.codes()
        self.assertIn("workspace-member", codes)
        self.assertIn("retired-crate", codes)

    def test_retired_dependency_is_rejected_even_through_package_alias(self) -> None:
        self.write(
            "crates/tsz-core/Cargo.toml",
            """
            [package]
            name = "tsz-core"
            version = "0.0.0"

            [dependencies]
            old-types = { package = "tsz-solver", path = "../tsz-solver" }
            """,
        )
        self.assertIn("retired-dependency", self.codes())

    def test_cli_may_only_consume_core_from_the_internal_graph(self) -> None:
        self.write(
            "crates/tsz-cli/Cargo.toml",
            """
            [package]
            name = "tsz-cli"
            version = "0.0.0"

            [dependencies]
            tsz = { workspace = true }
            helper = { path = "../helper" }
            """,
        )
        self.assertIn("cli-boundary", self.codes())

    def test_semantic_type_universe_cannot_become_a_public_core_api(self) -> None:
        self.write(
            "crates/tsz-core/src/lib.rs",
            "pub mod semantics;\npub use semantics::TypeId;\n",
        )
        self.assertEqual(self.codes().count("semantic-universe-api"), 2)

    def test_qualified_and_rebraced_semantic_reexports_are_rejected(self) -> None:
        cases = [
            "pub use crate::semantics::types::TypeId;\n",
            "pub use ::crate::semantics::types::{TypeKind, TypeStore as Store};\n",
            "pub use crate::{semantics::types::*};\n",
            "pub use self::{semantics as semantic_engine};\n",
        ]
        for source in cases:
            with self.subTest(source=source):
                self.write("crates/tsz-core/src/lib.rs", "mod semantics;\n" + source)
                self.assertIn("semantic-universe-api", self.codes())

    def test_semantic_handle_reexport_from_a_public_module_is_rejected(self) -> None:
        self.write("crates/tsz-core/src/lib.rs", "pub mod service;\nmod semantics;\n")
        self.write(
            "crates/tsz-core/src/service.rs",
            "pub use crate::semantics::types::TypeId as SessionType;\n",
        )
        self.assertIn("semantic-universe-api", self.codes())

    def test_semantic_handle_cannot_escape_through_a_private_alias(self) -> None:
        self.write(
            "crates/tsz-core/src/lib.rs",
            """
            mod semantics;
            use crate::semantics::types::TypeId as Handle;
            use Handle as RenamedHandle;
            pub use RenamedHandle as PublicHandle;
            """,
        )
        self.assertIn("semantic-universe-api", self.codes())

    def test_semantic_handle_cannot_escape_through_other_public_surfaces(self) -> None:
        cases = [
            "pub type PublicHandle = crate::semantics::types::TypeId;\n",
            "pub fn leak() -> crate::semantics::types::TypeId { todo!() }\n",
            "pub struct Leak(pub crate::semantics::types::TypeId);\n",
            "pub struct Leak { pub handle: crate::semantics::types::TypeId }\n",
        ]
        for source in cases:
            with self.subTest(source=source):
                self.write("crates/tsz-core/src/lib.rs", "mod semantics;\n" + source)
                self.assertIn("semantic-universe-api", self.codes())

    def test_private_type_aliases_cannot_launder_a_public_semantic_handle(self) -> None:
        self.write(
            "crates/tsz-core/src/lib.rs",
            """
            mod semantics;
            type Handle = crate::semantics::types::TypeId;
            type RenamedHandle = Handle;
            pub fn leak() -> RenamedHandle { todo!() }
            """,
        )
        self.assertIn("semantic-universe-api", self.codes())

    def test_rust_files_cannot_exceed_two_thousand_physical_lines(self) -> None:
        self.write("crates/tsz-core/src/large.rs", "line\n" * 2_001)
        self.assertIn("rust-file-size", self.codes())

    def test_conformance_tests_keep_the_active_line_limit(self) -> None:
        self.write("crates/conformance/tests/large.rs", "line\n" * 2_001)
        self.assertIn("rust-file-size", self.codes())

    def test_byte_preserved_oracle_corpus_is_not_line_limited(self) -> None:
        self.write("tests/legacy-internal/tsz-solver/tests/large.rs", "line\n" * 2_001)
        self.write("crates/tsz-solver/tests/large.rs", "line\n" * 2_001)
        self.write("crates/tsz-core/tests/large.rs", "line\n" * 2_001)
        self.assertNotIn("rust-file-size", self.codes())

    def test_oracle_corpus_still_rejects_sound_mode_markers(self) -> None:
        self.write(
            "tests/legacy-internal/tsz-solver/tests/retired.rs",
            "// Sound Mode belonged to the deleted implementation.\n",
        )
        self.write("crates/tsz-solver/tests/retired.rs", "// --sound\n")
        self.assertEqual(self.codes().count("sound-mode"), 2)

    def test_sound_mode_marker_is_rejected_outside_reset_history(self) -> None:
        self.write("docs/guide.md", "Enable sound-mode for stricter checks.\n")
        self.write("crates/tsz-core/src/options.rs", "const RETIRED: &str = \"--sound\";\n")
        self.write("docs/plan/ROADMAP.md", "Sound Mode was retired.\n")
        self.assertEqual(self.codes().count("sound-mode"), 2)

    def test_obvious_semantic_string_hardcoding_is_rejected(self) -> None:
        self.write(
            "crates/tsz-core/src/checker/relation.rs",
            """
            pub fn relate(rendered_type: &str) -> bool {
                if rendered_type.contains("FixtureOnly") { return true; }
                false
            }
            """,
        )
        self.assertIn("semantic-hardcoding", self.codes())

    def test_exact_strings_in_tests_are_not_semantic_inputs(self) -> None:
        self.write(
            "crates/tsz-core/src/checker/tests.rs",
            """
            #[test]
            fn exact_message() {
                if "message".contains("message") {}
            }
            """,
        )
        self.assertNotIn("semantic-hardcoding", self.codes())

    def test_core_cannot_read_process_environment_variables(self) -> None:
        self.write(
            "crates/tsz-core/src/options.rs",
            "pub fn option() { let _ = std::env::var(\"COMPAT_MODE\"); }\n",
        )
        self.assertIn("ambient-env", self.codes())

    def test_core_cannot_alias_a_process_environment_reader(self) -> None:
        self.write(
            "crates/tsz-core/src/options.rs",
            "pub fn option() { let read = std::env::var; let _ = read(\"COMPAT_MODE\"); }\n",
        )
        self.assertIn("ambient-env", self.codes())

    def test_core_cannot_hide_environment_access_behind_an_import(self) -> None:
        self.write(
            "crates/tsz-core/src/options.rs",
            "use std::env as process_env;\npub fn cwd() { let _ = process_env::current_dir(); }\n",
        )
        self.assertIn("ambient-env", self.codes())

    def test_unknown_tsz_switches_are_rejected_in_core_and_cli(self) -> None:
        self.write(
            "crates/tsz-core/src/options.rs",
            'const MODE: &str = r#"TSZ_FORCE_COMPAT"#;\n',
        )
        self.write(
            "crates/tsz-cli/src/config.rs",
            'const MODE: &str = "TSZ_USE_OLD_SOLVER";\n',
        )
        self.assertEqual(self.codes().count("behavior-switch"), 2)

    def test_cli_cannot_read_arbitrary_process_environment_variables(self) -> None:
        self.write(
            "crates/tsz-cli/src/driver.rs",
            'pub fn option() { let _ = std::env::var("COMPAT_MODE"); }\n',
        )
        self.assertIn("ambient-env", self.codes())

    def test_cli_observability_names_are_rejected_outside_telemetry_owner(self) -> None:
        self.write(
            "crates/tsz-cli/src/driver.rs",
            'pub fn configure() { let _ = std::env::var("TSZ_LOG"); }\n',
        )
        codes = self.codes()
        self.assertIn("ambient-env", codes)
        self.assertIn("behavior-switch", codes)

    def test_cli_dynamic_environment_name_is_rejected(self) -> None:
        self.write(
            "crates/tsz-cli/src/telemetry.rs",
            "pub fn configure(name: &str) { let _ = std::env::var(name); }\n",
        )
        self.assertIn("ambient-env", self.codes())

    def test_cli_environment_import_alias_is_rejected(self) -> None:
        self.write(
            "crates/tsz-cli/src/telemetry.rs",
            "use std::env as process_env;\npub fn configure() { "
            'let _ = process_env::var("TSZ_LOG"); }\n',
        )
        self.assertIn("ambient-env", self.codes())

    def test_cli_observability_environment_names_are_narrowly_allowed(self) -> None:
        self.write(
            "crates/tsz-cli/src/telemetry.rs",
            """
            pub fn configure() {
                let _ = std::env::var("TSZ_LOG");
                let _ = std::env::var("TSZ_LOG_FORMAT");
                let _ = std::env::var_os("TSZ_PERF_COUNTERS");
            }
            """,
        )
        self.assertNotIn("behavior-switch", self.codes())

    def test_compile_time_package_metadata_is_not_an_environment_read(self) -> None:
        self.write(
            "crates/tsz-core/src/version.rs",
            """
            const VERSION: &str = env!("CARGO_PKG_VERSION");
            const REVISION: Option<&str> = option_env!("GIT_REVISION");
            """,
        )
        self.assertNotIn("ambient-env", self.codes())

    def test_tsz_compile_time_switch_is_still_rejected(self) -> None:
        self.write(
            "crates/tsz-core/src/options.rs",
            'const MODE: Option<&str> = option_env!("TSZ_FORCE_COMPAT");\n',
        )
        self.assertIn("behavior-switch", self.codes())

    def test_environment_examples_in_comments_and_tests_are_ignored(self) -> None:
        self.write(
            "crates/tsz-core/src/options.rs",
            """
            // std::env::var("TSZ_FORCE_COMPAT") was intentionally removed.
            pub fn option() {}

            #[cfg(test)]
            mod tests {
                #[test]
                fn can_control_the_test_process() {
                    let _ = std::env::var("TSZ_TEST_ONLY");
                }
            }
            """,
        )
        codes = self.codes()
        self.assertNotIn("ambient-env", codes)
        self.assertNotIn("behavior-switch", codes)

    def test_production_after_cfg_test_modules_is_still_scanned(self) -> None:
        self.write(
            "crates/tsz-core/src/options.rs",
            """
            #[cfg(test)]
            mod first_tests {
                mod nested { fn fixture() { let _ = std::env::var("TEST_ONLY"); } }
            }
            pub fn middle() {}
            #[cfg(test)]
            #[allow(dead_code)]
            mod second_tests { fn fixture() { let _ = std::env::var_os("TEST_ONLY"); } }
            pub fn option() { let _ = std::env::vars(); }
            """,
        )
        self.assertEqual(self.codes().count("ambient-env"), 1)


if __name__ == "__main__":
    unittest.main()
