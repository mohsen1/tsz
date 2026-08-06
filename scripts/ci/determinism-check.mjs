#!/usr/bin/env node
// Determinism regression harness (issue #16309).
//
// Standing Rule 5 (cache/order honesty): same binary + same fixture + same
// tsconfig, no flags changed between runs => byte-identical diagnostics. A
// result that depends on visit order or thread schedule is a cache-key or
// cache-attribution bug, not a formatting wobble.
//
// This harness makes that rule *falsifiable and enforceable*, at a deliberately
// chosen granularity: it checks that the diagnostic *set* is identical run to
// run (content-identical modulo emission order), not that the raw byte stream
// is. It runs one tsz invocation N times, normalizes each run's output the same
// way the issue's repro loop does (`... 2>&1 | sort`: strip the invoking-cwd
// path prefix, then sort lines), fingerprints the normalized text, and reports
// how many distinct outputs the N runs produced. One distinct output =>
// content-deterministic. Two or more => the binary disagrees with itself on the
// diagnostic content for identical input.
//
// Sorting is a conscious scope choice, not an oversight: `tsc` gives no stable
// inter-file diagnostic order under `-p`, so emission-order-only flicker is not
// a `tsc`-parity defect and folding it away isolates the class this guard
// exists to catch — a flickering TS2345, a swapped rendered target type. A
// future guard could measure order-divergence as a *separate*, individually
// downgradeable signal; today it is intentionally out of scope.
//
// Why this is needed (from #16309): every per-row diagnostic count in the
// corpus trackers is measured to +/-1..3 by non-determinism, so "fixed N
// diagnostics" is unfalsifiable at that granularity and any diagnostic-delta
// ratchet on project rows flakes. A cheap, permanent guard that runs a row N
// times and fails on divergence would have caught the mobx regression; this is
// that guard.
//
// Policy (scripts/ci/determinism-policy.json): rows with a KNOWN, tracked
// non-determinism defect are advisory (reported, never blocking) so an
// already-filed bug never wedges the lane; every other row blocks on
// divergence. When a race is fixed, delete its row from the policy and the
// guard begins blocking future regressions automatically.
//
// Usage:
//   node scripts/ci/determinism-check.mjs \
//     --row mobx-project --runs 8 --cwd <project-dir> \
//     -- <tsz-bin> --noEmit -p tsconfig.json
//
//   # or feed pre-captured run outputs (one file per run) for offline analysis:
//   node scripts/ci/determinism-check.mjs --row mobx-project \
//     --inputs run1.txt run2.txt run3.txt
//
// Exit codes:
//   0 - deterministic, OR divergence on a known-flaky (advisory) row
//   1 - divergence on a row held to strict reproducibility (blocking)
//   2 - configuration error (bad args, unreadable policy/inputs, run failed)
import fs from "node:fs";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";

// --- Pure, unit-tested core ------------------------------------------------

// Normalize one run's raw stdout+stderr into the canonical comparison form.
// Pure string transform: FS resolution of the staged dir happens once at the
// CLI boundary (see `collectOutputs`), and the already-resolved `prefixes` are
// passed in here.
//
// Two transforms, matching the issue's `... 2>&1 | sort` repro and its
// path-prefix negative-control note (comment 1: normalizing the invoking-cwd
// prefix still left 6 distinct mobx outputs, so the divergence is real, not a
// path artifact):
//   1. Rewrite each `prefixes` spelling of the staged dir to a stable sentinel
//      so the same diagnostic anchored at the same file compares equal
//      regardless of where the fixture was staged. `prefixes` is longest-first
//      (see `pathPrefixVariants`) so a nested path is rewritten before its
//      parent; the bare directory is replaced, preserving the following `/` so
//      `<dir>/packages/x.ts` -> `<project>/packages/x.ts`. `replaceAll` with a
//      string needle is a literal replace, so a path with regex metacharacters
//      needs no escaping.
//   2. Sort non-empty lines. Diagnostic *emission order* across a parallel
//      schedule is not the invariant under test (tsc itself does not promise a
//      stable inter-file order under `-p`); diagnostic *content* is. Sorting
//      isolates content divergence (a flickering TS2345, a swapped rendered
//      type) from benign ordering.
export function normalizeOutput(text, { prefixes = [] } = {}) {
  let out = String(text);
  for (const prefix of prefixes) {
    out = out.replaceAll(prefix, "<project>");
  }
  // `\r?\n` split folds CRLF in one pass; the per-line trailing-space strip
  // also drops any stray `\r`.
  const lines = out
    .split(/\r?\n/)
    .map((l) => l.replace(/\s+$/u, ""))
    .filter((l) => l.length > 0);
  lines.sort();
  return lines.join("\n") + (lines.length > 0 ? "\n" : "");
}

