#!/usr/bin/env node
// Unit tests for the determinism regression harness (issue #16309).
import assert from "node:assert/strict";
import fs from "node:fs";
import {
  normalizeOutput,
  pathPrefixVariants,
  fingerprint,
  summarizeRuns,
  firstDivergence,
  parsePolicy,
  evaluateGate,
  renderReport,
} from "./determinism-check.mjs";

let passed = 0;
function check(name, fn) {
  fn();
  passed++;
  console.log(`PASS ${name}`);
}

// --- normalizeOutput -------------------------------------------------------

check("normalizeOutput sorts lines so emission order is not under test", () => {
  const a = normalizeOutput("b.ts(1,1): error TS1\na.ts(2,2): error TS2\n");
  const b = normalizeOutput("a.ts(2,2): error TS2\nb.ts(1,1): error TS1\n");
  assert.equal(a, b, "two runs differing only in emission order must normalize equal");
});

check("normalizeOutput strips the staged project-dir prefix", () => {
  const raw = "/tmp/stage-123/packages/mobx/src/spy.ts(45,15): error TS2345\n";
  const norm = normalizeOutput(raw, { prefixes: ["/tmp/stage-123"] });
  assert.equal(norm, "<project>/packages/mobx/src/spy.ts(45,15): error TS2345\n");
});

check("normalizeOutput makes two different stage dirs compare equal", () => {
  const line = (dir) => `${dir}/packages/mobx/src/spy.ts(45,15): error TS2345\n`;
  const a = normalizeOutput(line("/tmp/stage-a"), { prefixes: ["/tmp/stage-a"] });
  const b = normalizeOutput(line("/tmp/stage-b-longer"), { prefixes: ["/tmp/stage-b-longer"] });
  assert.equal(a, b, "content is equal once the invoking-cwd prefix is normalized");
});

check("normalizeOutput preserves genuine content divergence (the flickering diagnostic)", () => {
  // Evidence 1 in #16309: a TS2345 that appears in some runs, absent in others.
  const withDiag = normalizeOutput(
    "a.ts(1,1): error TS100\nspy.ts(45,15): error TS2345\n",
    { prefixes: ["/x"] },
  );
  const withoutDiag = normalizeOutput("a.ts(1,1): error TS100\n", { prefixes: ["/x"] });
  assert.notEqual(withDiag, withoutDiag, "a flickering diagnostic must survive normalization");
});

check("normalizeOutput preserves a swapped rendered target type", () => {
  // Evidence 2: two files trade each other's rendered target type.
  const runA = normalizeOutput(
    "computedannotation.ts(52,44): error TS2322: Type 'string' is not assignable to type 'undefined'.\n" +
      "observableannotation.ts(61,44): error TS2322: Type 'string' is not assignable to type 'DecoratorContext[\"kind\"]'.\n",
    { prefixes: ["/x"] },
  );
  const runB = normalizeOutput(
    "computedannotation.ts(52,44): error TS2322: Type 'string' is not assignable to type 'DecoratorContext[\"kind\"]'.\n" +
      "observableannotation.ts(61,44): error TS2322: Type 'string' is not assignable to type 'undefined'.\n",
    { prefixes: ["/x"] },
  );
  assert.notEqual(runA, runB, "a swapped rendered type is a content difference, not an ordering one");
});

check("normalizeOutput handles CRLF and trailing whitespace", () => {
  const a = normalizeOutput("x.ts(1,1): error TS1  \r\n");
  const b = normalizeOutput("x.ts(1,1): error TS1\n");
  assert.equal(a, b);
});

check("normalizeOutput on empty input yields empty string", () => {
  assert.equal(normalizeOutput("   \n\n"), "");
});

// --- helpers ---------------------------------------------------------------

check("pathPrefixVariants returns the bare dir when it does not resolve", () => {
  // A non-existent dir (realpathSync throws, caught) yields just the raw form.
  const v = pathPrefixVariants("/tmp/does-not-exist-16309/x");
  assert.deepEqual(v, ["/tmp/does-not-exist-16309/x"]);
});

