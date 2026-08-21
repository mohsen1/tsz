import importlib.util
import os
import subprocess
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
PROVENANCE_PATH = ROOT / "scripts/conformance/snapshot-provenance.py"
SPEC = importlib.util.spec_from_file_location("snapshot_provenance", PROVENANCE_PATH)
assert SPEC and SPEC.loader
PROVENANCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROVENANCE)


class SnapshotProvenanceContractTests(unittest.TestCase):
    def test_snapshot_revalidates_one_exact_provenance_before_publish(self):
        wrapper = (ROOT / "scripts/conformance/conformance.sh").read_text(
            encoding="utf-8"
        )
        provenance = PROVENANCE_PATH.read_text(encoding="utf-8")
        self.assertIn("set -euo pipefail", wrapper)
        self.assertNotIn("TSZ_CI_TRUST_DIST_FAST_CACHE", wrapper)
        self.assertNotIn("typescript/lib/tsc.js", wrapper)
        self.assertNotIn("corpus-lib-dir.sh", wrapper)
        self.assertIn("scripts/emit/resolve-oracle.mjs", wrapper)
        self.assertIn("--experimental-strip-types", wrapper)
        self.assertIn('capture_snapshot_provenance "$provenance_json"', wrapper)
        self.assertIn('capture_snapshot_provenance "$provenance_after"', wrapper)
        self.assertIn('cmp -s "$provenance_json" "$provenance_after"', wrapper)
        self.assertLess(
            wrapper.index('cmp -s "$provenance_json" "$provenance_after"'),
            wrapper.index('mv "$snapshot_tmp" "$snapshot_file"'),
        )
        self.assertIn('"status", "--porcelain", "--untracked-files=all"', provenance)
        self.assertIn('"--ignored=matching"', provenance)
        self.assertIn("build-manifest.py", provenance)
        self.assertIn("resolve-oracle.mjs", provenance)
        self.assertIn("candidate_content_sha256", provenance)
        for name in (
            "tsz",
            "tsz-server",
            "generate-tsc-cache",
            "tsz-conformance",
        ):
            self.assertIn(f'--binary "{name}=', wrapper)

    def test_snapshot_binary_subjects_cannot_be_selected_by_the_manifest(self):
        binaries = [
            "tsz=/repo/tsz",
            "tsz-server=/repo/tsz-server",
            "generate-tsc-cache=/repo/generate-tsc-cache",
            "tsz-conformance=/repo/tsz-conformance",
        ]
        self.assertEqual(len(PROVENANCE.validate_binary_specs(binaries)), 4)
        with self.assertRaisesRegex(ValueError, "four canonical"):
            PROVENANCE.validate_binary_specs(binaries[:-1])
        with self.assertRaisesRegex(ValueError, "four canonical"):
            PROVENANCE.validate_binary_specs(
                binaries[:-1] + ["substitute=/repo/tsz-conformance"]
            )

    def test_git_routing_environment_cannot_substitute_snapshot_identity(self):
        polluted = {
            "GIT_DIR": "/tmp/not-the-repository",
            "GIT_WORK_TREE": "/tmp/not-the-worktree",
            "GIT_INDEX_FILE": "/tmp/not-the-index",
        }
        with mock.patch.dict(os.environ, polluted):
            commit = PROVENANCE.git_output(ROOT, "rev-parse", "HEAD")
        self.assertRegex(commit, r"^[0-9a-f]{40}$")

    def test_snapshot_subset_guards_cover_all_selector_overrides(self):
        wrapper = (ROOT / "scripts/conformance/conformance.sh").read_text(
            encoding="utf-8"
        )
        for token in (
            "--filter",
            "--max",
            "-m",
            "--offset",
            "-o",
            "--shard",
            "--plan",
            "--test-dir",
            "--tsz-binary",
            "--mode",
        ):
            self.assertIn(token, wrapper)

    def test_snapshot_rejects_clustered_short_subset_selectors(self):
        script = ROOT / "scripts/conformance/conformance.sh"
        for argument in ("-vm1", "-vo1", "-vmo1"):
            result = subprocess.run(
                [str(script), "snapshot", argument],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0, argument)
            self.assertIn("clustered/unknown short", result.stderr, argument)


if __name__ == "__main__":
    unittest.main()
