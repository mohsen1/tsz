#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { benchReadinessMessages } from "./bench-readiness-banner.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");

assert.deepEqual(benchReadinessMessages(null), []);

assert.match(
  benchReadinessMessages(null, {
    two_x_target: {
      eligible_green_rows: 2,
      rows_below_target: 1,
      missing_attribution_rows: [],
    },
  }).join(" "),
  /1\/2 green row\(s\) below the 2x tsgo target/,
);

assert.match(
  benchReadinessMessages({ artifact_absent: true }).join(" "),
  /No recent benchmark artifact/,
);

assert.match(
  benchReadinessMessages({ missing: 2 }).join(" "),
  /missing 2 required row\(s\)/,
);

assert.match(
  benchReadinessMessages({
    duplicate_rows: [{ name: "utility-types-project", count: 2 }],
  }).join(" "),
  /duplicate required row\(s\)/,
);

assert.match(
  benchReadinessMessages({
    source_freshness: {
      current: false,
      warning: "source abc123 differs from expected def456",
    },
  }).join(" "),
  /not current release truth/,
);

assert.match(
  benchReadinessMessages({
    metadata_clean: false,
    metadata_warnings_total: 3,
  }).join(" "),
  /metadata has 3 warning\(s\)/,
);

assert.doesNotMatch(
  benchReadinessMessages({
    metadata_clean: true,
    metadata_warnings_total: 0,
    source_freshness: { current: true },
  }).join(" "),
  /warning|stale|missing|duplicate/i,
);

assert.match(
  benchReadinessMessages(
    {
      metadata_clean: true,
      metadata_warnings_total: 0,
      source_freshness: { current: true },
    },
    {
      two_x_target: {
        eligible_green_rows: 8,
        rows_below_target: 3,
        missing_attribution_rows: ["ts-toolbelt-project", "vite-vanilla-ts-app"],
      },
    },
  ).join(" "),
  /3\/8 green row\(s\) below the 2x tsgo target/,
);

assert.match(
  benchReadinessMessages(
    {},
    {
      two_x_target: {
        rows_below_target: 1,
        missing_attribution_rows: ["ts-toolbelt-project"],
      },
    },
  ).join(" "),
  /missing attribution for 1 2x target gap row\(s\)/,
);

const websiteBenchmarkData = fs.readFileSync(
  path.join(ROOT, "crates", "tsz-website", "src", "_data", "benchmark_data.js"),
  "utf8",
);
assert.match(
  websiteBenchmarkData,
  /current compatibility blocker/,
  "website compatibility dashboard should frame project rows as compatibility blockers",
);
assert.doesNotMatch(
  websiteBenchmarkData,
  /benchReadinessBanner|loadBenchReadinessStatus|loadBenchWinnerReport|bench-readiness-warning/,
  "website compatibility dashboard should not render benchmark readiness warnings",
);
assert.doesNotMatch(
  websiteBenchmarkData,
  /current release truth|public speed claims|2x target gap/,
  "website compatibility dashboard should not include benchmark launch messaging",
);

console.log("bench readiness banner tests passed");
