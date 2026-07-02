#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";

import { PROJECT_ROWS_BY_NAME } from "./project-rows.mjs";

const workflow = fs.readFileSync(".github/workflows/bench.yml", "utf8");
const runner = fs.readFileSync("scripts/bench/bench-vs-tsgo.sh", "utf8");
const websiteData = fs.readFileSync(
  "crates/tsz-website/src/_data/benchmark_data.js",
  "utf8",
);

// Project-derived hotspot micro families. The runner emits several N-scaled
// rows per family; the website classifies each family under a dedicated
// "Project Hotspot Microbenchmarks" category. This list is the single shared
// contract: the runner rows (via the golden list below) and the website
// classifier (via exact set equality below) are both pinned to it, so a family
// cannot be added to or removed from one side without the others following.
const PROJECT_HOTSPOT_FAMILIES = [
  "Recursive utility aliases",
  "Indexed access hotspot",
  "Remapped accessor hotspot",
  "Conditional infer hotspot",
  "Object spread hotspot",
  "Contextual callback hotspot",
];

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

// Parse the alternation rows out of the workflow's single manual `filter`
// input default, e.g. ^(Row A|Row B|project-c)$ -> ["Row A", "Row B", "project-c"].
function workflowDefaultFilterRows() {
  const match = workflow.match(/^\s+filter:\s*\n(?:\s+.*\n)*?\s+default:\s*"([^"]*)"/m);
  assert.ok(
    match,
    "bench workflow should expose a single manual `filter` input with a default row set",
  );
  const shape = match[1].match(/^\^\((.+)\)\$$/);
  assert.ok(
    shape,
    "bench workflow `filter` default must be an anchored ^(...)$ alternation so every named row is validated",
  );
  return shape[1].split("|").map((row) => row.trim()).filter(Boolean);
}

// The website `categoryFor` classifier is the source of truth for which
// benchmark families are "Project Hotspot Microbenchmarks". Parse its regex
// alternation so the test compares the shared contract against the real
// classifier instead of a hand-copied duplicate of it.
function websiteHotspotFamilies() {
  const match = websiteData.match(
    /\/([^/\n]+)\/i\.test\(name\)\)\s*\{\s*\n\s*return "Project Hotspot Microbenchmarks";/,
  );
  assert.ok(
    match,
    "website categoryFor should classify project hotspot micro rows via a family regex",
  );
  return match[1].split("|").map((family) => family.trim());
}

const benchmarkNames = parseBenchmarkNames();

// The bench workflow was intentionally collapsed to a single GCP-free manual
// run (see #15343), so there is no longer a per-family shard matrix to audit.
// The coverage that still matters is that the runner, the website category
// logic, and the workflow's single selection knob stay in sync with the real
// benchmark and project rows.

// 1. The runner's project-derived hotspot micro rows, computed from the family
//    predicate over the single source of truth (bench-vs-tsgo.sh) instead of a
//    now-removed workflow shard filter.
const projectHotspotRows = benchmarkNames.filter((name) => (
  PROJECT_HOTSPOT_FAMILIES.some((family) => name.includes(family))
));
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
  "bench runner should publish all current project-derived hotspot micro rows",
);

// 2. The website benchmark classifier must recognize exactly the shared
//    hotspot family contract. Exact set equality (rather than a per-family
//    substring check) means the website and the contract cannot silently
//    diverge: adding or removing a family on either side fails this test.
assert.deepEqual(
  [...websiteHotspotFamilies()].sort(),
  [...PROJECT_HOTSPOT_FAMILIES].sort(),
  "website hotspot classifier families must match the shared PROJECT_HOTSPOT_FAMILIES contract exactly",
);

// 3. The collapsed workflow's single manual `filter` default replaces the old
//    per-family shard matrix as the one knob that selects which rows run. It
//    must reference only real benchmark or project rows so the default can
//    never silently name a row that no longer exists.
const knownRows = new Set([
  ...benchmarkNames,
  ...Object.keys(PROJECT_ROWS_BY_NAME),
]);
const unknownDefaultRows = workflowDefaultFilterRows().filter((row) => !knownRows.has(row));
assert.deepEqual(
  unknownDefaultRows,
  [],
  "bench workflow default filter must reference only real benchmark or project rows",
);

console.log("bench workflow micro coverage tests passed");
