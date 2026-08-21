#!/usr/bin/env python3
"""Capture fail-closed, reproducible provenance for a tracked snapshot."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from lib.cache_domain import load_json_object_bytes, validate_portable_oracle_evidence

EXPECTED_BINARY_NAMES = {
    "generate-tsc-cache",
    "tsz",
    "tsz-conformance",
    "tsz-server",
}
GIT_ROUTING_ENV = (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_REPLACE_REF_BASE",
)


def lower_hex(value, length: int) -> bool:
    return (
        isinstance(value, str)
        and len(value) == length
        and all(character in "0123456789abcdef" for character in value)
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    value = digest.hexdigest()
    if not lower_hex(value, 64):
        raise ValueError(f"invalid SHA-256 for {path}")
    return value


def run_text(argv: list[str]) -> str:
    result = subprocess.run(argv, text=True, capture_output=True, check=False)
    if result.returncode:
        raise ValueError(
            f"command failed ({result.returncode}): {' '.join(argv)}: "
            f"{result.stderr.strip()}"
        )
    return result.stdout.strip()


def git_output(root: Path, *args: str) -> str:
    environment = os.environ.copy()
    for name in GIT_ROUTING_ENV:
        environment.pop(name, None)
    environment["GIT_NO_REPLACE_OBJECTS"] = "1"
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        text=True,
        capture_output=True,
        check=False,
        env=environment,
    )
    if result.returncode:
        raise ValueError(
            f"git {' '.join(args)} failed ({result.returncode}): "
            f"{result.stderr.strip()}"
        )
    return result.stdout.strip()


def relative(root: Path, path: Path) -> str:
    try:
        return path.resolve(strict=True).relative_to(root).as_posix()
    except ValueError as error:
        raise ValueError(f"provenance path escapes repository: {path}") from error


def validate_binary_specs(specs: list[str]) -> list[str]:
    names = []
    for spec in specs:
        if "=" not in spec:
            raise ValueError(f"binary must be NAME=PATH, got {spec!r}")
        name, path = spec.split("=", 1)
        if not name or not path or name in names:
            raise ValueError(f"binary identity is empty or duplicated: {name!r}")
        names.append(name)
    if set(names) != EXPECTED_BINARY_NAMES or len(names) != len(EXPECTED_BINARY_NAMES):
        raise ValueError("snapshot must bind the four canonical conformance binaries")
    return sorted(specs)


def verify_build_manifest(root: Path, manifest_path: Path, binary_specs: list[str]) -> dict:
    binary_specs = validate_binary_specs(binary_specs)
    command = [
        sys.executable,
        str(root / "scripts/conformance/build-manifest.py"),
        "verify",
        "--repo",
        str(root),
        "--manifest",
        str(manifest_path),
    ]
    for spec in binary_specs:
        command.extend(["--binary", spec])
    run_text(command)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    binaries = manifest.get("binaries")
    if not isinstance(binaries, dict) or set(binaries) != EXPECTED_BINARY_NAMES:
        raise ValueError("build manifest does not bind the canonical binary set")
    return manifest


def validate_portable_oracle_boundary(
    domain_oracle: object,
    runtime_oracle: object,
    oracle_manifest: dict,
    oracle_manifest_sha256: str,
    pinned_version: str,
) -> None:
    """Bind cache and runtime oracles to one release, not one native package."""

    domain_identity = validate_portable_oracle_evidence(
        domain_oracle,
        oracle_manifest,
        oracle_manifest_sha256,
        pinned_version,
    )
    runtime_evidence = {
        "schemaVersion": 1,
        "manifestSha256": oracle_manifest_sha256,
        "generator": runtime_oracle,
    }
    runtime_identity = validate_portable_oracle_evidence(
        runtime_evidence,
        oracle_manifest,
        oracle_manifest_sha256,
        pinned_version,
    )
    if domain_identity != runtime_identity:
        raise ValueError(
            "domain and runtime oracles have different platform-neutral identities"
        )


def capture(args) -> dict:
    root = Path(args.repo).resolve(strict=True)
    test_dir = Path(args.test_dir).resolve(strict=True)
    expected_test_dir = (root / "TypeScript/tests/cases").resolve(strict=True)
    if test_dir != expected_test_dir:
        raise ValueError("tracked snapshot test directory is not the pinned corpus")

    commit = git_output(root, "rev-parse", "HEAD")
    tree = git_output(root, "rev-parse", "HEAD^{tree}")
    if not lower_hex(commit, 40) or not lower_hex(tree, 40):
        raise ValueError("repository commit/tree identity is malformed")
    dirty = git_output(root, "status", "--porcelain", "--untracked-files=all")
    if dirty:
        raise ValueError("tracked snapshots require an entirely clean repository")

    corpus_root = root / "TypeScript"
    corpus_commit = git_output(corpus_root, "rev-parse", "HEAD")
    corpus_tree = git_output(corpus_root, "rev-parse", "HEAD^{tree}")
    corpus_pin = (root / "scripts/ci/typescript-submodule-ref").read_text().strip()
    if (
        not lower_hex(corpus_pin, 40)
        or corpus_commit != corpus_pin
        or not lower_hex(corpus_tree, 40)
    ):
        raise ValueError("TypeScript corpus commit/tree does not match the pin")
    corpus_dirty = git_output(
        corpus_root, "status", "--porcelain", "--untracked-files=all"
    )
    if corpus_dirty:
        raise ValueError("tracked snapshots require a pristine TypeScript corpus")
    ignored_candidates = git_output(
        corpus_root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--ignored=matching",
        "--",
        "tests/cases",
        "tests/lib",
    )
    if ignored_candidates:
        raise ValueError(
            "TypeScript semantic inputs contain ignored candidates not owned by the pinned tree"
        )

    build_manifest_path = Path(args.build_manifest).resolve(strict=True)
    build_manifest = verify_build_manifest(root, build_manifest_path, args.binary)
    cache_path = Path(args.cache).resolve(strict=True)
    domain_path = Path(args.domain).resolve(strict=True)
    domain = json.loads(domain_path.read_text(encoding="utf-8"))
    if (
        domain.get("schema_version") != 2
        or domain.get("corpus_commit") != corpus_commit
        or domain.get("corpus_tree") != corpus_tree
        or not lower_hex(domain.get("candidate_content_sha256"), 64)
    ):
        raise ValueError("domain corpus/content identity is incomplete or stale")

    manifest_path = root / "scripts/emit/oracle-manifest.json"
    oracle_manifest, oracle_manifest_bytes = load_json_object_bytes(
        manifest_path, "oracle manifest"
    )
    oracle_manifest_sha256 = hashlib.sha256(oracle_manifest_bytes).hexdigest()
    pinned_version = oracle_manifest.get("version")
    if not isinstance(pinned_version, str) or not pinned_version:
        raise ValueError("checked-in oracle manifest has no pinned TypeScript version")
    if domain.get("typescript_version") != pinned_version:
        raise ValueError("domain TypeScript version differs from the oracle manifest")

    resolver = root / "scripts/emit/resolve-oracle.mjs"
    oracle = json.loads(
        run_text(
            [
                "node",
                "--experimental-strip-types",
                str(resolver),
                "--root",
                str(root),
            ]
        )
    )
    oracle_provenance = oracle.get("provenance")
    if not isinstance(oracle_provenance, dict) or not lower_hex(
        oracle_provenance.get("binarySha256"), 64
    ):
        raise ValueError("verified native oracle returned incomplete provenance")
    configured_lib = os.environ.get("TSZ_LIB_DIR")
    binary_path = oracle.get("binaryPath")
    if not configured_lib or not isinstance(binary_path, str):
        raise ValueError("snapshot has no exact TSZ_LIB_DIR/native oracle path")
    lib_path = Path(configured_lib).resolve(strict=True)
    if lib_path != Path(binary_path).resolve(strict=True).parent:
        raise ValueError("TSZ_LIB_DIR is not the verified native oracle library tree")
    domain_oracle = domain.get("oracle")
    validate_portable_oracle_boundary(
        domain_oracle,
        oracle_provenance,
        oracle_manifest,
        oracle_manifest_sha256,
        pinned_version,
    )

    return {
        "schema_version": 2,
        "git": {"commit": commit, "tree": tree, "dirty": False},
        "build_manifest": {
            "path": relative(root, build_manifest_path),
            "sha256": sha256_file(build_manifest_path),
            "inputs_sha256": build_manifest["inputs"]["sha256"],
            "binaries": build_manifest["binaries"],
        },
        "oracle_cache": {
            "path": relative(root, cache_path),
            "sha256": sha256_file(cache_path),
        },
        "domain": {
            "path": relative(root, domain_path),
            "sha256": sha256_file(domain_path),
            "candidate_content_sha256": domain["candidate_content_sha256"],
            "oracle": domain_oracle,
        },
        "runtime_oracle": oracle_provenance,
        "typescript_lib": {
            "path": relative(root, lib_path),
            "package_tree_sha256": oracle_provenance["platformPackageTreeSha256"],
        },
        "oracle_manifest": {
            "path": relative(root, manifest_path),
            "sha256": oracle_manifest_sha256,
        },
        "corpus": {"commit": corpus_commit, "tree": corpus_tree, "dirty": False},
        "selection": {
            "mode": "fresh",
            "full_domain": True,
            "test_dir": "TypeScript/tests/cases",
            "workers": args.workers,
            "runner_args": args.runner_arg,
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True)
    parser.add_argument("--test-dir", required=True)
    parser.add_argument("--cache", required=True)
    parser.add_argument("--domain", required=True)
    parser.add_argument("--build-manifest", required=True)
    parser.add_argument("--binary", action="append", default=[])
    parser.add_argument("--workers", required=True, type=int)
    parser.add_argument("--runner-arg", action="append", default=[])
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    try:
        if args.workers <= 0:
            raise ValueError("workers must be positive")
        provenance = capture(args)
        output_path = Path(args.output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(
            "w", encoding="utf-8", dir=output_path.parent, delete=False
        ) as output:
            json.dump(provenance, output, sort_keys=True)
            output.write("\n")
            temporary = Path(output.name)
        temporary.replace(output_path)
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"snapshot provenance error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
