#!/usr/bin/env node
import assert from "node:assert/strict";
import { isTransientGhResult } from "./lib/gh.mjs";

let passed = 0;
function test(name, fn) {
  fn();
  passed += 1;
  console.log(`ok - ${name}`);
}

test("transient network error codes retry; ENOBUFS does not", () => {
  assert.equal(isTransientGhResult({ error: { code: "ETIMEDOUT" } }), true);
  assert.equal(isTransientGhResult({ error: { code: "ECONNREFUSED" } }), true);
  assert.equal(isTransientGhResult({ error: { code: "ENETUNREACH" } }), true);
  assert.equal(isTransientGhResult({ error: { code: "ENOBUFS" } }), false, "hard output-size error");
  assert.equal(isTransientGhResult({ error: { code: "EACCES" } }), false);
});

test("successful exits never retry", () => {
  assert.equal(isTransientGhResult({ status: 0, stdout: "HTTP 502 mentioned in output" }), false);
});

test("5xx / rate-limit failures retry; real failures do not", () => {
  assert.equal(isTransientGhResult({ status: 1, stdout: "", stderr: "HTTP 502" }), true);
  assert.equal(isTransientGhResult({ status: 1, stdout: "", stderr: "You have exceeded a secondary rate limit" }), true);
  assert.equal(isTransientGhResult({ status: 1, stdout: "", stderr: "Bad Gateway" }), true);
  assert.equal(isTransientGhResult({ status: 1, stdout: "", stderr: "HTTP 404: Not Found" }), false);
  assert.equal(isTransientGhResult({ status: 1, stdout: "", stderr: "could not resolve to an Issue" }), false);
});

console.log(`\n${passed} gh tests passed`);
