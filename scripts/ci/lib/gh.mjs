#!/usr/bin/env node
// Shared synchronous `gh` invocation seam for ci-health monitor scripts.
//
// Bounded exponential backoff over transient gh transport failures (5xx,
// secondary rate limit, transient network errors). This retries only the
// transport — never a real verdict/finding — so a monitor's signal is
// unchanged; it just survives a flaky GitHub API call instead of reddening the
// advisory ci-health workflow. See issue #13744.
//
// Consumers keep `fetchJson`/`runCommand` as injectable seams for tests and
// default them to `runGhJson`/`runGh` from here, so the retry policy lives in
// exactly one place.
import { spawnSync } from "node:child_process";

const GH_RETRY_ATTEMPTS = Math.max(1, Number.parseInt(process.env.GH_RETRY_ATTEMPTS || "", 10) || 4);
const GH_RETRY_BASE_MS = Math.max(0, Number.parseInt(process.env.GH_RETRY_BASE_MS || "", 10) || 500);
const GH_RETRY_MAX_MS = 8000;
const DEFAULT_GH_MAX_BUFFER_BYTES = 16 * 1024 * 1024;
const TRANSIENT_NET_CODES = new Set([
  "ETIMEDOUT", "ECONNRESET", "ECONNREFUSED", "EAI_AGAIN", "ENOTFOUND", "EPIPE", "ENETUNREACH",
]);

function sleepSync(ms) {
  if (!(ms > 0)) return;
  // Synchronous sleep without busy-waiting; spawnSync gives us no async seam.
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

export function isTransientGhResult(result) {
  if (result.error) {
    // ENOBUFS is a hard output-size error, not transient.
    if (result.error.code === "ENOBUFS") return false;
    return TRANSIENT_NET_CODES.has(result.error.code);
  }
  if ((result.status ?? 0) === 0) return false;
  const text = `${result.stdout || ""}\n${result.stderr || ""}`;
  return /\bHTTP\s+(?:408|425|429|5\d\d)\b/i.test(text)
    || /secondary rate limit/i.test(text)
    || /\b(?:Bad Gateway|Service Unavailable|Gateway Time-?out|Internal Server Error|Server Error)\b/i.test(text);
}

function spawnGh(args, spawnOptions = {
  encoding: "utf8",
  maxBuffer: DEFAULT_GH_MAX_BUFFER_BYTES,
  stdio: ["ignore", "pipe", "pipe"],
}) {
  let result;
  for (let attempt = 1; attempt <= GH_RETRY_ATTEMPTS; attempt += 1) {
    result = spawnSync("gh", args, spawnOptions);
    if (attempt === GH_RETRY_ATTEMPTS || !isTransientGhResult(result)) break;
    sleepSync(Math.min(GH_RETRY_BASE_MS * 2 ** (attempt - 1), GH_RETRY_MAX_MS));
  }
  return result;
}

/** Run `gh` and parse stdout as JSON; throws on spawn error or non-zero exit. */
export function runGhJson(args) {
  const result = spawnGh(args);
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error([`gh ${args.join(" ")} failed`, result.stdout?.trim(), result.stderr?.trim()]
      .filter(Boolean).join("\n"));
  }
  return JSON.parse(result.stdout);
}

/** Run `gh` and report `{ status, stdout, stderr }` without throwing. */
export function runGh(args) {
  const result = spawnGh(args);
  if (result.error) return { status: 1, stdout: "", stderr: result.error.message };
  return {
    status: result.status ?? 1,
    stdout: (result.stdout || "").trim(),
    stderr: (result.stderr || "").trim(),
  };
}
