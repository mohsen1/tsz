"""Validation helpers for the checked-in TSC cache and conformance domain."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


def _lower_hex(value: Any, length: int) -> bool:
    return (
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value)
    )


class CacheDomainValidationError(ValueError):
    """Raised when cache/domain artifacts do not describe one exact partition."""

    def __init__(self, errors: list[str]):
        self.errors = tuple(sorted(set(errors)))
        super().__init__("; ".join(self.errors))


@dataclass(frozen=True)
class CacheDomainSummary:
    typescript_version: str
    candidates: int
    runnable: int
    unsupported: int
    skipped: int


def load_json_object(path: Path, label: str) -> dict[str, Any]:
    """Load a JSON object with stable, actionable validation errors."""

    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise CacheDomainValidationError(
            [f"cannot read {label} at {path}: {error}"]
        ) from error
    except json.JSONDecodeError as error:
        raise CacheDomainValidationError(
            [f"cannot parse {label} at {path}: {error}"]
        ) from error
    if not isinstance(value, dict):
        raise CacheDomainValidationError([f"{label} must be a JSON object"])
    return value


def resolve_pinned_typescript_version(versions: dict[str, Any]) -> str:
    """Resolve the exact npm version paired with the active corpus pin."""

    current = versions.get("current")
    mappings = versions.get("mappings")
    mapping = mappings.get(current) if isinstance(mappings, dict) else None
    version = mapping.get("npm") if isinstance(mapping, dict) else None
    if not isinstance(current, str) or not current:
        raise CacheDomainValidationError(
            ["typescript-versions.json must contain a non-empty current corpus pin"]
        )
    if not isinstance(version, str) or not version:
        raise CacheDomainValidationError(
            [f"typescript-versions.json has no npm version for current pin {current}"]
        )
    return version


def _count(domain: dict[str, Any], key: str, errors: list[str]) -> int | None:
    value = domain.get(key)
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        errors.append(f"domain {key} must be a non-negative integer")
        return None
    return value


def _string_map(
    domain: dict[str, Any], key: str, errors: list[str]
) -> dict[str, str] | None:
    value = domain.get(key)
    if not isinstance(value, dict):
        errors.append(f"domain {key} must be an object")
        return None
    invalid = sorted(
        path
        for path, reason in value.items()
        if not isinstance(path, str)
        or not path
        or not isinstance(reason, str)
        or not reason
    )
    if invalid:
        errors.append(
            f"domain {key} has {len(invalid)} invalid path/reason entries; "
            f"first={invalid[0]!r}"
        )
    return value


def validate_cache_domain(
    cache: dict[str, Any],
    domain: dict[str, Any],
    pinned_version: str,
) -> CacheDomainSummary:
    """Validate version identity and the candidate partition encoded by artifacts."""

    errors: list[str] = []
    if not cache:
        errors.append("TSC cache must contain at least one runnable entry")

    invalid_cache_keys = sorted(
        repr(path) for path in cache if not isinstance(path, str) or not path
    )
    if invalid_cache_keys:
        errors.append(
            f"TSC cache has {len(invalid_cache_keys)} invalid keys; "
            f"first={invalid_cache_keys[0]}"
        )

    missing_metadata: list[str] = []
    wrong_versions: list[tuple[str, Any]] = []
    invalid_results: list[str] = []
    incomplete_evidence: list[str] = []
    for path, entry in cache.items():
        if not isinstance(path, str):
            continue
        if not isinstance(entry, dict):
            invalid_results.append(path)
            continue
        metadata = entry.get("metadata")
        if not isinstance(metadata, dict):
            missing_metadata.append(path)
        else:
            actual_version = metadata.get("typescript_version")
            if actual_version != pinned_version:
                wrong_versions.append((path, actual_version))
            if not _lower_hex(metadata.get("source_sha256"), 64):
                invalid_results.append(path)
        error_codes = entry.get("error_codes")
        fingerprints = entry.get("diagnostic_fingerprints")
        exits = entry.get("ordinary_exit_statuses")
        if (
            not isinstance(error_codes, list)
            or any(isinstance(code, bool) or not isinstance(code, int) for code in error_codes)
            or not isinstance(fingerprints, list)
            or any(not isinstance(fingerprint, dict) for fingerprint in fingerprints)
        ):
            invalid_results.append(path)
        elif error_codes != [fingerprint.get("code") for fingerprint in fingerprints]:
            invalid_results.append(path)
        if entry.get("diagnostic_blocks_complete") is not True or (
            not isinstance(exits, list)
            or not exits
            or any(
                isinstance(status, bool)
                or not isinstance(status, int)
                or status not in (0, 1, 2)
                for status in exits
            )
        ):
            incomplete_evidence.append(path)

    if missing_metadata:
        errors.append(
            f"{len(missing_metadata)} cache entries lack metadata; "
            f"first={sorted(missing_metadata)[0]}"
        )
    if wrong_versions:
        path, actual = sorted(wrong_versions, key=lambda item: item[0])[0]
        errors.append(
            f"{len(wrong_versions)} cache entries do not use TypeScript "
            f"{pinned_version}; first={path} version={actual!r}"
        )
    if invalid_results:
        errors.append(
            f"{len(set(invalid_results))} cache entries have invalid result payloads; "
            f"first={sorted(set(invalid_results))[0]}"
        )
    if incomplete_evidence:
        errors.append(
            f"{len(incomplete_evidence)} cache entries lack complete diagnostic blocks "
            f"or exact ordinary exit status; first={sorted(incomplete_evidence)[0]}"
        )

    domain_version = domain.get("typescript_version")
    if domain_version != pinned_version:
        errors.append(
            f"domain TypeScript version must be {pinned_version}, got {domain_version!r}"
        )
    if domain.get("schema_version") != 2:
        errors.append("domain schema_version must be 2")
    if not _lower_hex(domain.get("corpus_commit"), 40):
        errors.append("domain corpus_commit must be 40 lowercase hex bytes")
    if not _lower_hex(domain.get("corpus_tree"), 40):
        errors.append("domain corpus_tree must be 40 lowercase hex bytes")
    if not _lower_hex(domain.get("candidate_content_sha256"), 64):
        errors.append("domain candidate_content_sha256 must be 64 lowercase hex bytes")

    oracle = domain.get("oracle")
    if not isinstance(oracle, dict) or set(oracle) != {
        "schemaVersion",
        "manifestSha256",
        "generator",
    }:
        errors.append("domain oracle evidence has an invalid envelope")
    else:
        if oracle.get("schemaVersion") != 1 or not _lower_hex(
            oracle.get("manifestSha256"), 64
        ):
            errors.append("domain oracle manifest identity is invalid")
        generator = oracle.get("generator")
        expected_generator_keys = {
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
            "fingerprint",
        }
        if not isinstance(generator, dict) or set(generator) != expected_generator_keys:
            errors.append("domain oracle generator provenance has an invalid schema")
        else:
            if generator.get("schemaVersion") != 1:
                errors.append("domain oracle generator schemaVersion must be 1")
            if generator.get("version") != pinned_version:
                errors.append("domain oracle generator version does not match the pin")
            for key in (
                "wrapperPackageJsonSha256",
                "wrapperBinSha256",
                "platformPackageJsonSha256",
                "platformPackageTreeSha256",
                "binarySha256",
            ):
                if not _lower_hex(generator.get(key), 64):
                    errors.append(f"domain oracle {key} must be 64 lowercase hex bytes")
            fingerprint = generator.get("fingerprint")
            if not isinstance(fingerprint, str) or not _lower_hex(
                fingerprint.removeprefix("sha256:"), 64
            ) or not fingerprint.startswith("sha256:"):
                errors.append("domain oracle fingerprint is invalid")

    candidate_count = _count(domain, "candidate_count", errors)
    runnable_count = _count(domain, "runnable_count", errors)
    unsupported_count = _count(domain, "unsupported_count", errors)
    skipped_count = _count(domain, "skipped_count", errors)
    unsupported = _string_map(domain, "unsupported", errors)
    skipped = _string_map(domain, "skipped", errors)

    cache_keys = {path for path in cache if isinstance(path, str)}
    unsupported_keys = set(unsupported or {})
    skipped_keys = set(skipped or {})
    overlaps = {
        "cache/unsupported": cache_keys & unsupported_keys,
        "cache/skipped": cache_keys & skipped_keys,
        "unsupported/skipped": unsupported_keys & skipped_keys,
    }
    for label, paths in overlaps.items():
        if paths:
            errors.append(
                f"domain partitions {label} overlap at {len(paths)} paths; "
                f"first={sorted(paths)[0]}"
            )

    if runnable_count is not None and runnable_count != len(cache_keys):
        errors.append(
            f"domain runnable_count={runnable_count} but cache has {len(cache_keys)} entries"
        )
    if unsupported is not None and unsupported_count is not None:
        if unsupported_count != len(unsupported_keys):
            errors.append(
                f"domain unsupported_count={unsupported_count} but unsupported has "
                f"{len(unsupported_keys)} entries"
            )
    if skipped is not None and skipped_count is not None:
        if skipped_count != len(skipped_keys):
            errors.append(
                f"domain skipped_count={skipped_count} but skipped has "
                f"{len(skipped_keys)} entries"
            )

    union_size = len(cache_keys | unsupported_keys | skipped_keys)
    if candidate_count is not None and candidate_count != union_size:
        errors.append(
            f"domain candidate_count={candidate_count} but partition union has "
            f"{union_size} entries"
        )
    counts = (runnable_count, unsupported_count, skipped_count)
    if candidate_count is not None and all(value is not None for value in counts):
        partition_count = sum(value for value in counts if value is not None)
        if candidate_count != partition_count:
            errors.append(
                f"domain candidate_count={candidate_count} but declared partition counts "
                f"sum to {partition_count}"
            )

    if errors:
        raise CacheDomainValidationError(errors)

    assert candidate_count is not None
    assert runnable_count is not None
    assert unsupported_count is not None
    assert skipped_count is not None
    return CacheDomainSummary(
        typescript_version=pinned_version,
        candidates=candidate_count,
        runnable=runnable_count,
        unsupported=unsupported_count,
        skipped=skipped_count,
    )
