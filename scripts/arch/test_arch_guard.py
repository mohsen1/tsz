from __future__ import annotations

from pathlib import Path
import tempfile
import textwrap
import unittest
import sys

sys.dont_write_bytecode = True

import arch_guard


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

    def codes(self) -> list[str]:
        return [violation.code for violation in arch_guard.check(self.root)]

    def test_clean_reset_layout_passes(self) -> None:
        self.assertEqual(arch_guard.check(self.root), [])

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
