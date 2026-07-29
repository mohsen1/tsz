"""Tests for the CheckerContext field-lifetime inventory guard.

Covers the T2.1 destination-shell decomposition contract added on top of the
existing lifetime/capability classification: every valid lifetime class must
name a destination shell, and every mapped shell must exist as a real
`pub struct`. Run standalone (as CI's arch-tool-smoke job does):

    python3 scripts/arch/test_checker_field_inventory.py
"""

import tempfile
from unittest import mock

from test_arch_guard_support import ROOT, load_arch_script_module, pathlib, unittest

INVENTORY_PATH = ROOT / "scripts" / "arch" / "checker_field_inventory.py"


def load_inventory_module():
    return load_arch_script_module("checker_field_inventory", INVENTORY_PATH)


class CheckerContextFieldParsingTests(unittest.TestCase):
    def setUp(self):
        self.inv = load_inventory_module()

    def test_all_field_visibilities_and_multiline_types_are_inventoried(self):
        fixture = """\
pub struct CheckerContext<'a> {
    pub public_field: u32,
    pub(crate) crate_field:
        Vec<&'a str>,
    private_field: Option<String>,
}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "context.rs"
            path.write_text(fixture, encoding="utf-8")
            fields = self.inv.parse_checker_context_fields(path)

        self.assertEqual(
            [(field.name, field.rust_type) for field in fields],
            [
                ("public_field", "u32"),
                ("crate_field", "Vec<&'a str>"),
                ("private_field", "Option<String>"),
            ],
        )

    def test_real_inventory_includes_private_augmentation_journal(self):
        fields = self.inv.parse_checker_context_fields(
            self.inv.CHECKER_CONTEXT_RS
        )
        self.assertIn("augmentation_local_journals", {field.name for field in fields})


class DestinationShellContractTests(unittest.TestCase):
    def setUp(self):
        self.inv = load_inventory_module()
        self.declared = self.inv.declared_shell_structs()

    def test_every_valid_lifetime_maps_to_a_shell(self):
        """The decomposition contract must name a destination for every
        classifiable lifetime; otherwise a newly classified field would have no
        machine-checked home."""
        unmapped = self.inv.VALID_LIFETIMES - self.inv.LIFETIME_DESTINATION_SHELL.keys()
        self.assertEqual(
            unmapped,
            set(),
            f"lifetime classes with no destination shell: {sorted(unmapped)}",
        )

    def test_declared_shells_include_every_mapped_shell(self):
        """Every shell named by the mapping must exist as a `pub struct` in the
        scanned source files — proving the shells are real, not prose."""
        for shell in set(self.inv.LIFETIME_DESTINATION_SHELL.values()):
            self.assertIn(
                shell,
                self.declared,
                f"destination shell {shell!r} is not declared as a `pub struct`",
            )

    def test_check_passes_on_the_real_tree(self):
        """The shipped manifest + shells must satisfy the contract."""
        self.assertEqual(self.inv.check_destination_shells(self.declared), [])

    def test_missing_shell_struct_is_reported(self):
        """A mapped shell that is absent from the declared structs must fail —
        this is the drift guard the contract exists to provide."""
        # Drop one real shell to simulate a rename/removal that left the mapping
        # pointing at a now-missing type.
        victim = "FileSession"
        self.assertIn(victim, self.declared)
        failures = self.inv.check_destination_shells(self.declared - {victim})
        self.assertTrue(failures, "expected a failure when a shell is missing")
        self.assertTrue(
            any(victim in line for line in failures),
            f"missing-shell failure did not mention {victim!r}: {failures}",
        )

    def test_unmapped_lifetime_is_reported(self):
        """A valid lifetime with no destination must fail the mapping check."""
        # `patch.dict` snapshots and restores the mapping around the mutation.
        with mock.patch.dict(self.inv.LIFETIME_DESTINATION_SHELL):
            # Simulate a newly classified lifetime that nobody mapped to a shell.
            self.inv.LIFETIME_DESTINATION_SHELL.pop("SpeculationScoped")
            failures = self.inv.check_destination_shells(self.declared)
        self.assertTrue(
            any("SpeculationScoped" in line for line in failures),
            f"unmapped lifetime not reported: {failures}",
        )

    def test_shell_sources_exist(self):
        """The scanned shell source files must exist, else the guard would
        silently find zero structs and pass vacuously."""
        for path in self.inv.DESTINATION_SHELL_SOURCES:
            self.assertTrue(
                pathlib.Path(path).exists(), f"shell source missing: {path}"
            )


if __name__ == "__main__":
    unittest.main()
