import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CACHE_PATH = ROOT / "scripts/conformance/tsc-cache-full.json"


def skipped_conformance_cache_reason(path: str):
    normalized = path.replace("\\", "/")
    if "/fourslash/" in f"/{normalized}":
        return "fourslash"
    if "APISample" in normalized or "APILibCheck" in normalized:
        return "api-sample"
    if normalized.endswith((".d.ts", ".d.mts", ".d.cts")):
        return "declaration"
    return None


class ConformanceCorpusCoverageTests(unittest.TestCase):
    def test_checked_in_tsc_cache_has_no_known_unrunnable_entries(self):
        cache = json.loads(CACHE_PATH.read_text(encoding="utf-8"))
        unrunnable = sorted(
            (path, skipped_conformance_cache_reason(path))
            for path in cache
            if skipped_conformance_cache_reason(path) is not None
        )

        self.assertEqual([], unrunnable)


if __name__ == "__main__":
    unittest.main()