// The absolute-path prefix spellings a diagnostic might carry for the same
// staged dir. A stage dir is frequently a symlink (CI temp roots — the repo has
// already hit a `/tmp` symlink artifact), so a diagnostic can anchor at either
// the symlink path or its resolved real path depending on how tsz resolved the
// file. Normalizing BOTH to `<project>` keeps two runs that resolved the prefix
// differently comparing equal. Returned longest-first (deduped) so a nested
// spelling is rewritten before a prefix of it. Kept tiny and dependency-free.
export function pathPrefixVariants(projectDir) {
  const dir = String(projectDir);
  const variants = new Set();
  if (dir.length > 0) {
    variants.add(dir);
    try {
      variants.add(fs.realpathSync(dir));
    } catch {
      // Dir may not exist (e.g. offline --inputs analysis with a synthetic
      // --cwd); the raw spelling is still worth normalizing.
    }
  }
  return [...variants].sort((a, b) => b.length - a.length);
}

export function fingerprint(normalizedText) {
  return crypto.createHash("sha256").update(normalizedText).digest("hex");
}

// Reduce N normalized outputs to a divergence summary. `histogram` is ordered
// by descending count then hash, so the report is stable run-to-run (the tool
// that guards determinism must itself be deterministic).
export function summarizeRuns(normalizedOutputs) {
  // hash -> { hash, count, sample }; the first-seen text is kept as the sample
  // so the report can diff two distinct outputs without a second index.
  const byHash = new Map();
  for (const output of normalizedOutputs) {
    const h = fingerprint(output);
    const entry = byHash.get(h);
    if (entry) entry.count += 1;
    else byHash.set(h, { hash: h, count: 1, sample: output });
  }
  const histogram = [...byHash.values()].sort(
    (a, b) => b.count - a.count || (a.hash < b.hash ? -1 : 1),
  );
  return {
    total: normalizedOutputs.length,
    distinct: histogram.length,
    histogram,
  };
}

// First line that differs between two normalized (already line-sorted) outputs,
// as a readable one-line diff. Returns null when identical. Because both sides
// are sorted, a divergent line surfaces as a `<`/`>` pair at the first index
// where they disagree — enough to point a reader at the flickering diagnostic.
export function firstDivergence(aNorm, bNorm) {
  const a = aNorm.split("\n");
  const b = bNorm.split("\n");
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) {
      return { index: i, a: a[i] ?? null, b: b[i] ?? null };
    }
  }
  return null;
}

// Load and validate the policy document. A malformed policy is a loud config
// error (exit 2), never a silent "everything blocks" or "everything passes".
export function parsePolicy(raw) {
  const doc = typeof raw === "string" ? JSON.parse(raw) : raw;
  const knownFlaky = new Map();
  const src = doc && typeof doc === "object" ? doc.known_flaky ?? {} : {};
  if (typeof src !== "object" || src === null || Array.isArray(src)) {
    throw new Error("policy.known_flaky must be an object keyed by row name");
  }
  for (const [name, meta] of Object.entries(src)) {
    if (!meta || typeof meta.issue !== "number") {
      throw new Error(`policy.known_flaky['${name}'] must carry a numeric 'issue'`);
    }
    knownFlaky.set(name, { issue: meta.issue, reason: meta.reason ?? "" });
  }
  return { knownFlaky };
}

// Decide the gate outcome for one row given its run summary and the policy.
//   deterministic  -> pass (blocking:false), always
//   divergent + known-flaky -> advisory (blocking:false), names the issue
//   divergent + not listed   -> blocking:true
export function evaluateGate(rowName, summary, policy) {
  const deterministic = summary.distinct <= 1;
  if (deterministic) {
    return { deterministic: true, blocking: false, status: "deterministic", issue: null, reason: "" };
  }
  const flaky = policy.knownFlaky.get(rowName);
  if (flaky) {
    return {
      deterministic: false,
      blocking: false,
      status: "known-flaky-advisory",
      issue: flaky.issue,
      reason: flaky.reason,
    };
  }
  return { deterministic: false, blocking: true, status: "divergent", issue: null, reason: "" };
}

