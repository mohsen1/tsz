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


if __name__ == "__main__":
    unittest.main()
