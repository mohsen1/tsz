#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const DEFAULT_ROWS = ["fp-ts-project", "io-ts-project", "neverthrow-project"];

function usage() {
  console.error(`usage: node scripts/bench/deferred-hkt-split-gauge.mjs [options]

Options:
  --rows <a,b,c>          Project rows to measure (default: ${DEFAULT_ROWS.join(",")})
  --fixture-root <path>   project-compile fixture root (default: .target/project-compile-guard)
  --summary-json <path>   Output summary JSON path
  --from-existing         Read existing JSONL/perf artifacts instead of running the guard
  --timeout <seconds>     Row timeout passed to project-compile-guard.sh
`);
}

function parseArgs(argv) {
  const out = {
    rows: DEFAULT_ROWS,
    fixtureRoot: path.join(ROOT, ".target", "project-compile-guard"),
    summaryJson: null,
    fromExisting: false,
    timeout: null,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--rows") {
      out.rows = argv[++i]?.split(",").map((row) => row.trim()).filter(Boolean) ?? [];
    } else if (arg === "--fixture-root") {
      out.fixtureRoot = path.resolve(argv[++i] ?? "");
    } else if (arg === "--summary-json") {
      out.summaryJson = path.resolve(argv[++i] ?? "");
    } else if (arg === "--from-existing") {
      out.fromExisting = true;
    } else if (arg === "--timeout") {
      out.timeout = argv[++i] ?? null;
    } else if (arg === "-h" || arg === "--help") {
      usage();
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }

  if (!out.rows.length) throw new Error("--rows must name at least one row");
  out.summaryJson ??= path.join(out.fixtureRoot, "deferred-hkt-split-gauge-summary.json");
  return out;
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function readJsonl(file) {
  if (!fs.existsSync(file)) return [];
  return fs.readFileSync(file, "utf8")
    .split(/\r?\n/)
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line));
}

function getNumber(object, pathSegments) {
  let current = object;
  for (const segment of pathSegments) {
    current = current?.[segment];
  }
  return Number.isFinite(Number(current)) ? Number(current) : 0;
}

function rowPerfPath(fixtureRoot, row) {
  return path.join(fixtureRoot, "perf-counters", `${row}.perf.json`);
}

function diagnosticDeltaCount(row) {
  if (row?.diagnostic_status === "none") return 0;
  if (Array.isArray(row?.diagnostic_deltas)) return row.diagnostic_deltas.length;
  return 0;
}

export function summarizeRow(row, perf) {
  const identity = perf?.identity ?? {};
  const inferenceFallbackTypes = getNumber(identity, [
    "inference_source_placeholder_unknown_fallback_types",
  ]);
  const inferenceFallbackPlaceholders = getNumber(identity, [
    "inference_source_placeholder_unknown_fallback_placeholders",
  ]);
  const inferenceFallbackIndexAccessTypes = getNumber(identity, [
    "inference_source_placeholder_unknown_fallback_index_access_types",
  ]);
  const relationDeferredPairs = getNumber(identity, [
    "relation_deferred_index_access_pair_total",
  ]);
  const relationDeferredAccepted = getNumber(identity, [
    "relation_deferred_index_access_pair_accepted",
  ]);

  let split = "no-signal";
  if (inferenceFallbackTypes > 0 && relationDeferredPairs > 0) {
    split = "mixed";
  } else if (inferenceFallbackTypes > 0) {
    split = "inference-fallback";
  } else if (relationDeferredPairs > 0) {
    split = "relation-deferred";
  }

  return {
    name: row.name,
    state: row.state ?? null,
    exit_class: row.exit_class ?? null,
    diagnostic_status: row.diagnostic_status ?? null,
    diagnostic_delta_count: diagnosticDeltaCount(row),
    split,
    counters: {
      inference_source_placeholder_unknown_fallback_types: inferenceFallbackTypes,
      inference_source_placeholder_unknown_fallback_placeholders: inferenceFallbackPlaceholders,
      inference_source_placeholder_unknown_fallback_index_access_types: inferenceFallbackIndexAccessTypes,
      relation_deferred_index_access_pair_total: relationDeferredPairs,
      relation_deferred_index_access_pair_accepted: relationDeferredAccepted,
    },
  };
}

