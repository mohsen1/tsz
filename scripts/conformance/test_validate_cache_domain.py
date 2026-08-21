#!/usr/bin/env python3
"""Focused tests for cache/domain artifact validation."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from lib.cache_domain import (
    CacheDomainValidationError,
    resolve_pinned_typescript_version,
    validate_cache_domain,
)


VERSION = "7.0.2"
SCRIPT = Path(__file__).with_name("validate-cache-domain.py")


def cache_entry(version: str = VERSION):
    return {
        "metadata": {
            "mtime_ms": 1,
            "size": 2,
            "typescript_version": version,
            "source_sha256": "a" * 64,
        },
        "error_codes": [2322],
        "diagnostic_fingerprints": [
            {
                "code": 2322,
                "file": "a.ts",
                "line": 1,
                "column": 1,
                "message_key": "mismatch",
            }
        ],
        "diagnostic_blocks_complete": True,
        "ordinary_exit_statuses": [1],
    }


def valid_artifacts():
    cache = {
        "compiler/a.ts": cache_entry(),
        "compiler/b.ts": cache_entry(),
    }
    domain = {
        "schema_version": 2,
        "typescript_version": VERSION,
        "corpus_commit": "b" * 40,
        "corpus_tree": "c" * 40,
        "candidate_content_sha256": "d" * 64,
        "oracle": {
            "schemaVersion": 1,
            "manifestSha256": "e" * 64,
            "generator": {
                "schemaVersion": 1,
                "packageName": "typescript",
                "platformPackageName": "@typescript/typescript-linux-x64",
                "version": VERSION,
                "gitHead": "f" * 40,
                "wrapperIntegrity": "sha512-wrapper",
                "platformIntegrity": "sha512-platform",
                "wrapperPackageJsonSha256": "1" * 64,
                "wrapperBinSha256": "2" * 64,
                "platformPackageJsonSha256": "3" * 64,
                "platformPackageTreeSha256": "4" * 64,
                "binarySha256": "5" * 64,
                "binaryPath": "scripts/node_modules/@typescript/typescript-linux-x64/lib/tsc",
                "fingerprint": "sha256:" + "6" * 64,
            },
        },
        "candidate_count": 4,
        "runnable_count": 2,
        "unsupported_count": 1,
        "skipped_count": 1,
        "unsupported": {
            "compiler/legacy.ts": "typescript-7-unsupported-configuration",
        },
        "skipped": {"compiler/skip.ts": "@skip"},
    }
    return cache, domain


class CacheDomainValidationTests(unittest.TestCase):
    def test_accepts_exact_disjoint_partition(self):
        cache, domain = valid_artifacts()

        summary = validate_cache_domain(cache, domain, VERSION)

        self.assertEqual(4, summary.candidates)
        self.assertEqual(2, summary.runnable)
        self.assertEqual(1, summary.unsupported)
        self.assertEqual(1, summary.skipped)

    def test_rejects_cache_and_domain_version_drift(self):
        cache, domain = valid_artifacts()
        cache["compiler/a.ts"]["metadata"]["typescript_version"] = "6.0.3"
        domain["typescript_version"] = "6.0.3"

        with self.assertRaises(CacheDomainValidationError) as context:
            validate_cache_domain(cache, domain, VERSION)

        message = str(context.exception)
        self.assertIn("cache entries do not use TypeScript 7.0.2", message)
        self.assertIn("domain TypeScript version must be 7.0.2", message)

    def test_rejects_overlap_and_partition_count_drift(self):
        cache, domain = valid_artifacts()
        domain["unsupported"]["compiler/a.ts"] = (
            "typescript-7-unsupported-configuration"
        )
        domain["candidate_count"] = 99

        with self.assertRaises(CacheDomainValidationError) as context:
            validate_cache_domain(cache, domain, VERSION)

        message = str(context.exception)
        self.assertIn("cache/unsupported overlap", message)
        self.assertIn("candidate_count=99", message)
        self.assertIn("unsupported_count=1", message)

    def test_rejects_invalid_cache_result_payload(self):
        cache, domain = valid_artifacts()
        cache["compiler/a.ts"]["error_codes"] = ["TS2322"]

        with self.assertRaises(CacheDomainValidationError) as context:
            validate_cache_domain(cache, domain, VERSION)

        self.assertIn("invalid result payloads", str(context.exception))

    def test_rejects_source_or_candidate_content_identity_drift(self):
        cache, domain = valid_artifacts()
        cache["compiler/a.ts"]["metadata"]["source_sha256"] = "not-a-hash"
        domain["candidate_content_sha256"] = "0" * 63

        with self.assertRaises(CacheDomainValidationError) as context:
            validate_cache_domain(cache, domain, VERSION)

        message = str(context.exception)
        self.assertIn("invalid result payloads", message)
        self.assertIn("candidate_content_sha256", message)

    def test_rejects_incomplete_evidence_even_for_clean_rows(self):
        cache, domain = valid_artifacts()
        cache["compiler/a.ts"].update(
            error_codes=[], diagnostic_fingerprints=[], ordinary_exit_statuses=[0]
        )
        cache["compiler/a.ts"]["diagnostic_blocks_complete"] = False

        with self.assertRaises(CacheDomainValidationError) as context:
            validate_cache_domain(cache, domain, VERSION)

        self.assertIn("lack complete diagnostic blocks", str(context.exception))

    def test_rejects_missing_or_nonordinary_exit_status(self):
        for exits in ([], [3], [101]):
            cache, domain = valid_artifacts()
            cache["compiler/a.ts"]["ordinary_exit_statuses"] = exits
            with self.subTest(exits=exits), self.assertRaises(
                CacheDomainValidationError
            ):
                validate_cache_domain(cache, domain, VERSION)

    def test_resolves_version_only_from_current_mapping(self):
        versions = {
            "current": "corpus-sha",
            "mappings": {"corpus-sha": {"npm": VERSION}},
            "default": {"npm": "0.0.0"},
        }

        self.assertEqual(VERSION, resolve_pinned_typescript_version(versions))

    def test_cli_reports_stable_partition_summary(self):
        cache, domain = valid_artifacts()
        versions = {
            "current": "corpus-sha",
            "mappings": {"corpus-sha": {"npm": VERSION}},
        }
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            cache_path = root / "cache.json"
            domain_path = root / "domain.json"
            versions_path = root / "versions.json"
            cache_path.write_text(json.dumps(cache), encoding="utf-8")
            domain_path.write_text(json.dumps(domain), encoding="utf-8")
            versions_path.write_text(json.dumps(versions), encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--cache",
                    str(cache_path),
                    "--domain",
                    str(domain_path),
                    "--versions",
                    str(versions_path),
                ],
                text=True,
                capture_output=True,
                check=False,
            )

        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(
            "ok: TypeScript 7.0.2 domain has 4 candidates = "
            "2 runnable + 1 unsupported + 1 skipped\n",
            result.stdout,
        )


if __name__ == "__main__":
    unittest.main()
