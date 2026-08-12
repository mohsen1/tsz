#!/usr/bin/env node
/**
 * Ratchet + parser tests for the project-fixture stub fidelity audit (#16311).
 *
 * Two jobs:
 *   1. Verify the static heredoc parser, the JSON-vs-stub body classifier, and
 *      the per-body metric on a synthetic shard (no dependency on the real
 *      fixture source, so a fixture edit never breaks the parser test).
 *   2. Guard the committed baseline: the live audit must not have grown its
 *      `any`-erosion past baseline, and every stub writer must be acknowledged
 *      in the baseline. This converts #16311's silent erosion into a tracked,
 *      monotonically-improving quantity.
 *
 * Static-only, like `test-project-fixture-deprecations.mjs`: no network, no repo
 * checkout, no build.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  auditStubFidelity,
  checkAgainstBaseline,
  extractStubHeredocs,
  isStubBody,
  loadBaseline,
  measureStubBody,
  toBaseline,
  BASELINE_PATH,
} from "./project-fixture-stub-fidelity.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`  ✓ ${name}`);
    passed++;
  } catch (err) {
    console.error(`  ✗ ${name}`);
    console.error(`    ${err.message}`);
    failed++;
  }
}

console.log("test-project-fixture-stub-fidelity: erosion ratchet + parser");

// A synthetic shard exercising the three shapes the parser must handle:
//  - a stub heredoc written to `"$fixture_dir/…d.ts"`,
//  - a JSON tsconfig heredoc (must be skipped by body classification),
//  - a stub heredoc written to a bare `"$output"` variable (the blind spot the
//    body-classification fix closes — a `.d.ts`-target-only gate would miss it).
const SYNTHETIC_SHARD = [
  "tsz_write_demo_stubs() {",
  '  local output="$1"',
  '  cat > "$fixture_dir/tsz-bench-external-modules.d.ts" <<\'TYPES\'',
  "declare module 'left-pad' {",
  "  export type Pad<A = any, B = any> = any;",
  "  export const pad: any;",
  "  // a prose comment mentioning any should not count",
  "  const __d: any;",
  "  export default __d;",
  "}",
  "TYPES",
  "}",
  "",
  "tsz_write_config_only() {",
  '  local output="$1"',
  '  cat > "$output" <<\'JSON\'',
  '{ "compilerOptions": { "strict": true, "types": [] } }',
  "JSON",
  "}",
  "",
  "tsz_write_globals_to_output() {",
  '  local output="$1"',
  '  cat > "$output" <<\'TYPES\'',
  "declare const value: any;",
  "TYPES",
  "}",
].join("\n");

test("isStubBody keeps TS declarations and rejects JSON tsconfigs", () => {
  assert.equal(isStubBody("declare module 'x' {}"), true);
  assert.equal(isStubBody("export const a: any;"), true);
  assert.equal(isStubBody("// lead comment\ninterface W { x: any }"), true);
  assert.equal(isStubBody('{ "compilerOptions": {} }'), false);
  assert.equal(isStubBody('\n  // note\n  { "x": 1 }'), false);
});

test("extractStubHeredocs finds stub heredocs and skips JSON heredocs", () => {
  const recs = extractStubHeredocs(SYNTHETIC_SHARD, "synthetic.sh");
  assert.equal(recs.length, 2, "two stub heredocs; JSON tsconfig skipped");
  assert.equal(recs[0].writer, "tsz_write_demo_stubs");
  assert.equal(recs[0].module, "tsz-bench-external-modules.d.ts");
  // The `"$output"`-targeted stub is captured despite no `.d.ts` in the target.
  assert.equal(recs[1].writer, "tsz_write_globals_to_output");
});

test("measureStubBody counts declare-module, any tokens, exports; ignores comments", () => {
  const recs = extractStubHeredocs(SYNTHETIC_SHARD, "synthetic.sh");
  const m = measureStubBody(recs[0].body);
  assert.equal(m.declareModules, 1, "one declare module");
  // any tokens: Pad<A = any, B = any> = any (3) + pad: any (1) + __d: any (1)
  // = 5; the comment's "any" is stripped.
  assert.equal(m.anyTokens, 5, `expected 5 any tokens, got ${m.anyTokens}`);
  assert.equal(m.exports, 3, "export type + export const + export default");
});

test("auditStubFidelity aggregates a synthetic temp tree by writer", () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-stub-fidelity-"));
  try {
    const rel = "scripts/bench/project-fixtures.sh";
    fs.mkdirSync(path.join(tmp, path.dirname(rel)), { recursive: true });
    fs.writeFileSync(path.join(tmp, rel), SYNTHETIC_SHARD);
    // Scope to the one synthetic shard: absent shards are now a hard error.
    const audit = auditStubFidelity(tmp, [rel]);
    assert.equal(audit.writers.tsz_write_demo_stubs.anyTokens, 5);
    assert.equal(audit.writers.tsz_write_demo_stubs.declareModules, 1);
    assert.equal(audit.writers.tsz_write_globals_to_output.anyTokens, 1);
    assert.ok(!("tsz_write_config_only" in audit.writers), "JSON heredoc → no entry");
    assert.equal(audit.totals.anyTokens, 6);
    assert.equal(audit.totals.writers, 2);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test("extractStubHeredocs throws on an unterminated heredoc", () => {
  const bad = [
    "tsz_write_broken() {",
    '  cat > "$dir/x.d.ts" <<\'TYPES\'',
    "declare module 'oops' {}",
  ].join("\n");
  assert.throws(() => extractStubHeredocs(bad, "broken.sh"), /unterminated heredoc/);
});

// ---------------------------------------------------------------------------
// checkAgainstBaseline semantics on crafted inputs.
// ---------------------------------------------------------------------------

test("checkAgainstBaseline flags growth as a regression", () => {
  const audit = { writers: { a: { anyTokens: 10, declareModules: 2 } } };
  const baseline = { writers: { a: { anyTokens: 5, declareModules: 2 } } };
  const r = checkAgainstBaseline(audit, baseline);
  assert.equal(r.ok, false);
  assert.match(r.regressions.join("\n"), /a\.anyTokens grew 5 -> 10/);
});

test("checkAgainstBaseline flags an unrecorded new writer", () => {
  const audit = { writers: { fresh: { anyTokens: 3, declareModules: 1 } } };
  const baseline = { writers: {} };
  const r = checkAgainstBaseline(audit, baseline);
  assert.equal(r.ok, false);
  assert.match(r.regressions.join("\n"), /new stub writer 'fresh'/);
});

test("checkAgainstBaseline accepts improvement and records it", () => {
  const audit = { writers: { a: { anyTokens: 2, declareModules: 1 } } };
  const baseline = { writers: { a: { anyTokens: 5, declareModules: 2 } } };
  const r = checkAgainstBaseline(audit, baseline);
  assert.equal(r.ok, true, "shrinking erosion is never a failure");
  assert.equal(r.improvements.length, 2, "both metrics improved");
});

test("checkAgainstBaseline reports a removed writer without failing", () => {
  const audit = { writers: {} };
  const baseline = { writers: { gone: { anyTokens: 5, declareModules: 1 } } };
  const r = checkAgainstBaseline(audit, baseline);
  assert.equal(r.ok, true);
  assert.deepEqual(r.missing, ["gone"]);
});

// ---------------------------------------------------------------------------
// Live baseline guard — the actual ratchet. One audit, shared by both checks.
// ---------------------------------------------------------------------------

const liveAudit = auditStubFidelity(ROOT);
const liveBaseline = loadBaseline();

test("committed baseline file exists and is well-formed", () => {
  assert.ok(fs.existsSync(BASELINE_PATH), "baseline JSON is committed");
  assert.ok(liveBaseline.writers && typeof liveBaseline.writers === "object");
  assert.ok(
    liveBaseline.totals && typeof liveBaseline.totals.anyTokens === "number",
  );
});

test("baseline totals equal the sum of its per-writer entries", () => {
  const sum = { declareModules: 0, anyTokens: 0, exports: 0, writers: 0 };
  for (const e of Object.values(liveBaseline.writers)) {
    sum.declareModules += e.declareModules;
    sum.anyTokens += e.anyTokens;
    sum.exports += e.exports;
    sum.writers += 1;
  }
  assert.deepEqual(sum, liveBaseline.totals, "hand-edited baseline totals drifted");
});

test("auditStubFidelity hard-errors on a missing shard (no silent coverage loss)", () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-stub-fidelity-miss-"));
  try {
    assert.throws(
      () => auditStubFidelity(tmp, ["scripts/bench/does-not-exist.sh"]),
      /stub source shard not found/,
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test("live fixture stubs do not exceed the fidelity baseline", () => {
  const result = checkAgainstBaseline(liveAudit, liveBaseline);
  const detail = [...result.regressions].join("\n  ");
  assert.equal(
    result.ok,
    true,
    `stub fidelity regressed against the baseline:\n  ${detail}\n` +
      `If this is an intentional new/changed fixture, acknowledge it with:\n` +
      `  node scripts/bench/project-fixture-stub-fidelity.mjs --update-baseline`,
  );
});

test("regenerated baseline is byte-stable (no nondeterminism in the audit)", () => {
  const regenerated = JSON.stringify(toBaseline(liveAudit).writers);
  const committed = JSON.stringify(liveBaseline.writers);
  assert.equal(
    regenerated,
    committed,
    "re-running the audit produced different per-writer counts than the " +
      "committed baseline; re-pin with --update-baseline",
  );
});

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
