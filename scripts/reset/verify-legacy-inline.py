#!/usr/bin/env python3
"""Verify the extracted inline-test corpus from the pre-reset compiler."""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_ROOT = ROOT / "tests" / "legacy-internal" / "inline"
MANIFEST_PATH = ARCHIVE_ROOT / "manifest.json"
MAX_PHYSICAL_LINES = 2_000
TEST_ATTRIBUTE = re.compile(
    r"(?m)^[ \t]*#\[(?:tokio::)?test(?:[ \t]*\([^\]\n]*\))?\][ \t]*$"
)
FUNCTION_NAME = re.compile(r"\b(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
BEGIN_MARKER = re.compile(
    r"^// TSZ_INLINE_TEST_BEGIN ([0-9a-f]{64}) ([0-9]+) ([A-Za-z_][A-Za-z0-9_]*)$"
)
END_MARKER = re.compile(r"^// TSZ_INLINE_TEST_END ([0-9a-f]{64})$")


def digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def run_git(args: list[str]) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or "git command failed")
    return completed.stdout


def git_show(commit: str, source_path: str) -> str:
    return run_git(["show", f"{commit}:{source_path}"])


def _blank(masked: list[str], start: int, end: int) -> None:
    for index in range(start, end):
        if masked[index] != "\n":
            masked[index] = " "


def mask_non_code(source: str) -> str:
    """Replace Rust comments and literals with spaces while preserving offsets."""

    masked = list(source)
    length = len(source)
    index = 0
    while index < length:
        if source.startswith("//", index):
            end = source.find("\n", index + 2)
            if end < 0:
                end = length
            _blank(masked, index, end)
            index = end
            continue

        if source.startswith("/*", index):
            depth = 1
            end = index + 2
            while end < length and depth:
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            _blank(masked, index, end)
            index = end
            continue

        raw = re.match(r"(?:br|r)(?P<hashes>#{0,255})\"", source[index:])
        if raw:
            hashes = raw.group("hashes")
            terminator = f'\"{hashes}'
            body_start = index + raw.end()
            close = source.find(terminator, body_start)
            end = length if close < 0 else close + len(terminator)
            _blank(masked, index, end)
            index = end
            continue

        string_start = index
        if source.startswith(('b"', 'c"'), index):
            quote = index + 1
        elif source[index] == '"':
            quote = index
        else:
            quote = -1
        if quote >= 0:
            end = quote + 1
            escaped = False
            while end < length:
                char = source[end]
                end += 1
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    break
            _blank(masked, string_start, end)
            index = end
            continue

        char_start = index
        if source.startswith("b'", index):
            quote = index + 1
        elif source[index] == "'":
            quote = index
        else:
            quote = -1
        if quote >= 0:
            if quote + 1 < length and source[quote + 1] == "\\":
                end = quote + 2
                escaped = False
                while end < length and source[end] != "\n":
                    char = source[end]
                    end += 1
                    if char == "'" and not escaped:
                        break
                    if escaped:
                        escaped = False
                    elif char == "\\":
                        escaped = True
                if end <= length and source[end - 1] == "'":
                    _blank(masked, char_start, end)
                    index = end
                    continue
            elif quote + 2 < length and source[quote + 2] == "'":
                end = quote + 3
                _blank(masked, char_start, end)
                index = end
                continue

        index += 1

    return "".join(masked)


def _line_start(source: str, offset: int) -> int:
    newline = source.rfind("\n", 0, offset)
    return 0 if newline < 0 else newline + 1


def _include_leading_annotations(source: str, start: int) -> int:
    candidate = start
    while candidate > 0:
        previous_end = candidate - 1
        previous_start = source.rfind("\n", 0, previous_end)
        previous_start = 0 if previous_start < 0 else previous_start + 1
        line = source[previous_start:previous_end].strip()
        if not line or not (line.startswith("#") or line.startswith("//")):
            break
        candidate = previous_start
    return candidate


