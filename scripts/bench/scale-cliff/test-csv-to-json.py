#!/usr/bin/env python3
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("csv-to-json.py")


def load_module():
    spec = importlib.util.spec_from_file_location("scale_cliff_csv_to_json", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class ScaleCliffCsvToJsonTests(unittest.TestCase):
    def setUp(self):
        self.module = load_module()

    def test_builds_typed_rows_and_max_ratio_summary(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            csv_path = root / "cliff.csv"
            csv_path.write_text(
                "\n".join(
                    [
                        "fixture,files,total_s,check_s,parse_bind_s,io_read_s,memory_kb,"
                        "delegate_calls,delegate_misses,delegate_lib_hits,delegate_xfile_hits,"
                        "checker_state_new,checker_with_parent_cache,"
                        "overlay_copy_calls,overlay_entries_copied,"
                        "compute_type_of_symbol_calls,resolver_lookup_calls,resolver_pj_reads,"
                        "ratio_checkers_per_file,ratio_overlay_per_file,"
                        "ratio_delegations_per_file,ratio_compute_per_file,status,exit_code",
                        "monorepo-001,10,1.20,0.90,0.10,0.02,2048,"
                        "30,4,10,16,12,8,2,100,50,3,1,0.80,10,3.00,5.00,ok,0",
                        "monorepo-002,20,2.50,2.10,0.20,0.03,4096,"
                        "90,9,30,51,22,30,4,900,75,6,2,1.50,45,4.50,3.75,failed,101",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            rows = self.module.load_rows(csv_path)
            payload = self.module.build_payload(
                rows,
                csv_path=csv_path,
                generated_at="2026-05-18T00:00:00+00:00",
                tsz_bin="/tmp/tsz",
            )

        self.assertEqual(payload["schema_version"], 1)
        self.assertEqual(payload["fixtures"], 2)
        self.assertEqual(payload["tsz_bin"], "/tmp/tsz")
        self.assertEqual(payload["rows"][0]["files"], 10)
        self.assertEqual(payload["rows"][1]["status"], "failed")
        self.assertEqual(payload["rows"][1]["exit_code"], 101)
        self.assertEqual(payload["rows"][0]["total_s"], 1.2)
        self.assertEqual(
            payload["max_ratios"]["ratio_checkers_per_file"],
            {"fixture": "monorepo-002", "value": 1.5},
        )
        self.assertEqual(
            payload["max_ratios"]["ratio_overlay_per_file"],
            {"fixture": "monorepo-002", "value": 45.0},
        )

    def test_cli_writes_json_file(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            csv_path = root / "cliff.csv"
            json_path = root / "out" / "cliff.json"
            csv_path.write_text(
                "fixture,files,total_s,check_s,parse_bind_s,io_read_s,memory_kb,"
                "delegate_calls,delegate_misses,delegate_lib_hits,delegate_xfile_hits,"
                "checker_state_new,checker_with_parent_cache,"
                "overlay_copy_calls,overlay_entries_copied,"
                "compute_type_of_symbol_calls,resolver_lookup_calls,resolver_pj_reads,"
                "ratio_checkers_per_file,ratio_overlay_per_file,"
                "ratio_delegations_per_file,ratio_compute_per_file,status,exit_code\n",
                encoding="utf-8",
            )

            rc = self.module.main(
                [
                    str(csv_path),
                    "--json-file",
                    str(json_path),
                    "--generated-at",
                    "2026-05-18T00:00:00+00:00",
                ]
            )

            self.assertEqual(rc, 0)
            payload = json.loads(json_path.read_text(encoding="utf-8"))
            self.assertEqual(payload["fixtures"], 0)
            self.assertEqual(
                payload["max_ratios"],
                {
                    "ratio_checkers_per_file": None,
                    "ratio_overlay_per_file": None,
                    "ratio_delegations_per_file": None,
                    "ratio_compute_per_file": None,
                },
            )


if __name__ == "__main__":
    unittest.main()
