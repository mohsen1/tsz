#!/usr/bin/env python3
"""Write or verify exact conformance binary build-input provenance."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import subprocess
import sys
import tempfile
from pathlib import Path


INPUT_ROOTS = (
    "Cargo.toml",
    "Cargo.lock",
    "crates/tsz-core",
    "crates/tsz-cli",
    "crates/conformance",
)
OPTIONAL_INPUT_ROOTS = (".cargo", "rust-toolchain", "rust-toolchain.toml")
GIT_ROUTING_ENV = (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_REPLACE_REF_BASE",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


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
        raise ValueError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def input_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for relative in INPUT_ROOTS + tuple(
        path for path in OPTIONAL_INPUT_ROOTS if (root / path).exists()
    ):
        candidate = root / relative
        if not candidate.exists():
            raise ValueError(f"build input is missing: {relative}")
        if candidate.is_symlink():
            raise ValueError(f"build input symlink is unsupported: {relative}")
        if candidate.is_file():
            files.append(candidate)
            continue
        for current, directories, names in os.walk(candidate, followlinks=False):
            current_path = Path(current)
            for name in sorted(directories + names):
                entry = current_path / name
                mode = entry.lstat().st_mode
                if stat.S_ISLNK(mode):
                    raise ValueError(
                        f"build input symlink is unsupported: {entry.relative_to(root)}"
                    )
                if not stat.S_ISDIR(mode) and not stat.S_ISREG(mode):
                    raise ValueError(
                        f"unsupported build input: {entry.relative_to(root)}"
                    )
            files.extend(current_path / name for name in sorted(names))
    return sorted(files, key=lambda path: path.relative_to(root).as_posix())


def input_manifest(root: Path) -> dict:
    records = []
    framed = hashlib.sha256()
    framed.update(b"tsz-conformance-build-inputs-v1\0")
    for path in input_files(root):
        relative = path.relative_to(root).as_posix()
        file_hash = sha256_file(path)
        size = path.stat().st_size
        for value in (relative.encode(), bytes.fromhex(file_hash)):
            framed.update(len(value).to_bytes(8, "big"))
            framed.update(value)
        framed.update(size.to_bytes(8, "big"))
        records.append({"path": relative, "sha256": file_hash, "size": size})
    return {
        "sha256": framed.hexdigest(),
        "file_count": len(records),
        "files": records,
    }


def reject_ignored_build_inputs(root: Path) -> None:
    semantic_roots = list(INPUT_ROOTS) + [
        path for path in OPTIONAL_INPUT_ROOTS if (root / path).exists()
    ]
    status = git_output(
        root,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--ignored=matching",
        "--",
        *semantic_roots,
    )
    ignored = [line for line in status.splitlines() if line.startswith("!! ")]
    if ignored:
        raise ValueError(
            "ignored build inputs are not owned by the repository tree: "
            + "\n".join(ignored)
        )


def parse_binaries(root: Path, specs: list[str]) -> dict:
    binaries = {}
    for spec in specs:
        if "=" not in spec:
            raise ValueError(f"binary must be NAME=PATH, got {spec!r}")
        name, raw_path = spec.split("=", 1)
        if not name or name in binaries:
            raise ValueError(f"binary name is empty or duplicated: {name!r}")
        configured_path = Path(raw_path)
        if configured_path.is_symlink():
            raise ValueError(f"binary symlink is unsupported: {configured_path}")
        path = configured_path.resolve()
        if not path.is_file() or not os.access(path, os.X_OK):
            raise ValueError(f"binary is not one executable regular file: {path}")
        try:
            relative = path.relative_to(root).as_posix()
        except ValueError as error:
            raise ValueError(f"binary escapes repository root: {path}") from error
        binaries[name] = {
            "path": relative,
            "sha256": sha256_file(path),
            "size": path.stat().st_size,
        }
    if not binaries:
        raise ValueError("at least one binary is required")
    return dict(sorted(binaries.items()))


def snapshot(root: Path, specs: list[str]) -> dict:
    commit = git_output(root, "rev-parse", "HEAD")
    tree = git_output(root, "rev-parse", "HEAD^{tree}")
    if len(commit) != 40 or len(tree) != 40:
        raise ValueError("repository commit/tree identity is malformed")
    dirty = bool(git_output(root, "status", "--porcelain", "--untracked-files=all"))
    reject_ignored_build_inputs(root)
    return {
        "schema_version": 1,
        "repository": {"commit": commit, "tree": tree, "dirty": dirty},
        "inputs": input_manifest(root),
        "binaries": parse_binaries(root, specs),
    }


def write_atomic(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False
    ) as output:
        json.dump(value, output, indent=2, sort_keys=True)
        output.write("\n")
        temporary = Path(output.name)
    temporary.replace(path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("write", "verify"))
    parser.add_argument("--repo", required=True)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--binary", action="append", default=[])
    args = parser.parse_args()
    try:
        root = Path(args.repo).resolve(strict=True)
        manifest_path = Path(args.manifest)
        observed = snapshot(root, args.binary)
        if args.command == "write":
            write_atomic(manifest_path, observed)
        else:
            expected = json.loads(manifest_path.read_text(encoding="utf-8"))
            if expected != observed:
                raise ValueError("build inputs, repository identity, or binary hashes changed")
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"conformance build manifest error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
