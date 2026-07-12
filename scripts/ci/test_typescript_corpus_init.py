#!/usr/bin/env python3
"""Focused safety tests for full CI's standalone TypeScript corpus setup."""

from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
CORPUS_HELPER = REPO_ROOT / "scripts/ci/lib/typescript-corpus.sh"
PINNED_REF = "a" * 40


class TypeScriptCorpusInitTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="tsz-ci-corpus-")
        self.root = Path(self.temp.name)
        (self.root / "scripts/ci").mkdir(parents=True)
        (self.root / "scripts/setup").mkdir(parents=True)
        (self.root / "scripts/ci/typescript-submodule-ref").write_text(
            f"{PINNED_REF}\n", encoding="utf-8"
        )
        reset_helper = self.root / "scripts/setup/reset-ts-submodule.sh"
        reset_helper.write_text(
            """#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
printf '%s\\n' "$*" >> "$root_dir/reset-helper.log"
mkdir -p "$root_dir/TypeScript/src/lib"
: > "$root_dir/TypeScript/src/lib/es5.d.ts"
""",
            encoding="utf-8",
        )
        reset_helper.chmod(0o755)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def run_init(
        self, *, github_actions: bool = False
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.pop("GITHUB_ACTIONS", None)
        if github_actions:
            env["GITHUB_ACTIONS"] = "true"
        return subprocess.run(
            [
                "bash",
                "-c",
                'set -Eeuo pipefail; ROOT_DIR="$1"; source "$2"; '
                "materialize_typescript_corpus",
                "bash",
                str(self.root),
                str(CORPUS_HELPER),
            ],
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def helper_calls(self) -> list[str]:
        log = self.root / "reset-helper.log"
        return log.read_text(encoding="utf-8").splitlines() if log.exists() else []

    def test_absent_corpus_routes_through_guarded_reset_helper(self) -> None:
        result = self.run_init()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.helper_calls(), ["--sparse"])

    def test_existing_git_checkout_routes_through_helper_without_deletion(self) -> None:
        corpus = self.root / "TypeScript"
        (corpus / ".git").mkdir(parents=True)
        (corpus / ".tsz-cache-ref").write_text(
            f"{'b' * 40}\n", encoding="utf-8"
        )
        sentinel = corpus / "local-work.txt"
        sentinel.write_text("keep me\n", encoding="utf-8")

        result = self.run_init()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.helper_calls(), ["--sparse"])
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep me\n")

    def test_complete_source_cache_is_reused_without_reset(self) -> None:
        corpus = self.root / "TypeScript"
        (corpus / "src/lib").mkdir(parents=True)
        (corpus / ".tsz-cache-ref").write_text(f"{PINNED_REF}\n", encoding="utf-8")
        (corpus / "src/lib/es5.d.ts").write_text("cached\n", encoding="utf-8")

        result = self.run_init()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.helper_calls(), [])
        self.assertIn("Using cached TypeScript source tree", result.stdout)

    def test_stale_local_source_cache_is_refused_without_deletion(self) -> None:
        corpus = self.root / "TypeScript"
        corpus.mkdir()
        (corpus / ".tsz-cache-ref").write_text(f"{'b' * 40}\n", encoding="utf-8")
        sentinel = corpus / "local-work.txt"
        sentinel.write_text("keep me\n", encoding="utf-8")

        result = self.run_init()

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refusing to delete stale local", result.stderr)
        self.assertEqual(self.helper_calls(), [])
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "keep me\n")

    def test_stale_github_actions_cache_is_replaced_via_helper(self) -> None:
        corpus = self.root / "TypeScript"
        corpus.mkdir()
        (corpus / ".tsz-cache-ref").write_text(f"{'b' * 40}\n", encoding="utf-8")
        sentinel = corpus / "stale-cache-file.txt"
        sentinel.write_text("discardable\n", encoding="utf-8")

        result = self.run_init(github_actions=True)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.helper_calls(), ["--sparse"])
        self.assertFalse(sentinel.exists())
        self.assertTrue((corpus / "src/lib/es5.d.ts").is_file())


if __name__ == "__main__":
    unittest.main()
