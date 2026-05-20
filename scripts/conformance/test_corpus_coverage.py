import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CACHE_PATH = ROOT / "scripts/conformance/tsc-cache-full.json"
TEST_CASES_PATH = ROOT / "TypeScript/tests/cases"

SOURCE_SUFFIXES = {".ts", ".tsx", ".js", ".jsx", ".mts", ".cts"}
DECLARATION_SUFFIXES = (".d.ts", ".d.mts", ".d.cts")


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
        stripped = line.strip()
        if not stripped.startswith("//"):
            continue
        directive = stripped[2:].strip()
        if not directive.startswith("@") or ":" not in directive:
            continue
        key, value = directive[1:].split(":", 1)
        options[key.strip().lower()] = value.strip()

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
