#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


SCRIPT_PATH = pathlib.Path(__file__).with_name("analyze-conformance-areas.py")
SPEC = importlib.util.spec_from_file_location("analyze_conformance_areas", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class AnalyzeConformanceAreasTests(unittest.TestCase):
    def test_extract_area_handles_relative_harness_paths(self):
        area = MODULE.extract_area(
            "TypeScript/tests/cases/conformance/types/literal/foo.ts",
            2,
        )
        self.assertEqual(area, "types/literal")

    def test_extract_area_handles_absolute_harness_paths(self):
        area = MODULE.extract_area(
            "/tmp/workspace/TypeScript/tests/cases/conformance/types/literal/foo.ts",
            2,
        )
        self.assertEqual(area, "types/literal")

    def test_extract_area_handles_absolute_compiler_paths(self):
        area = MODULE.extract_area(
            "/tmp/workspace/TypeScript/tests/cases/compiler/foo.ts",
            2,
        )
        self.assertEqual(area, "compiler")


if __name__ == "__main__":
    unittest.main()
