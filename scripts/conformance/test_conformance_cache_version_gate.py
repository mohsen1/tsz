#!/usr/bin/env python3
"""The conformance wrapper must reject cache/compiler version drift."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
CONFORMANCE_SCRIPT = ROOT / "scripts" / "conformance" / "conformance.sh"


class ConformanceCacheVersionGateTests(unittest.TestCase):
    def test_mismatched_cache_version_fails_before_runner(self) -> None:
        with tempfile.TemporaryDirectory(prefix="tsz-conformance-cache-version-") as temp:
            fixture = Path(temp)
            script = fixture / "scripts" / "conformance" / "conformance.sh"
            script.parent.mkdir(parents=True)
            shutil.copy2(CONFORMANCE_SCRIPT, script)

            test_dir = fixture / "TypeScript" / "tests" / "cases"
            test_dir.mkdir(parents=True)
            test_dir.joinpath("example.ts").write_text(
                "const value = 1;\n",
                encoding="utf-8",
            )
            subprocess.run(
                ["git", "init", "-q", str(fixture / "TypeScript")],
                check=True,
            )
            subprocess.run(
                ["git", "-C", str(fixture / "TypeScript"), "add", "."],
                check=True,
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(fixture / "TypeScript"),
                    "-c",
                    "user.name=TSZ Test",
                    "-c",
                    "user.email=tsz-test@example.invalid",
                    "commit",
                    "-qm",
                    "fixture corpus",
                ],
                check=True,
            )
            corpus_pin = subprocess.check_output(
                ["git", "-C", str(fixture / "TypeScript"), "rev-parse", "HEAD"],
                text=True,
            ).strip()

            versions = {
                "current": corpus_pin,
                "mappings": {corpus_pin: {"npm": "7.0.2"}},
                "default": {"npm": "7.0.2"},
            }
            script.parent.joinpath("typescript-versions.json").write_text(
                json.dumps(versions) + "\n",
                encoding="utf-8",
            )

            ref_file = fixture / "scripts" / "ci" / "typescript-submodule-ref"
            ref_file.parent.mkdir(parents=True)
            ref_file.write_text(corpus_pin + "\n", encoding="utf-8")
            reset_helper = fixture / "scripts" / "setup" / "reset-ts-submodule.sh"
            reset_helper.parent.mkdir(parents=True)
            reset_helper.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            reset_helper.chmod(0o755)
            script.parent.joinpath("tsc-cache-full.json").write_text(
                json.dumps(
                    {
                        "compiler/example.ts": {
                            "metadata": {"typescript_version": "6.0.3"},
                        },
                    }
                )
                + "\n",
                encoding="utf-8",
            )

            binary_dir = fixture / ".target" / "dist-fast"
            binary_dir.mkdir(parents=True)
            for name in ("tsz", "tsz-server", "generate-tsc-cache", "tsz-conformance"):
                binary = binary_dir / name
                binary.write_text("#!/bin/sh\nexit 97\n", encoding="utf-8")
                binary.chmod(0o755)

            result = subprocess.run(
                [str(script), "run", "--test-dir", str(test_dir)],
                cwd=fixture,
                text=True,
                capture_output=True,
                env={**os.environ, "NO_COLOR": "1"},
            )

            output = result.stdout + result.stderr
            self.assertNotEqual(result.returncode, 0, output)
            self.assertIn("cache does not match the pinned TypeScript version", output)
            self.assertIn("Pinned version: 7.0.2", output)
            self.assertIn("sampleVersion=6.0.3", output)
            self.assertNotIn("Proceeding with stale cache", output)
            self.assertNotIn("Running conformance tests", output)


if __name__ == "__main__":
    unittest.main()