check("pathPrefixVariants adds the realpath spelling, longest-first, deduped", () => {
  // cwd exists, so realpathSync succeeds; for a non-symlinked cwd the realpath
  // equals the raw dir and the Set dedups it back to one entry.
  const v = pathPrefixVariants(process.cwd());
  assert.ok(v.length >= 1 && v.length <= 2);
  for (let i = 1; i < v.length; i++) {
    assert.ok(v[i - 1].length >= v[i].length, "variants are longest-first");
  }
});

check("normalizeOutput rewrites a prefix containing regex metacharacters literally", () => {
  const raw = "/tmp/x+y(z)/a.ts(1,1): error TS1\n";
  const norm = normalizeOutput(raw, { prefixes: ["/tmp/x+y(z)"] });
  assert.equal(norm, "<project>/a.ts(1,1): error TS1\n");
});

check("normalizeOutput applies longest-first prefixes (nested before parent)", () => {
  // Mirrors collectOutputs feeding pathPrefixVariants output (longest-first).
  const raw = "/tmp/stage/inner/a.ts(1,1): error TS1\n";
  const norm = normalizeOutput(raw, { prefixes: ["/tmp/stage/inner", "/tmp/stage"] });
  assert.equal(norm, "<project>/a.ts(1,1): error TS1\n");
});

check("fingerprint is stable and content-sensitive", () => {
  assert.equal(fingerprint("hello\n"), fingerprint("hello\n"));
  assert.notEqual(fingerprint("hello\n"), fingerprint("world\n"));
});

// --- summarizeRuns ---------------------------------------------------------

check("summarizeRuns: all identical -> 1 distinct", () => {
  const s = summarizeRuns(["A\n", "A\n", "A\n"]);
  assert.equal(s.total, 3);
  assert.equal(s.distinct, 1);
  assert.equal(s.histogram[0].count, 3);
});

check("summarizeRuns: mixed -> histogram descending by count then hash", () => {
  // 6-of-8 shape from the issue: two md5s twice, ... here: A x3, B x2, C x1.
  const s = summarizeRuns(["A\n", "A\n", "A\n", "B\n", "B\n", "C\n"]);
  assert.equal(s.total, 6);
  assert.equal(s.distinct, 3);
  assert.deepEqual(
    s.histogram.map((h) => h.count),
    [3, 2, 1],
  );
});

check("summarizeRuns histogram order is deterministic across input permutations", () => {
  const a = summarizeRuns(["A\n", "B\n", "A\n", "C\n", "B\n", "A\n"]);
  const b = summarizeRuns(["C\n", "A\n", "B\n", "A\n", "B\n", "A\n"]);
  assert.deepEqual(a.histogram, b.histogram, "the determinism tool must itself be deterministic");
});

// --- firstDivergence -------------------------------------------------------

check("firstDivergence returns null for identical outputs", () => {
  assert.equal(firstDivergence("x\ny\n", "x\ny\n"), null);
});

check("firstDivergence points at the first differing sorted line", () => {
  const d = firstDivergence("a\nb\nc\n", "a\nb\nd\n");
  assert.equal(d.index, 2);
  assert.equal(d.a, "c");
  assert.equal(d.b, "d");
});

check("firstDivergence reports an absent line when one side is shorter", () => {
  const d = firstDivergence("a\nb\n", "a\n");
  assert.equal(d.a, "b");
  assert.equal(d.b, "");
});

// --- parsePolicy -----------------------------------------------------------

check("parsePolicy reads known_flaky entries", () => {
  const p = parsePolicy({ known_flaky: { "mobx-project": { issue: 16309, reason: "race" } } });
  assert.ok(p.knownFlaky.has("mobx-project"));
  assert.equal(p.knownFlaky.get("mobx-project").issue, 16309);
});

check("parsePolicy accepts a JSON string", () => {
  const p = parsePolicy('{"known_flaky":{"r":{"issue":1}}}');
  assert.equal(p.knownFlaky.get("r").issue, 1);
});

