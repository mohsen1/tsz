import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("build-manifest.py")


class BuildManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        for relative in (
            "crates/tsz-core/data",
            "crates/tsz-cli/src",
            "crates/conformance/src",
            ".target/debug",
        ):
            (self.root / relative).mkdir(parents=True)
        (self.root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
        (self.root / "Cargo.lock").write_text("version = 4\n", encoding="utf-8")
        (self.root / ".gitignore").write_text(".target/\n", encoding="utf-8")
        (self.root / "crates/tsz-core/data/lib.json").write_text(
            '{"pin": 1}\n', encoding="utf-8"
        )
        (self.root / "crates/tsz-cli/src/main.rs").write_text(
            "fn main() {}\n", encoding="utf-8"
        )
        (self.root / "crates/conformance/src/lib.rs").write_text(
            "pub fn run() {}\n", encoding="utf-8"
        )
        self.binary = self.root / ".target/debug/tsz"
        self.binary.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        self.binary.chmod(0o755)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.email", "test@example.invalid"],
            check=True,
        )
        subprocess.run(
            ["git", "-C", str(self.root), "config", "user.name", "Test"], check=True
        )
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "commit", "-qm", "fixture"], check=True
        )
        self.manifest = self.root / ".target/debug/manifest.json"

    def tearDown(self):
        self.temp.cleanup()

    def run_manifest(self, command, extra_env=None):
        environment = os.environ.copy()
        environment.update(extra_env or {})
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                command,
                "--repo",
                str(self.root),
                "--manifest",
                str(self.manifest),
                "--binary",
                f"tsz={self.binary}",
            ],
            text=True,
            capture_output=True,
            check=False,
            env=environment,
        )

    def test_non_rust_build_input_and_binary_are_independently_bound(self):
        result = self.run_manifest("write")
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        paths = {record["path"] for record in manifest["inputs"]["files"]}
        self.assertIn("crates/tsz-core/data/lib.json", paths)
        self.assertEqual(self.run_manifest("verify").returncode, 0)

        (self.root / "crates/tsz-core/data/lib.json").write_text(
            '{"pin": 2}\n', encoding="utf-8"
        )
        self.assertNotEqual(self.run_manifest("verify").returncode, 0)

        self.assertEqual(self.run_manifest("write").returncode, 0)
        self.binary.write_text("#!/bin/sh\nexit 1\n", encoding="utf-8")
        self.binary.chmod(0o755)
        self.assertNotEqual(self.run_manifest("verify").returncode, 0)

    def test_git_routing_environment_cannot_substitute_repository_identity(self):
        polluted = {
            "GIT_DIR": str(self.root / "not-the-repository"),
            "GIT_WORK_TREE": str(self.root / "also-not-the-worktree"),
            "GIT_INDEX_FILE": str(self.root / "not-the-index"),
        }
        result = self.run_manifest("write", polluted)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.run_manifest("verify", polluted).returncode, 0)

    def test_ignored_build_input_is_not_owned_by_the_repository_tree(self):
        with (self.root / ".gitignore").open("a", encoding="utf-8") as ignore:
            ignore.write("crates/tsz-core/data/ignored.json\n")
        (self.root / "crates/tsz-core/data/ignored.json").write_text(
            '{"hidden": true}\n', encoding="utf-8"
        )
        result = self.run_manifest("write")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ignored build inputs", result.stderr)

    def test_nonignored_untracked_build_input_is_hashed_for_dirty_observation(self):
        untracked = self.root / "crates/conformance/src/new_rewrite_module.rs"
        untracked.write_text("pub fn new_module() {}\n", encoding="utf-8")
        result = self.run_manifest("write")
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        paths = {record["path"] for record in manifest["inputs"]["files"]}
        self.assertIn("crates/conformance/src/new_rewrite_module.rs", paths)

    def test_git_replace_object_cannot_substitute_build_tree_identity(self):
        original = subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True
        ).strip()
        (self.root / "crates/tsz-core/data/lib.json").write_text(
            '{"pin": "replacement"}\n', encoding="utf-8"
        )
        subprocess.run(["git", "-C", str(self.root), "add", "."], check=True)
        subprocess.run(
            ["git", "-C", str(self.root), "commit", "-qm", "replacement"],
            check=True,
        )
        replacement = subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True
        ).strip()
        subprocess.run(
            ["git", "-C", str(self.root), "switch", "-q", "--detach", original],
            check=True,
        )
        subprocess.run(
            ["git", "-C", str(self.root), "replace", original, replacement], check=True
        )
        replaced_tree = subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD^{tree}"], text=True
        ).strip()
        exact_environment = os.environ.copy()
        exact_environment["GIT_NO_REPLACE_OBJECTS"] = "1"
        exact_tree = subprocess.check_output(
            ["git", "-C", str(self.root), "rev-parse", "HEAD^{tree}"],
            text=True,
            env=exact_environment,
        ).strip()
        self.assertNotEqual(replaced_tree, exact_tree)

        result = self.run_manifest("write")
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads(self.manifest.read_text(encoding="utf-8"))
        self.assertEqual(manifest["repository"]["tree"], exact_tree)


if __name__ == "__main__":
    unittest.main()
