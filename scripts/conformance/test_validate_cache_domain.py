#!/usr/bin/env python3
"""Focused tests for cache/domain artifact validation."""

from __future__ import annotations

import hashlib
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
    validate_portable_oracle_evidence,
)


VERSION = "7.0.2"
SCRIPT = Path(__file__).with_name("validate-cache-domain.py")
FINGERPRINT_KEYS = (
    "schemaVersion",
    "packageName",
    "platformPackageName",
    "version",
    "gitHead",
    "wrapperIntegrity",
    "platformIntegrity",
    "wrapperPackageJsonSha256",
    "wrapperBinSha256",
    "platformPackageJsonSha256",
    "platformPackageTreeSha256",
    "binarySha256",
    "binaryPath",
)


def oracle_manifest():
    return {
        "schemaVersion": 1,
        "packageName": "typescript",
        "platformPackagePrefix": "@typescript/typescript-",
        "version": VERSION,
        "gitHead": "f" * 40,
        "wrapperIntegrity": "sha512-wrapper",
        "wrapperPackageJsonSha256": "1" * 64,
        "wrapperBinSha256": "2" * 64,
        "platforms": {
            "linux-x64": {
                "packageIntegrity": "sha512-linux",
                "packageJsonSha256": "3" * 64,
                "packageTreeSha256": "4" * 64,
                "binarySha256": "5" * 64,
            },
            "darwin-arm64": {
                "packageIntegrity": "sha512-darwin",
                "packageJsonSha256": "7" * 64,
                "packageTreeSha256": "8" * 64,
                "binarySha256": "9" * 64,
            },
        },
    }


def oracle_evidence(platform: str, manifest_sha256: str):
    manifest = oracle_manifest()
    platform_manifest = manifest["platforms"][platform]
    package_name = manifest["platformPackagePrefix"] + platform
    executable = "tsc.exe" if platform.startswith("win32-") else "tsc"
    generator = {
        "schemaVersion": 1,
        "packageName": manifest["packageName"],
        "platformPackageName": package_name,
        "version": manifest["version"],
        "gitHead": manifest["gitHead"],
        "wrapperIntegrity": manifest["wrapperIntegrity"],
        "platformIntegrity": platform_manifest["packageIntegrity"],
        "wrapperPackageJsonSha256": manifest["wrapperPackageJsonSha256"],
        "wrapperBinSha256": manifest["wrapperBinSha256"],
        "platformPackageJsonSha256": platform_manifest["packageJsonSha256"],
        "platformPackageTreeSha256": platform_manifest["packageTreeSha256"],
        "binarySha256": platform_manifest["binarySha256"],
        "binaryPath": f"scripts/node_modules/{package_name}/lib/{executable}",
    }
    fingerprint_base = {key: generator[key] for key in FINGERPRINT_KEYS}
    encoded = json.dumps(fingerprint_base, separators=(",", ":")).encode()
    generator["fingerprint"] = "sha256:" + hashlib.sha256(encoded).hexdigest()
    return {
        "schemaVersion": 1,
        "manifestSha256": manifest_sha256,
        "generator": generator,
    }


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

    def test_portable_oracle_evidence_validates_recorded_platform_from_manifest(self):
        manifest_sha256 = "e" * 64
        evidence = oracle_evidence("linux-x64", manifest_sha256)

        identity = validate_portable_oracle_evidence(
            evidence,
            oracle_manifest(),
            manifest_sha256,
            VERSION,
        )

        self.assertEqual("typescript", identity["packageName"])
        self.assertEqual(VERSION, identity["version"])
        self.assertNotIn("platformPackageName", identity)
        self.assertNotIn("binarySha256", identity)

    def test_portable_oracle_evidence_rejects_recorded_platform_tampering(self):
        manifest_sha256 = "e" * 64
        evidence = oracle_evidence("linux-x64", manifest_sha256)
        evidence["generator"]["binarySha256"] = "9" * 64

        with self.assertRaises(CacheDomainValidationError) as context:
            validate_portable_oracle_evidence(
                evidence,
                oracle_manifest(),
                manifest_sha256,
                VERSION,
            )

        message = str(context.exception)
        self.assertIn("binarySha256 disagrees with manifest", message)
        self.assertIn("fingerprint is invalid", message)

    def test_portable_oracle_evidence_rejects_manifest_substitution(self):
        evidence = oracle_evidence("linux-x64", "e" * 64)

        with self.assertRaises(CacheDomainValidationError) as context:
            validate_portable_oracle_evidence(
                evidence,
                oracle_manifest(),
                "a" * 64,
                VERSION,
            )

        self.assertIn("manifest hash does not match", str(context.exception))

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
            oracle_manifest_path = root / "oracle-manifest.json"
            cache_path.write_text(json.dumps(cache), encoding="utf-8")
            versions_path.write_text(json.dumps(versions), encoding="utf-8")
            oracle_manifest_path.write_text(
                json.dumps(oracle_manifest()), encoding="utf-8"
            )
            oracle_manifest_sha256 = hashlib.sha256(
                oracle_manifest_path.read_bytes()
            ).hexdigest()
            domain["oracle"] = oracle_evidence(
                "linux-x64", oracle_manifest_sha256
            )
            domain_path.write_text(json.dumps(domain), encoding="utf-8")

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
                    "--oracle-manifest",
                    str(oracle_manifest_path),
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
