import json
import re
import tempfile
import unittest
from pathlib import Path

from lib.cache_domain import (
    load_json_object,
    resolve_pinned_typescript_version,
    validate_cache_domain,
)


ROOT = Path(__file__).resolve().parents[2]
CACHE_PATH = ROOT / "scripts/conformance/tsc-cache-full.json"
DOMAIN_PATH = ROOT / "scripts/conformance/conformance-domain.json"
VERSIONS_PATH = ROOT / "scripts/conformance/typescript-versions.json"
TEST_CASES_PATH = ROOT / "TypeScript/tests/cases"
DIRECTIVE_SPEC_VECTORS_PATH = ROOT / "scripts/test-directives/spec-vectors.json"

SOURCE_SUFFIXES = {".ts", ".tsx", ".js", ".jsx", ".mts", ".cts"}
DECLARATION_SUFFIXES = (".d.ts", ".d.mts", ".d.cts")

# Canonical test-directive grammar (issue #13127); must agree with
# crates/conformance/src/test_directives.rs and scripts/emit/src/directives.ts.
# Locked to scripts/test-directives/spec-vectors.json by the tests below.
_DIRECTIVE_LINE_RE = re.compile(r"^\s*//\s*@([A-Za-z0-9_]+)\s*:([^\r\n]*)$")
_FLAG_LINE_RE = re.compile(r"^\s*//\s*@([A-Za-z0-9_-]+)\s*$")


def parse_directive_line(line: str):
    """Recognize a `// @key: value` directive; returns (key_lower, value)."""
    match = _DIRECTIVE_LINE_RE.match(line.rstrip("\r"))
    if not match:
        return None
    return match.group(1).lower(), match.group(2).strip()


def parse_flag_directive_line(line: str):
    """Recognize a `// @name` flag directive; returns the name as written."""
    match = _FLAG_LINE_RE.match(line.rstrip("\r"))
    return match.group(1) if match else None


def skipped_conformance_cache_reason(path: str):
    normalized = path.replace("\\", "/")
    if Path(normalized).name.startswith("._"):
        return "appledouble"
    if "/fourslash/" in f"/{normalized}":
        return "fourslash"
    if "APISample" in normalized or "APILibCheck" in normalized:
        return "api-sample"
    if normalized.endswith((".d.ts", ".d.mts", ".d.cts")):
        return "declaration"
    return None


def discover_candidate_keys(test_cases_path=TEST_CASES_PATH):
    """Mirror Rust discovery before runnable/unsupported/skipped partitioning."""

    keys = []
    for path in test_cases_path.rglob("*"):
        if not path.is_file():
            continue

        relative = path.relative_to(test_cases_path).as_posix()
        if path.suffix not in SOURCE_SUFFIXES:
            continue
        if relative.endswith(DECLARATION_SUFFIXES):
            continue
        if skipped_conformance_cache_reason(relative) is not None:
            continue

        keys.append(relative)

    return sorted(keys)


class DirectiveSpecVectorTests(unittest.TestCase):
    """Bind this module's directive recognizer to the shared grammar vectors."""

    @classmethod
    def setUpClass(cls):
        cls.vectors = json.loads(
            DIRECTIVE_SPEC_VECTORS_PATH.read_text(encoding="utf-8")
        )

    def test_directive_lines_match_spec_vectors(self):
        for case in self.vectors["directive_lines"]:
            parsed = parse_directive_line(case["line"])
            if case["key"] is None:
                self.assertIsNone(parsed, f"line: {case['line']!r}")
            else:
                self.assertEqual(
                    (case["key"], case["value"]), parsed, f"line: {case['line']!r}"
                )

    def test_flag_lines_match_spec_vectors(self):
        for case in self.vectors["flag_lines"]:
            self.assertEqual(
                case["name"],
                parse_flag_directive_line(case["line"]),
                f"line: {case['line']!r}",
            )


class CorpusDiscoveryTests(unittest.TestCase):
    def test_skip_directives_remain_in_candidate_domain(self):
        with tempfile.TemporaryDirectory() as temp:
            cases = Path(temp)
            compiler = cases / "compiler"
            compiler.mkdir()
            compiler.joinpath("ordinary.ts").write_text("let x = 1;\n")
            compiler.joinpath("skipped.ts").write_text("// @skip: true\n")
            compiler.joinpath("lib.d.ts").write_text("declare const x: number;\n")
            compiler.joinpath("APISample_case.ts").write_text("let x = 1;\n")

            self.assertEqual(
                ["compiler/ordinary.ts", "compiler/skipped.ts"],
                discover_candidate_keys(cases),
            )


class ConformanceCorpusCoverageTests(unittest.TestCase):
    def test_appledouble_files_are_not_runnable_corpus_entries(self):
        self.assertEqual(
            "appledouble",
            skipped_conformance_cache_reason("compiler/._stray.ts"),
        )

    def test_checked_in_tsc_cache_has_no_known_unrunnable_entries(self):
        cache = json.loads(CACHE_PATH.read_text(encoding="utf-8"))
        unrunnable = sorted(
            (path, skipped_conformance_cache_reason(path))
            for path in cache
            if skipped_conformance_cache_reason(path) is not None
        )

        self.assertEqual([], unrunnable)

    def test_checked_in_cache_domain_partition_is_valid(self):
        cache = load_json_object(CACHE_PATH, "TSC cache")
        domain = load_json_object(DOMAIN_PATH, "conformance domain")
        versions = load_json_object(VERSIONS_PATH, "TypeScript version manifest")
        pinned_version = resolve_pinned_typescript_version(versions)

        summary = validate_cache_domain(cache, domain, pinned_version)

        self.assertEqual(summary.runnable, len(cache))
        self.assertEqual(summary.unsupported, len(domain["unsupported"]))
        self.assertEqual(summary.skipped, len(domain["skipped"]))

    def test_checked_in_partition_matches_discovered_corpus(self):
        if not TEST_CASES_PATH.exists():
            self.skipTest("TypeScript test corpus is not checked out")

        cache = load_json_object(CACHE_PATH, "TSC cache")
        domain = load_json_object(DOMAIN_PATH, "conformance domain")
        expected = discover_candidate_keys()
        actual = sorted(
            set(cache) | set(domain["unsupported"]) | set(domain["skipped"])
        )

        self.assertEqual(expected, actual)


if __name__ == "__main__":
    unittest.main()
