"""Tests for the clean-slate README status contract."""

import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
REFRESH_README = ROOT / "scripts" / "refresh-readme.py"
WEBSITE = ROOT / "crates" / "tsz-website"

spec = importlib.util.spec_from_file_location("refresh_readme", REFRESH_README)
refresh_readme = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(refresh_readme)


class RefreshReadmeTests(unittest.TestCase):
    def canonical_readme(self):
        return "\n".join(
            (
                "# Current status: R0",
                "TypeScript `7.0.2`",
                "The eventual goal is exact TypeScript compatibility and at least 3x throughput.",
                refresh_readme.STATUS_BLOCK,
                "## Frozen legacy checkpoint",
                refresh_readme.LEGACY_CHECKPOINT,
                "11,667 / 12,043 runnable cases (96.9%)",
            )
        )

    def test_repository_readme_satisfies_contract(self):
        text = refresh_readme.README.read_text(encoding="utf-8")
        self.assertEqual(refresh_readme.validation_errors(text), [])

    def test_status_replacement_repairs_only_managed_block(self):
        text = "before\n<!-- R0_STATUS_START -->\nstale\n<!-- R0_STATUS_END -->\nafter\n"
        repaired = refresh_readme.replace_status_block(text)
        self.assertEqual(
            repaired,
            f"before\n{refresh_readme.STATUS_BLOCK}\nafter\n",
        )

    def test_status_replacement_requires_exactly_one_block(self):
        with self.assertRaisesRegex(ValueError, "exactly one"):
            refresh_readme.replace_status_block("no managed block")
        with self.assertRaisesRegex(ValueError, "exactly one"):
            refresh_readme.replace_status_block(
                f"{refresh_readme.STATUS_BLOCK}\n{refresh_readme.STATUS_BLOCK}"
            )

    def test_retired_live_dashboard_markers_are_rejected(self):
        text = self.canonical_readme() + "\n<!-- CONFORMANCE_START -->\n"
        errors = refresh_readme.validation_errors(text)
        self.assertTrue(any("CONFORMANCE_START" in error for error in errors))

    def test_frozen_checkpoint_must_remain_explicit(self):
        text = self.canonical_readme().replace(refresh_readme.LEGACY_CHECKPOINT, "missing")
        errors = refresh_readme.validation_errors(text)
        self.assertTrue(any(refresh_readme.LEGACY_CHECKPOINT in error for error in errors))

    def test_managed_block_does_not_claim_a_release(self):
        self.assertIn("validation-only", refresh_readme.STATUS_BLOCK)
        self.assertIn("no supported install", refresh_readme.STATUS_BLOCK)
        self.assertIn("WASM", refresh_readme.STATUS_BLOCK)

    def test_website_has_no_legacy_metric_builder(self):
        package = (WEBSITE / "package.json").read_text(encoding="utf-8")
        self.assertNotIn("build:legacy", package)
        self.assertFalse((WEBSITE / "build.mjs").exists())
        self.assertFalse((WEBSITE / "src" / "_data" / "metrics.js").exists())

    def test_website_does_not_fallback_to_retired_benchmark_snapshot(self):
        sources = (
            WEBSITE / ".eleventy.js",
            WEBSITE / "src" / "_data" / "benchmark_data.js",
            WEBSITE / "src" / "_data" / "benchmark_mean_chart.js",
        )
        for source in sources:
            with self.subTest(source=source):
                self.assertNotIn(
                    "bench-snapshot.json",
                    source.read_text(encoding="utf-8"),
                )

    def test_site_status_pages_do_not_render_live_legacy_metrics(self):
        site_docs = tuple((ROOT / "docs" / "site" / name).read_text(encoding="utf-8") for name in (
            "index.md",
            "compatibility.md",
            "benchmarks.md",
        ))
        for text in site_docs:
            self.assertNotIn("{{ metrics.", text)
            self.assertNotIn("{{ benchmark_mean_chart", text)
            self.assertNotIn("{{ benchmark_charts", text)
        self.assertIn("Frozen legacy checkpoint", site_docs[1])


if __name__ == "__main__":
    unittest.main()
