"""Contract tests for emit metrics publication."""

import json
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
GCP_FULL_CI = ROOT / "scripts" / "ci" / "gcp-full-ci.sh"


class EmitMetricPublicationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.script = GCP_FULL_CI.read_text(encoding="utf-8")

    def function_body(self, name, end_marker):
        start = self.script.index(f"{name}() {{")
        end = self.script.index(end_marker, start)
        return self.script[start:end]

    def test_write_emit_metric_schema(self):
        helper = self.function_body("write_emit_metric", "\nsuite_needs_group() {")
        with tempfile.TemporaryDirectory() as temp_dir:
            temp = pathlib.Path(temp_dir)
            out = temp / "emit.json"
            runner = temp / "run.sh"
            runner.write_text(
                f"""#!/usr/bin/env bash
set -Eeuo pipefail
{helper}
write_emit_metric "{out}" 13401 13530 1 0 1619 1669 11862
""",
                encoding="utf-8",
            )
            subprocess.run(["bash", str(runner)], check=True, cwd=temp)
            data = json.loads(out.read_text(encoding="utf-8"))

        self.assertEqual(data["suite"], "emit")
        self.assertEqual(data["js_pass_rate"], "99.0")
        self.assertEqual(data["js_passed"], 13401)
        self.assertEqual(data["js_total"], 13530)
        self.assertEqual(data["js_skipped"], 1)
        self.assertEqual(data["js_timeouts"], 0)
        self.assertEqual(data["dts_pass_rate"], "97.0")
        self.assertEqual(data["dts_passed"], 1619)
        self.assertEqual(data["dts_total"], 1669)
        self.assertEqual(data["dts_skipped"], 11862)

    def test_single_shard_emit_publishes_latest_metric(self):
        body = self.function_body("run_emit_shard", "\nrun_emit_aggregate() {")
        validate_idx = body.index(
            'validate_emit_aggregate_counts "$js_p" "$js_t" "$js_s" "$js_to" "$dts_p" "$dts_t" "$dts_s" 1 1',
        )
        write_idx = body.index('write_emit_metric "$METRICS_DIR/emit.json"', validate_idx)
        publish_idx = body.index('publish_latest_metric emit "$METRICS_DIR/emit.json"', write_idx)
        self.assertLess(validate_idx, write_idx)
        self.assertLess(write_idx, publish_idx)

    def test_multi_shard_emit_aggregate_publishes_latest_metric(self):
        body = self.function_body("run_emit_aggregate", "\nrun_fourslash_shard() {")
        validate_idx = body.index(
            'validate_emit_aggregate_counts "$js_passed" "$js_total" "$js_skipped" "$js_timeouts"',
        )
        write_idx = body.index('write_emit_metric "$METRICS_DIR/emit.json"', validate_idx)
        publish_idx = body.index('publish_latest_metric emit "$METRICS_DIR/emit.json"', write_idx)
        self.assertLess(validate_idx, write_idx)
        self.assertLess(write_idx, publish_idx)


if __name__ == "__main__":
    unittest.main()
