#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [, , outFile, ...inputFiles] = process.argv;

if (!outFile || inputFiles.length === 0) {
  console.error("Usage: scripts/bench/merge-results.mjs <out-file> <input-json...>");
  process.exit(2);
}

const payloads = inputFiles
  .filter((file) => fs.existsSync(file))
  .map((file) => {
    const payload = JSON.parse(fs.readFileSync(file, "utf8"));
    return { file, payload };
  });

if (payloads.length === 0) {
  console.error("No benchmark JSON inputs found.");
  process.exit(1);
}

const results = payloads.flatMap(({ payload }) => payload.results || []);
const tszWins = results.filter((row) => row.winner === "tsz").length;
const tsgoWins = results.filter((row) => row.winner === "tsgo").length;
const errorCases = results.filter((row) => row.status).length;

const merged = {
  generated_at: new Date().toISOString(),
  benchmark_runner: "scripts/bench/bench-vs-tsgo.sh",
  merged_from: payloads.map(({ file }) => path.basename(file)).sort(),
  quick_mode: payloads.every(({ payload }) => payload.quick_mode === true),
  filter: null,
  binaries: payloads.find(({ payload }) => payload.binaries)?.payload.binaries || {},
  totals: {
    benchmarks_run: payloads.reduce(
      (sum, { payload }) => sum + Number(payload.totals?.benchmarks_run || 0),
      0,
    ),
    rows: results.length,
    tsz_wins: tszWins,
    tsgo_wins: tsgoWins,
    error_cases: errorCases,
  },
  results,
};

fs.mkdirSync(path.dirname(outFile), { recursive: true });
fs.writeFileSync(outFile, `${JSON.stringify(merged, null, 2)}\n`, "utf8");
console.log(`Merged ${payloads.length} benchmark files into ${outFile}`);
