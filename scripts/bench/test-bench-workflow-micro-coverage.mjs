#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

import { PERF_TIMED_CANARY_PROJECT_ROWS } from "./project-rows.mjs";

const workflow = fs.readFileSync(".github/workflows/bench.yml", "utf8");
const runner = fs.readFileSync("scripts/bench/bench-vs-tsgo.sh", "utf8");
const websiteData = fs.readFileSync(
  "crates/tsz-website/src/_data/benchmark_data.js",
  "utf8",
);

function workflowShardFilters() {
  return [...workflow.matchAll(/^\s+- label: ([^\n]+)\n\s+timeout: \d+\n\s+filter: '([^']+)'/gm)]
    .map((match) => ({
      label: match[1].trim(),
      pattern: match[2],
      regex: new RegExp(match[2]),
    }));
}

function expandNameTemplate(template, frames) {
  if (template.includes("$(") || template.includes("${rel") || template.includes("$label")) {
    return [];
  }

  const envs = frames.reduce((acc, frame) => (
    acc.flatMap((env) => frame.values.map((value) => ({ ...env, [frame.name]: value })))
  ), [{}]);

  return envs.flatMap((env) => {
    let name = template;
    for (const [key, value] of Object.entries(env)) {
      name = name
        .replaceAll(`\${${key}}`, value)
        .replaceAll(`$${key}`, value);
    }
    return name.includes("$") ? [] : [name];
  });
}

function parseBenchmarkNames() {
  const names = new Set();
  const frames = [];

  for (const line of runner.split(/\r?\n/)) {
    const loop = line.match(/^\s*for\s+([a-zA-Z_][a-zA-Z0-9_]*)\s+in\s+([0-9 ]+);\s+do\s*$/);
    if (loop) {
      frames.push({
        name: loop[1],
        values: loop[2].trim().split(/\s+/),
      });
      continue;
    }

    if (/^\s*done\s*$/.test(line) && frames.length > 0) {
      frames.pop();
      continue;
    }

    const benchmark = line.match(/\brun_benchmark\s+"([^"]*)"/);
    if (!benchmark) continue;
    for (const name of expandNameTemplate(benchmark[1], frames)) {
      names.add(name);
    }
  }

  return [...names].sort();
}

const shardFilters = workflowShardFilters();

assert.deepEqual(
  shardFilters.map((filter) => filter.label),
  [
    "compiler-files",
    "synthetic",
    "project-hotspots",
    "solver-stress",
    "algorithmic-bct",
    "algorithmic-constraint",
    "algorithmic-mapped",
    "projects",
    "bench-canaries",
    "large-ts-repo",
  ],
  "bench workflow shard labels should stay explicit when adding timed benchmark families",
);

const benchmarkNames = parseBenchmarkNames();
const missing = benchmarkNames.filter((name) => (
  !shardFilters.some((filter) => filter.regex.test(name))
));

assert.deepEqual(
  missing,
  [],
  "bench workflow shard filters should cover every statically named bench-vs-tsgo benchmark",
);

const projectHotspotFilter = shardFilters.find((filter) => filter.label === "project-hotspots");
assert.ok(projectHotspotFilter, "bench workflow should have a dedicated project-hotspots shard");

const projectHotspotRows = benchmarkNames.filter((name) => projectHotspotFilter.regex.test(name));
assert.deepEqual(
  projectHotspotRows,
  [
    "Conditional infer hotspot N=100",
    "Conditional infer hotspot N=200",
    "Conditional infer hotspot N=25",
    "Conditional infer hotspot N=50",
    "Contextual callback hotspot N=100",
    "Contextual callback hotspot N=200",
    "Contextual callback hotspot N=25",
    "Contextual callback hotspot N=50",
    "Indexed access hotspot N=100",
    "Indexed access hotspot N=200",
    "Indexed access hotspot N=25",
    "Indexed access hotspot N=50",
    "Object spread hotspot N=100",
    "Object spread hotspot N=200",
    "Object spread hotspot N=25",
    "Object spread hotspot N=50",
    "Recursive utility aliases N=120",
    "Recursive utility aliases N=240",
    "Recursive utility aliases N=30",
    "Remapped accessor hotspot N=100",
    "Remapped accessor hotspot N=200",
    "Remapped accessor hotspot N=25",
    "Remapped accessor hotspot N=50",
  ],
  "project-hotspots shard should publish all current project-derived micro rows",
);

assert.match(
  websiteData,
  /Project Hotspot Microbenchmarks/,
  "website benchmark data should classify project-derived micro rows separately",
);
assert.match(
  websiteData,
  /Recursive utility aliases\|Indexed access hotspot\|Remapped accessor hotspot\|Conditional infer hotspot\|Object spread hotspot\|Contextual callback hotspot/,
  "website benchmark category logic should recognize every project-hotspots shard row family",
);

// The dedicated passing-canary perf shard must time exactly the rows opted in
// via PERF_TIMED_CANARY_PROJECT_ROWS, so the single source of truth in
// project-rows.mjs cannot drift away from the bench workflow filter.
const benchCanariesFilter = shardFilters.find((filter) => filter.label === "bench-canaries");
assert.ok(benchCanariesFilter, "bench workflow should have a dedicated bench-canaries shard");
assert.deepEqual(
  [...benchCanariesFilter.pattern.split("|")].sort(),
  [...PERF_TIMED_CANARY_PROJECT_ROWS].sort(),
  "bench-canaries shard filter must match PERF_TIMED_CANARY_PROJECT_ROWS exactly",
);

console.log("bench workflow micro coverage tests passed");
