#!/usr/bin/env node
// Unit tests for the project-compile no-regression ratchet.
import assert from "node:assert/strict";
import { mergeSummaries, evaluateRatchet } from "./project-compat-ratchet.mjs";

const baseline = { rows: { a: "green", b: "green", c: "green" } };
const s = (rows, shards_complete) => ({ rows, shards_complete });
const row = (name, state) => ({ name, state });

let passed = 0;
function check(name, fn) {
  fn();
  passed++;
  console.log(`PASS ${name}`);
}

check("all baseline-green still green -> no regression", () => {
  const m = mergeSummaries([s([row("a", "green"), row("b", "green"), row("c", "green")], true)]);
  const r = evaluateRatchet(baseline, m);
  assert.equal(r.regressions.length, 0);
  assert.equal(r.inconclusive.length, 0);
});

check("baseline-green -> red is a blocking regression", () => {
  const m = mergeSummaries([s([row("a", "green"), row("b", "red"), row("c", "green")], true)]);
  const r = evaluateRatchet(baseline, m);
  assert.deepEqual(r.regressions.map((x) => x.name), ["b"]);
});

check("baseline-green -> yellow is a blocking regression", () => {
  const m = mergeSummaries([s([row("a", "green"), row("b", "yellow"), row("c", "green")], true)]);
  const r = evaluateRatchet(baseline, m);
  assert.deepEqual(r.regressions.map((x) => x.name), ["b"]);
});

check("baseline-green -> gray is inconclusive, never blocks (covers install/fixture failure)", () => {
  const m = mergeSummaries([s([row("a", "green"), row("b", "gray"), row("c", "green")], true)]);
  const r = evaluateRatchet(baseline, m);
  assert.equal(r.regressions.length, 0);
  assert.deepEqual(r.inconclusive.map((x) => x.name), ["b"]);
});

check("baseline-green row absent from summaries is inconclusive, never blocks", () => {
  const m = mergeSummaries([s([row("a", "green"), row("c", "green")], true)]);
  const r = evaluateRatchet(baseline, m);
  assert.equal(r.regressions.length, 0);
  assert.deepEqual(r.inconclusive.map((x) => x.name), ["b"]);
});

check("incomplete shard set never blocks, even with a red baseline row", () => {
  const m = mergeSummaries([s([row("a", "green"), row("b", "red"), row("c", "green")], false)]);
  const r = evaluateRatchet(baseline, m);
  assert.equal(r.regressions.length, 0);
  assert.equal(r.inconclusive.length, 3);
});

check("non-baseline red rows are ignored (advisory)", () => {
  const m = mergeSummaries([s([row("a", "green"), row("b", "green"), row("c", "green"), row("zzz", "red")], true)]);
  const r = evaluateRatchet(baseline, m);
  assert.equal(r.regressions.length, 0);
  assert.equal(r.inconclusive.length, 0);
});

check("rows merge across multiple summaries (guard + canary); last-seen state wins", () => {
  // 'b' lives only in the second (canary) summary; shards_complete=false in one => merged incomplete.
  const guard = s([row("a", "green"), row("c", "green")], true);
  const canary = s([row("b", "green")], true);
  const m = mergeSummaries([guard, canary]);
  assert.equal(m.shardsComplete, true);
  const r = evaluateRatchet(baseline, m);
  assert.equal(r.regressions.length, 0);
  assert.equal(r.inconclusive.length, 0);
  // and a regression in the canary summary is caught
  const m2 = mergeSummaries([guard, s([row("b", "red")], true)]);
  assert.deepEqual(evaluateRatchet(baseline, m2).regressions.map((x) => x.name), ["b"]);
});

check("missing shards_complete field defaults to complete (single-job summary)", () => {
  const m = mergeSummaries([{ rows: [row("a", "green"), row("b", "red"), row("c", "green")] }]);
  assert.equal(m.shardsComplete, true);
  assert.deepEqual(evaluateRatchet(baseline, m).regressions.map((x) => x.name), ["b"]);
});

console.log(`\nAll ${passed} project-compat-ratchet tests passed.`);
