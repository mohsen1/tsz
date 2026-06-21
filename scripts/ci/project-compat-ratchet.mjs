#!/usr/bin/env node
// Project-compile no-regression ratchet.
//
// Blocks (exit 1) ONLY when a baseline-green row regressed to a genuine failure
// (red/yellow) in a COMPLETE canary run. Everything else is INCONCLUSIVE and
// never blocks, matching the canary's advisory-on-infra-flake design:
//   - gray (fixture invalid / install-clone failure / not measured) -> skip
//   - row absent from the measured summaries                          -> skip
//   - incomplete shard set (a dead/missing self-hosted shard)         -> skip
// Non-baseline rows (currently red/yellow/gray, or brand-new) are advisory and
// never block. The live tsc oracle remains the correctness reference; this file
// only enforces that rows recorded as green in project-compat-baseline.json stay
// green.
//
// Usage:
//   node scripts/ci/project-compat-ratchet.mjs \
//     --baseline scripts/ci/project-compat-baseline.json \
//     <summary1.json> [summary2.json ...]
//
// Exit codes:
//   0 - no regression (all baseline-green rows still green, or inconclusive)
//   1 - one or more baseline-green rows regressed to red/yellow
//   2 - baseline unreadable/invalid (configuration error, surfaced loudly)
import fs from "node:fs";

const BLOCKING_STATES = new Set(["red", "yellow"]);

function readJson(path) {
  return JSON.parse(fs.readFileSync(path, "utf8"));
}

// Merge any number of compatibility summaries into one view. A baseline-green row
// may live in the guard summary (project rows) OR the canary aggregate summary
// (application rows), so callers pass both. Last non-undefined state wins.
// shards_complete is the AND across summaries that report it (absent = treated as
// complete, e.g. a single-job non-sharded summary).
export function mergeSummaries(summaries) {
  const stateByName = new Map();
  let shardsComplete = true;
  for (const summary of summaries) {
    if (summary && summary.shards_complete === false) shardsComplete = false;
    for (const row of summary?.rows ?? []) {
      if (row && typeof row.name === "string") stateByName.set(row.name, row.state);
    }
  }
  return { stateByName, shardsComplete };
}

export function evaluateRatchet(baseline, merged) {
  const baselineGreen = Object.entries(baseline?.rows ?? {})
    .filter(([, want]) => want === "green")
    .map(([name]) => name);
  const regressions = [];
  const inconclusive = [];
  for (const name of baselineGreen) {
    if (!merged.shardsComplete) {
      inconclusive.push({ name, reason: "canary shard set incomplete" });
      continue;
    }
    const state = merged.stateByName.get(name);
    if (state === undefined) {
      inconclusive.push({ name, reason: "row not measured in this run" });
    } else if (state === "green") {
      // held
    } else if (state === "gray") {
      inconclusive.push({ name, reason: "gray (fixture invalid / not measured)" });
    } else if (BLOCKING_STATES.has(state)) {
      regressions.push({ name, state });
    } else {
      inconclusive.push({ name, reason: `unrecognized state '${state}'` });
    }
  }
  return { baselineGreenCount: baselineGreen.length, shardsComplete: merged.shardsComplete, regressions, inconclusive };
}

function parseArgs(argv) {
  const out = { baseline: "scripts/ci/project-compat-baseline.json", summaries: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--baseline") out.baseline = argv[++i];
    else if (a === "--help" || a === "-h") out.help = true;
    else out.summaries.push(a);
  }
  return out;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    process.stdout.write(
      "usage: node scripts/ci/project-compat-ratchet.mjs --baseline <baseline.json> <summary.json> [summary.json ...]\n",
    );
    process.exit(0);
  }

  let baseline;
  try {
    baseline = readJson(args.baseline);
  } catch (err) {
    process.stderr.write(`project-compat-ratchet: cannot read baseline ${args.baseline}: ${err.message}\n`);
    process.exit(2);
  }

  // A missing/unreadable summary is treated as inconclusive (the canary itself
  // surfaces its own failure); the ratchet must not wedge CI on absent data.
  const summaries = [];
  for (const path of args.summaries) {
    try {
      summaries.push(readJson(path));
    } catch (err) {
      process.stderr.write(`project-compat-ratchet: warning: skipping unreadable summary ${path}: ${err.message}\n`);
    }
  }

  const merged = mergeSummaries(summaries);
  const result = evaluateRatchet(baseline, merged);

  const lines = [];
  lines.push(`Project-compile no-regression ratchet: ${result.baselineGreenCount} baseline-green row(s), shards_complete=${result.shardsComplete}.`);
  for (const inc of result.inconclusive) lines.push(`  inconclusive: ${inc.name} (${inc.reason})`);
  for (const reg of result.regressions) lines.push(`  REGRESSION: ${reg.name} green -> ${reg.state}`);
  process.stdout.write(lines.join("\n") + "\n");

  if (result.regressions.length > 0) {
    const names = result.regressions.map((r) => `${r.name} (->${r.state})`).join(", ");
    process.stderr.write(
      `::error title=Project compatibility regression::${result.regressions.length} baseline-green row(s) regressed: ${names}. ` +
        `A change must not make a previously-green project row fail to compile like tsc. ` +
        `If this is an intentional, reviewed change, update scripts/ci/project-compat-baseline.json.\n`,
    );
    process.exit(1);
  }
  process.exit(0);
}

// Only run main when invoked directly, so tests can import the pure functions.
const invokedDirectly = process.argv[1] && fs.realpathSync(process.argv[1]) === fs.realpathSync(new URL(import.meta.url).pathname);
if (invokedDirectly) main();