def _matching_brace(masked: str, opening: int) -> int:
    depth = 0
    for index in range(opening, len(masked)):
        if masked[index] == "{":
            depth += 1
        elif masked[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    raise ValueError(f"unclosed test body at byte {opening}")


def extract_tests(source: str) -> list[dict[str, object]]:
    masked = mask_non_code(source)
    extracted: list[dict[str, object]] = []
    for attribute in TEST_ATTRIBUTE.finditer(masked):
        function = FUNCTION_NAME.search(masked, attribute.end())
        next_attribute = TEST_ATTRIBUTE.search(masked, attribute.end())
        if function is None or (
            next_attribute is not None and next_attribute.start() < function.start()
        ):
            raise ValueError(f"test attribute without a function at byte {attribute.start()}")
        opening = masked.find("{", function.end())
        if opening < 0:
            raise ValueError(f"test function without a body at byte {function.start()}")
        end = _matching_brace(masked, opening) + 1
        if end < len(source) and source[end] == "\n":
            end += 1
        start = _include_leading_annotations(source, _line_start(source, attribute.start()))
        snippet = source[start:end]
        extracted.append(
            {
                "name": function.group(1),
                "line": source.count("\n", 0, attribute.start()) + 1,
                "text": snippet,
                "sha256": digest(snippet),
            }
        )
    return extracted


def mapped_archive_path(source_path: str) -> Path:
    parts = Path(source_path).parts
    if len(parts) < 4 or parts[0] != "crates" or parts[2] != "src":
        raise ValueError(f"unexpected compiler path: {source_path}")
    return ARCHIVE_ROOT.parent / parts[1] / Path(*parts[3:])


def inline_archive_path(source_path: str) -> Path:
    parts = Path(source_path).parts
    return ARCHIVE_ROOT / parts[1] / Path(*parts[3:])


def discover_unmapped_sources(commit: str) -> list[str]:
    output = run_git(
        [
            "grep",
            "-l",
            "-E",
            r"#\[(tokio::)?test([ (]|\])",
            commit,
            "--",
            "crates/tsz-*/src/**/*.rs",
            "crates/tsz-*/src/*.rs",
        ]
    )
    sources = [line.removeprefix(f"{commit}:") for line in output.splitlines()]
    return sorted(path for path in sources if not mapped_archive_path(path).is_file())


def read_archives(source_commit: str) -> tuple[list[str], int]:
    hashes: list[str] = []
    file_count = 0
    for archive in sorted(ARCHIVE_ROOT.rglob("*.rs")):
        file_count += 1
        lines = archive.read_text(encoding="utf-8").splitlines(keepends=True)
        if len(lines) > MAX_PHYSICAL_LINES:
            raise ValueError(f"archive exceeds {MAX_PHYSICAL_LINES} lines: {archive}")
        relative = archive.relative_to(ARCHIVE_ROOT)
        expected_source = Path("crates") / relative.parts[0] / "src" / Path(
            *relative.parts[1:]
        )
        expected_header = [
            "//! Disabled inline tests extracted from the pre-reset compiler.\n",
            f"//! Source: `{expected_source.as_posix()}`\n",
            f"//! Commit: `{source_commit}`\n",
        ]
        if lines[:3] != expected_header:
            raise ValueError(f"invalid source metadata in {archive}")
        marker_count = 0
        index = 0
        while index < len(lines):
            begin = BEGIN_MARKER.match(lines[index].rstrip("\n"))
            if begin is None:
                index += 1
                continue
            marker_count += 1
            expected_hash = begin.group(1)
            index += 1
            body: list[str] = []
            while index < len(lines):
                end = END_MARKER.match(lines[index].rstrip("\n"))
                if end is not None:
                    break
                body.append(lines[index])
                index += 1
            if index >= len(lines):
                raise ValueError(f"missing end marker in {archive}")
            if end.group(1) != expected_hash:
                raise ValueError(f"marker hash mismatch in {archive}")
            actual_hash = digest("".join(body))
            if actual_hash != expected_hash:
                raise ValueError(f"body hash mismatch in {archive}")
            hashes.append(actual_hash)
            index += 1
        if marker_count == 0:
            raise ValueError(f"archive contains no test fragments: {archive}")
    return hashes, file_count


def verify(*, verify_source: bool) -> dict[str, int]:
    manifest_text = MANIFEST_PATH.read_text(encoding="utf-8")
    if len(manifest_text.splitlines()) > MAX_PHYSICAL_LINES:
        raise ValueError(f"manifest exceeds {MAX_PHYSICAL_LINES} lines")
    manifest = json.loads(manifest_text)
    archived_hashes, archive_files = read_archives(manifest["source_commit"])
    omitted_hashes = manifest["omitted_hashes"]
    source_hashes = manifest["source_hashes"]

    if len(archived_hashes) != manifest["archived_test_count"]:
        raise ValueError("archive test count does not match the manifest")
    if archive_files != manifest["archive_file_count"]:
        raise ValueError("archive file count does not match the manifest")
    if len(omitted_hashes) != manifest["omitted_test_count"]:
        raise ValueError("omitted test count does not match the manifest")
    if len(source_hashes) != manifest["source_test_count"]:
        raise ValueError("source test count does not match the manifest")
    if Counter(archived_hashes) + Counter(omitted_hashes) != Counter(source_hashes):
        raise ValueError("archived and omitted tests do not cover the source hash multiset")

    if verify_source:
        sources = discover_unmapped_sources(manifest["source_commit"])
        if len(sources) != manifest["source_file_count"]:
            raise ValueError("source file count does not match the manifest")
        live_source_hashes: list[str] = []
        for source_path in sources:
            tests = extract_tests(git_show(manifest["source_commit"], source_path))
            live_source_hashes.extend(test["sha256"] for test in tests)
        if Counter(live_source_hashes) != Counter(source_hashes):
            raise ValueError("source commit tests do not match the frozen manifest")

    return {
        "source_files": manifest["source_file_count"],
        "source_tests": len(source_hashes),
        "archive_files": archive_files,
        "archived_tests": len(archived_hashes),
        "omitted_tests": len(omitted_hashes),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--verify-source",
        action="store_true",
        help="also re-extract tests from the recorded git commit",
    )
    args = parser.parse_args()
    try:
        result = verify(verify_source=args.verify_source)
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"legacy inline-test verification failed: {error}", file=sys.stderr)
        return 1
    print(
        "legacy inline-test corpus: "
        f"{result['archived_tests']}/{result['source_tests']} tests archived in "
        f"{result['archive_files']} files; {result['omitted_tests']} retired-policy tests omitted"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