// Render a stable, human-readable report block for one row.
export function renderReport(rowName, summary, gate) {
  const lines = [];
  lines.push(
    `determinism[${rowName}]: ${summary.total} run(s) -> ${summary.distinct} distinct output(s) [${gate.status}]`,
  );
  if (summary.distinct > 1) {
    for (const { hash, count } of summary.histogram) {
      lines.push(`  ${count}x ${hash.slice(0, 12)}`);
    }
    const [first, second] = summary.histogram;
    const div = firstDivergence(first.sample, second.sample);
    if (div) {
      lines.push(`  first divergent line (index ${div.index}):`);
      lines.push(`    < ${div.a ?? "<absent>"}`);
      lines.push(`    > ${div.b ?? "<absent>"}`);
    }
    if (gate.issue) {
      const why = gate.reason ? `: ${gate.reason}` : "";
      lines.push(`  advisory: known-flaky, tracked by #${gate.issue} (not blocking)${why}`);
    }
  }
  return lines.join("\n");
}

// --- CLI runner ------------------------------------------------------------

function parseArgs(argv) {
  const args = {
    row: null,
    runs: 8,
    cwd: process.cwd(),
    policyPath: new URL("./determinism-policy.json", import.meta.url).pathname,
    inputs: [],
    command: [],
  };
  let i = 0;
  for (; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--") {
      args.command = argv.slice(i + 1);
      break;
    } else if (a === "--row") {
      args.row = argv[++i];
    } else if (a === "--runs") {
      args.runs = Number.parseInt(argv[++i], 10);
    } else if (a === "--cwd") {
      args.cwd = argv[++i];
    } else if (a === "--policy") {
      args.policyPath = argv[++i];
    } else if (a === "--inputs") {
      while (i + 1 < argv.length && !argv[i + 1].startsWith("--")) {
        args.inputs.push(argv[++i]);
      }
    } else {
      throw new Error(`unknown argument: ${a}`);
    }
  }
  return args;
}

function loadPolicy(policyPath) {
  return parsePolicy(fs.readFileSync(policyPath, "utf8"));
}

// Collect N normalized outputs, either by executing the command N times or by
// reading N pre-captured files.
function collectOutputs(args) {
  // Resolve the staged-dir prefix spellings once (the only FS work), then feed
  // the pure `normalizeOutput` per run.
  const prefixes = pathPrefixVariants(args.cwd);
  if (args.inputs.length > 0) {
    return args.inputs.map((path) =>
      normalizeOutput(fs.readFileSync(path, "utf8"), { prefixes }),
    );
  }
  if (args.command.length === 0) {
    throw new Error("no command to run (pass `-- <cmd> ...`) and no --inputs");
  }
  const [bin, ...rest] = args.command;
  const outputs = [];
  for (let r = 0; r < args.runs; r++) {
    const res = spawnSync(bin, rest, {
      cwd: args.cwd,
      encoding: "utf8",
      maxBuffer: 256 * 1024 * 1024,
    });
    if (res.error) {
      throw new Error(`run ${r + 1} failed to spawn: ${res.error.message}`);
    }
    outputs.push(normalizeOutput(`${res.stdout ?? ""}${res.stderr ?? ""}`, { prefixes }));
  }
  return outputs;
}

// Config-error exit: report on stderr and exit 2. Shared by every
// bad-args/unreadable-input site so the prefix and code stay in one place.
function die(message) {
  process.stderr.write(`determinism-check: ${message}\n`);
  process.exit(2);
}

function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (err) {
    die(err.message);
  }
  if (!args.row) {
    die("--row <name> is required");
  }
  if (!Number.isInteger(args.runs) || args.runs < 2) {
    die("--runs must be an integer >= 2");
  }

  let policy;
  try {
    policy = loadPolicy(args.policyPath);
  } catch (err) {
    die(`cannot load policy ${args.policyPath}: ${err.message}`);
  }

  let outputs;
  try {
    outputs = collectOutputs(args);
  } catch (err) {
    die(err.message);
  }

  const summary = summarizeRuns(outputs);
  const gate = evaluateGate(args.row, summary, policy);
  process.stdout.write(renderReport(args.row, summary, gate) + "\n");

  if (gate.blocking) {
    process.stderr.write(
      `::error title=Non-deterministic diagnostics::row '${args.row}' produced ` +
        `${summary.distinct} distinct outputs across ${summary.total} identical runs. ` +
        `Same binary + same fixture must be byte-identical (Standing Rule 5). ` +
        `If this row has a known, tracked race, add it to ${args.policyPath} with its issue number.\n`,
    );
    process.exit(1);
  }
  process.exit(0);
}

// Only run main when invoked directly, so tests can import the pure functions.
const invokedDirectly =
  process.argv[1] &&
  fs.realpathSync(process.argv[1]) === fs.realpathSync(new URL(import.meta.url).pathname);
if (invokedDirectly) main();
