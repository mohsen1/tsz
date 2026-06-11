import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CACHE_PATH = ROOT / "scripts/conformance/tsc-cache-full.json"
TEST_CASES_PATH = ROOT / "TypeScript/tests/cases"
DIRECTIVE_SPEC_VECTORS_PATH = ROOT / "scripts/test-directives/spec-vectors.json"

SOURCE_SUFFIXES = {".ts", ".tsx", ".js", ".jsx", ".mts", ".cts"}
DECLARATION_SUFFIXES = (".d.ts", ".d.mts", ".d.cts")

# Canonical test-directive grammar (issue #13127); must agree with
# crates/tsz-common/src/test_directives.rs and scripts/emit/src/directives.ts.
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


def has_skip_directive(path: Path) -> bool:
    options = {}
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        directive = parse_directive_line(line)
        if directive is not None:
            key, value = directive
            options[key] = value

    return "skip" in options or options.get("nocheck") == "true"


def discover_expected_cache_keys():
    keys = []
    for path in TEST_CASES_PATH.rglob("*"):
        if not path.is_file():
            continue

        relative = path.relative_to(TEST_CASES_PATH).as_posix()
        if path.suffix not in SOURCE_SUFFIXES:
            continue
        if relative.endswith(DECLARATION_SUFFIXES):
            continue
        if skipped_conformance_cache_reason(relative) is not None:
            continue
        if has_skip_directive(path):
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

    def test_checked_in_tsc_cache_matches_discovered_corpus(self):
        if not TEST_CASES_PATH.exists():
            self.skipTest("TypeScript test corpus is not checked out")

        cache = json.loads(CACHE_PATH.read_text(encoding="utf-8"))
        expected = discover_expected_cache_keys()
        actual = sorted(cache)

        self.assertEqual(expected, actual)


if __name__ == "__main__":
    unittest.main()