export function buildSummary({ rows, fixtureRoot, compatibilityJsonl }) {
  const compatibilityRows = new Map(readJsonl(compatibilityJsonl).map((row) => [row.name, row]));
  const measuredRows = rows.map((name) => {
    const row = compatibilityRows.get(name) ?? { name, state: "missing-artifact" };
    const perfPath = rowPerfPath(fixtureRoot, name);
    const perf = fs.existsSync(perfPath) ? readJson(perfPath) : null;
    return {
      ...summarizeRow(row, perf),
      perf_counters_json: fs.existsSync(perfPath) ? path.relative(ROOT, perfPath) : null,
    };
  });

  const totals = measuredRows.reduce((acc, row) => {
    acc.rows += 1;
    acc[row.split] = (acc[row.split] ?? 0) + 1;
    for (const [key, value] of Object.entries(row.counters)) {
      acc.counters[key] = (acc.counters[key] ?? 0) + value;
    }
    return acc;
  }, { rows: 0, counters: {} });

  return {
    schema_version: 1,
    generated_at: new Date().toISOString(),
    rows: measuredRows,
    totals,
  };
}

function renderMarkdown(summary) {
  const lines = [
    "### Deferred HKT split gauge",
    "",
    "| Row | State | Diagnostics | Split | Inference fallback | Indexed fallback | Relation pairs | Relation accepted |",
    "| --- | --- | ---: | --- | ---: | ---: | ---: | ---: |",
  ];
  for (const row of summary.rows) {
    lines.push([
      `| ${row.name}`,
      row.state ?? "-",
      String(row.diagnostic_delta_count),
      row.split,
      String(row.counters.inference_source_placeholder_unknown_fallback_types),
      String(row.counters.inference_source_placeholder_unknown_fallback_index_access_types),
      String(row.counters.relation_deferred_index_access_pair_total),
      `${row.counters.relation_deferred_index_access_pair_accepted} |`,
    ].join(" | "));
  }
  return lines.join("\n");
}

function runGuard(options, compatibilityJsonl, compatibilitySummary) {
  const filter = `^(${options.rows.map(escapeRegex).join("|")})$`;
  const env = {
    ...process.env,
    TSZ_PROJECT_COMPILE_SET: "canary",
    TSZ_PROJECT_COMPILE_FILTER: filter,
    TSZ_PROJECT_COMPILE_ALLOW_FAILURES: "1",
    TSZ_PROJECT_COMPILE_PERF_COUNTERS: "1",
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: options.fixtureRoot,
    TSZ_PROJECT_COMPILE_COMPATIBILITY_JSONL: compatibilityJsonl,
    TSZ_PROJECT_COMPILE_COMPATIBILITY_SUMMARY: compatibilitySummary,
    TSZ_DETERMINISTIC_STORE_ELECTION: process.env.TSZ_DETERMINISTIC_STORE_ELECTION ?? "1",
    TSZ_PROJECT_COMPILE_RESULT_CACHE_DIR: path.join(
      options.fixtureRoot,
      `.result-cache-deferred-hkt-split-gauge-${process.pid}`,
    ),
  };
  if (options.timeout) env.TSZ_PROJECT_COMPILE_TIMEOUT = options.timeout;

  const result = spawnSync("scripts/ci/project-compile-guard.sh", {
    cwd: ROOT,
    stdio: "inherit",
    env,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`project-compile-guard.sh exited ${result.status}`);
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  fs.mkdirSync(options.fixtureRoot, { recursive: true });
  const compatibilityJsonl = path.join(options.fixtureRoot, "deferred-hkt-split-gauge.jsonl");
  const compatibilitySummary = path.join(
    options.fixtureRoot,
    "deferred-hkt-split-gauge.project-summary.json",
  );

  if (!options.fromExisting) {
    runGuard(options, compatibilityJsonl, compatibilitySummary);
  }

  const summary = buildSummary({
    rows: options.rows,
    fixtureRoot: options.fixtureRoot,
    compatibilityJsonl,
  });
  fs.mkdirSync(path.dirname(options.summaryJson), { recursive: true });
  fs.writeFileSync(options.summaryJson, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  console.log(renderMarkdown(summary));
  console.log(`\nsummary: ${path.relative(ROOT, options.summaryJson)}`);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    main();
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }
}
