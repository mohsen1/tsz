#!/usr/bin/env python3
"""Guard against orphaned `crates/*/tests/*.rs` files (#16013).

`tsz-checker`, `tsz-cli`, `tsz-emitter`, `tsz-binder`, `tsz-solver`, and other
crates set `autotests = false`, so under Cargo a file in `tests/` only builds
into a target through one of two mechanisms: an explicit `[[test]]` stanza in
the crate's `Cargo.toml`, or a `#[path = "../tests/<file>.rs"] mod ...;`
include from a `src/` file. A file reached by neither still compiles under
`cargo check` and looks like a live suite, but its `#[test]` functions never
run and nothing reports the gap — `cargo nextest list` simply omits them.

#16013 found 11 such files (74 `#[test]` fns) already orphaned on main. This
script does not resurrect them (stale frozen expectations are a repeat
failure mode in this repo — see #16001/#16002/#16005 and the #15632 revert);
it only stops the set from growing. Known orphans are tracked in
`scripts/ci/orphaned-test-files-baseline.txt`, the same shrink-only-baseline
shape as `scripts/ci/known-failures.txt`: removing an entry (because the file
was registered or deleted) is always fine, adding one is not — fix the
reachability or delete the dead file instead.

Usage:
    python3 scripts/ci/check-test-file-reachability.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT_DIR = pathlib.Path(__file__).resolve().parents[2]
CRATES_DIR = ROOT_DIR / "crates"
BASELINE_PATH = ROOT_DIR / "scripts" / "ci" / "orphaned-test-files-baseline.txt"

TEST_ATTR_RE = re.compile(r"#\[test\]")
PATH_MOD_RE = re.compile(
    r'#\[path\s*=\s*"([^"]+)"\]\s*\n\s*(?:pub\s+)?mod\s+\w+\s*;'
)
CARGO_TEST_STANZA_RE = re.compile(
    r'\[\[test\]\]\s*\n\s*name\s*=\s*"[^"]+"\s*\n\s*path\s*=\s*"([^"]+)"'
)


def autotests_false_crate_dirs() -> list[pathlib.Path]:
    """Crate directories whose Cargo.toml sets `autotests = false`.

    Crates without that key (e.g. `tsz-scanner`) get Cargo's default
    autodiscovery, so every direct `tests/*.rs` file is already its own
    target — this guard only concerns crates that opted out.
    """
    dirs = []
    for cargo_toml in sorted(CRATES_DIR.glob("*/Cargo.toml")):
        text = cargo_toml.read_text(encoding="utf-8")
        if re.search(r"(?m)^autotests\s*=\s*false\s*$", text):
            dirs.append(cargo_toml.parent)
    return dirs


def cargo_registered_relpaths(crate_dir: pathlib.Path) -> set[str]:
    """`tests/<file>.rs`-shaped paths named by `[[test]]` stanzas."""
    text = (crate_dir / "Cargo.toml").read_text(encoding="utf-8")
    return set(CARGO_TEST_STANZA_RE.findall(text))


def src_included_absolute_paths() -> set[pathlib.Path]:
    """Absolute paths reached by a `#[path] mod` from any crate's `src/`.

    Resolved per including file's own directory, not a hardcoded `src/`, so
    a future nested-module include still resolves correctly. Scanned across
    every crate (not just the one being checked): `tsz-core/src/lib.rs`
    reaches into `tsz-checker/tests/` this way, so reachability is a
    workspace-wide graph, not a per-crate one.
    """
    included = set()
    for rs_file in CRATES_DIR.glob("*/src/**/*.rs"):
        text = rs_file.read_text(encoding="utf-8")
        for raw_path in PATH_MOD_RE.findall(text):
            included.add((rs_file.parent / raw_path).resolve())
    return included


def file_has_tests(path: pathlib.Path, _visited: set | None = None) -> bool:
    """True if `path` (or a file it `#[path]`-includes, recursively) has a
    `#[test]` fn — covers the >2000-line split-file pattern where a root
    `tests/X.rs` declares zero tests itself and only `mod`s in
    `tests/X/part_NN.rs` siblings that hold the real fns."""
    if _visited is None:
        _visited = set()
    resolved = path.resolve()
    if resolved in _visited or not resolved.is_file():
        return False
    _visited.add(resolved)
    text = resolved.read_text(encoding="utf-8")
    if TEST_ATTR_RE.search(text):
        return True
    for raw_path in PATH_MOD_RE.findall(text):
        if file_has_tests(resolved.parent / raw_path, _visited):
            return True
    return False


def find_orphaned_test_files() -> list[str]:
    """`crate-name/tests/<file>.rs` entries with live `#[test]` fns that no
    `[[test]]` stanza or workspace `src/` `#[path]` include reaches."""
    orphans = []
    src_included = src_included_absolute_paths()
    for crate_dir in autotests_false_crate_dirs():
        tests_dir = crate_dir / "tests"
        if not tests_dir.is_dir():
            continue
        cargo_registered = {
            (crate_dir / relpath).resolve()
            for relpath in cargo_registered_relpaths(crate_dir)
        }
        for root_file in sorted(tests_dir.glob("*.rs")):
            if root_file.resolve() in cargo_registered | src_included:
                continue
            if file_has_tests(root_file):
                rel = root_file.relative_to(crate_dir).as_posix()
                orphans.append(f"{crate_dir.name}/{rel}")
    return orphans


def parse_baseline(text: str) -> set[str]:
    entries = set()
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        entries.add(line)
    return entries


def main() -> int:
    current = set(find_orphaned_test_files())
    baseline = parse_baseline(BASELINE_PATH.read_text(encoding="utf-8"))
    new_orphans = sorted(current - baseline)
    if new_orphans:
        print(
            "New orphaned test file(s) found — these have #[test] fns but no "
            "[[test]] Cargo.toml stanza and no src/ #[path] include, so they "
            "never run:",
            file=sys.stderr,
        )
        for entry in new_orphans:
            print(f"  {entry}", file=sys.stderr)
        print(
            f"\nRegister the file (add it to {BASELINE_PATH} "
            "only if this is a deliberate, already-known orphan; otherwise wire "
            "it into the crate's Cargo.toml or a src/ #[path] include, or delete "
            "it if it is genuinely dead).",
            file=sys.stderr,
        )
        return 1
    print(f"test-file-reachability: {len(current)} known orphan(s), 0 new.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
