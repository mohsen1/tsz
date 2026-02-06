#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

function fail(message) {
  console.error(message);
  process.exit(1);
}

function parseArgs(argv) {
  const out = {
    bin: "",
    baseline: "",
    thresholdPct: 12,
    runs: 4,
    warmup: 1,
    updateBaseline: false,
    profile: "dev",
    cases: [],
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    switch (arg) {
      case "--bin":
        out.bin = argv[++i] ?? "";
        break;
      case "--baseline":
        out.baseline = argv[++i] ?? "";
        break;
      case "--threshold-pct":
        out.thresholdPct = Number(argv[++i] ?? "12");
        break;
      case "--runs":
        out.runs = Number(argv[++i] ?? "4");
        break;
      case "--warmup":
        out.warmup = Number(argv[++i] ?? "1");
        break;
      case "--update-baseline":
        out.updateBaseline = (argv[++i] ?? "0") === "1";
        break;
      case "--profile":
        out.profile = argv[++i] ?? "dev";
        break;
      case "--case": {
        const raw = argv[++i] ?? "";
        const splitAt = raw.indexOf("=");
        if (splitAt <= 0 || splitAt === raw.length - 1) {
          fail(`Invalid --case format: ${raw}`);
        }
        const name = raw.slice(0, splitAt);
        const file = raw.slice(splitAt + 1);
        out.cases.push({ name, file });
        break;
      }
      default:
        fail(`Unknown argument: ${arg}`);
    }
  }

  if (!out.bin) fail("Missing required argument: --bin");
  if (!out.baseline) fail("Missing required argument: --baseline");
  if (!Number.isFinite(out.thresholdPct) || out.thresholdPct < 0) {
    fail(`Invalid threshold percent: ${out.thresholdPct}`);
  }
  if (!Number.isInteger(out.runs) || out.runs <= 0) {
    fail(`Invalid run count: ${out.runs}`);
  }
  if (!Number.isInteger(out.warmup) || out.warmup < 0) {
    fail(`Invalid warmup count: ${out.warmup}`);
  }
  if (out.cases.length === 0) fail("At least one --case is required");
  return out;
}

function runOnce(bin, file) {
  const start = process.hrtime.bigint();
  const result = spawnSync(bin, ["--noEmit", file], { stdio: "ignore" });
  const end = process.hrtime.bigint();

  if (result.error) {
    fail(`Failed to execute ${bin}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`Benchmark command failed for ${path.basename(file)} (exit ${result.status})`);
  }
  return Number(end - start) / 1e6;
}

function mean(values) {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 0) {
    return (sorted[mid - 1] + sorted[mid]) / 2;
  }
  return sorted[mid];
}

function readBaseline(filePath) {
  if (!fs.existsSync(filePath)) {
    return null;
  }
  try {
    const raw = fs.readFileSync(filePath, "utf8");
    return JSON.parse(raw);
  } catch (error) {
    fail(`Failed to read baseline ${filePath}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function writeBaseline(filePath, baseline) {
  const dir = path.dirname(filePath);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(baseline, null, 2)}\n`, "utf8");
}

function formatMs(value) {
  return `${value.toFixed(2)}ms`;
}

function main() {
  const opts = parseArgs(process.argv.slice(2));

  const measurements = {};
  console.log("   Running microbenchmarks...");
  for (const testCase of opts.cases) {
    if (!fs.existsSync(testCase.file)) {
      fail(`Benchmark file not found: ${testCase.file}`);
    }

    for (let i = 0; i < opts.warmup; i += 1) {
      runOnce(opts.bin, testCase.file);
    }

    const samples = [];
    for (let i = 0; i < opts.runs; i += 1) {
      samples.push(runOnce(opts.bin, testCase.file));
    }

    const entry = {
      file: testCase.file,
      mean_ms: mean(samples),
      median_ms: median(samples),
      samples_ms: samples,
    };
    measurements[testCase.name] = entry;
    console.log(
      `   - ${testCase.name}: mean ${formatMs(entry.mean_ms)} | median ${formatMs(entry.median_ms)}`
    );
  }

  const baseline = readBaseline(opts.baseline);
  const nextBaseline = {
    schema_version: 1,
    profile: opts.profile,
    runs: opts.runs,
    warmup: opts.warmup,
    updated_at: new Date().toISOString(),
    cases: Object.fromEntries(
      Object.entries(measurements).map(([name, data]) => [name, { mean_ms: data.mean_ms }])
    ),
  };

  if (!baseline || baseline.schema_version !== 1 || baseline.profile !== opts.profile) {
    writeBaseline(opts.baseline, nextBaseline);
    console.log(`   Baseline initialized at ${opts.baseline}`);
    return;
  }

  const regressions = [];
  for (const [name, current] of Object.entries(measurements)) {
    const previous = baseline.cases?.[name];
    if (!previous || typeof previous.mean_ms !== "number") {
      regressions.push({
        name,
        reason: "missing_baseline",
      });
      continue;
    }

    const deltaPct = ((current.mean_ms - previous.mean_ms) / previous.mean_ms) * 100;
    if (deltaPct > opts.thresholdPct) {
      regressions.push({
        name,
        reason: "regression",
        baselineMs: previous.mean_ms,
        currentMs: current.mean_ms,
        deltaPct,
      });
    }
  }

  if (regressions.length > 0) {
    console.error("");
    console.error("❌ Microbenchmark regression detected.");
    for (const item of regressions) {
      if (item.reason === "missing_baseline") {
        console.error(`   - ${item.name}: missing baseline entry`);
      } else {
        console.error(
          `   - ${item.name}: ${formatMs(item.baselineMs)} -> ${formatMs(item.currentMs)} (+${item.deltaPct.toFixed(2)}%)`
        );
      }
    }
    console.error("");
    console.error(
      `   Allowed per-case regression: ${opts.thresholdPct.toFixed(2)}%`
    );
    console.error(
      "   If this change is expected, refresh baseline with TSZ_BENCH_UPDATE_BASELINE=1."
    );
    process.exit(1);
  }

  if (opts.updateBaseline) {
    writeBaseline(opts.baseline, nextBaseline);
    console.log(`   Baseline updated at ${opts.baseline}`);
  } else {
    console.log(
      `   Regression gate passed (threshold: ${opts.thresholdPct.toFixed(2)}%).`
    );
  }
}

main();
