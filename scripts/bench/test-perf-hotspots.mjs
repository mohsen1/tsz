#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const HOTSPOTS_SCRIPT = path.join(SCRIPT_DIR, "perf-hotspots.sh");
const script = fs.readFileSync(HOTSPOTS_SCRIPT, "utf8");

function shellSingleQuotedValue(name) {
  const match = script.match(new RegExp(`^${name}='([^']*)'$`, "m"));
  assert.ok(match, `missing ${name} in perf-hotspots.sh`);
  return match[1];
}

const fullFilter = new RegExp(shellSingleQuotedValue("DEFAULT_FILTER_FULL"));
const quickFilter = new RegExp(shellSingleQuotedValue("DEFAULT_FILTER_QUICK"));

for (const [name, filter] of [
  ["DEFAULT_FILTER_FULL", fullFilter],
  ["DEFAULT_FILTER_QUICK", quickFilter],
]) {
  assert.equal(
    filter.test("ts-toolbelt-project"),
    true,
    `${name} must keep the #7378 ts-toolbelt project hotspot`,
  );
  assert.equal(
    filter.test("not-ts-toolbelt-project"),
    false,
    `${name} should stay anchored to exact hotspot labels`,
  );
}

assert.equal(fullFilter.test("DeepPartial optional-chain N=400"), true);
assert.equal(quickFilter.test("DeepPartial optional-chain N=50"), true);

assert.match(
  script,
  /tsgo-winner-report\.mjs"\s+"\$JSON_FILE"\s+"\$WINNER_REPORT"/,
  "perf-hotspots.sh should emit a green tsgo-winner summary beside benchmark JSON",
);