check("parsePolicy defaults to empty when known_flaky absent", () => {
  const p = parsePolicy({});
  assert.equal(p.knownFlaky.size, 0);
});

check("parsePolicy rejects a flaky entry without a numeric issue", () => {
  assert.throws(() => parsePolicy({ known_flaky: { r: { reason: "x" } } }), /numeric 'issue'/);
});

check("parsePolicy rejects a non-object known_flaky", () => {
  assert.throws(() => parsePolicy({ known_flaky: [] }), /must be an object/);
});

check("the shipped policy file parses and lists mobx-project as tracked flaky", () => {
  const raw = fs.readFileSync(new URL("./determinism-policy.json", import.meta.url), "utf8");
  const p = parsePolicy(raw);
  assert.equal(p.knownFlaky.get("mobx-project").issue, 16309);
});

// --- evaluateGate ----------------------------------------------------------

const emptyPolicy = { knownFlaky: new Map() };
const flakyPolicy = { knownFlaky: new Map([["mobx-project", { issue: 16309, reason: "" }]]) };

check("evaluateGate: deterministic row passes", () => {
  const g = evaluateGate("rxjs", summarizeRuns(["A\n", "A\n"]), emptyPolicy);
  assert.equal(g.deterministic, true);
  assert.equal(g.blocking, false);
  assert.equal(g.status, "deterministic");
});

check("evaluateGate: divergent unlisted row BLOCKS", () => {
  const g = evaluateGate("rxjs", summarizeRuns(["A\n", "B\n"]), emptyPolicy);
  assert.equal(g.deterministic, false);
  assert.equal(g.blocking, true);
  assert.equal(g.status, "divergent");
});

check("evaluateGate: divergent known-flaky row is advisory, names the issue", () => {
  const g = evaluateGate("mobx-project", summarizeRuns(["A\n", "B\n"]), flakyPolicy);
  assert.equal(g.deterministic, false);
  assert.equal(g.blocking, false);
  assert.equal(g.status, "known-flaky-advisory");
  assert.equal(g.issue, 16309);
});

check("evaluateGate: a known-flaky row that came back deterministic still passes as deterministic", () => {
  // Guards the promotion path: once fixed, a green run reads as plain
  // deterministic (not advisory), so the row is safe to delist.
  const g = evaluateGate("mobx-project", summarizeRuns(["A\n", "A\n", "A\n"]), flakyPolicy);
  assert.equal(g.status, "deterministic");
  assert.equal(g.blocking, false);
});

// --- renderReport ----------------------------------------------------------

check("renderReport for a clean row is a single summary line", () => {
  const s = summarizeRuns(["A\n", "A\n"]);
  const r = renderReport("rxjs", s, evaluateGate("rxjs", s, emptyPolicy));
  assert.match(r, /rxjs.*2 run\(s\) -> 1 distinct output\(s\) \[deterministic\]/);
  assert.equal(r.split("\n").length, 1);
});

check("renderReport for a divergent row includes histogram, first divergence, and (if flaky) the issue", () => {
  const s = summarizeRuns(["a\nb\nc\n", "a\nb\nc\n", "a\nb\nd\n"]);
  const r = renderReport("mobx-project", s, evaluateGate("mobx-project", s, flakyPolicy));
  assert.match(r, /2 distinct output\(s\)/);
  assert.match(r, /first divergent line/);
  assert.match(r, /< c/);
  assert.match(r, /> d/);
  assert.match(r, /#16309/);
});

check("renderReport surfaces the policy reason on the advisory line", () => {
  const s = summarizeRuns(["A\n", "B\n"]);
  const withReason = {
    knownFlaky: new Map([["mobx-project", { issue: 16309, reason: "materialization race" }]]),
  };
  const r = renderReport("mobx-project", s, evaluateGate("mobx-project", s, withReason));
  assert.match(r, /tracked by #16309 \(not blocking\): materialization race/);
});

console.log(`\n${passed} determinism-check unit tests passed.`);
