#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { normalizePullRequest, readyStateFailures } from "./check-pr-ready-state.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(ROOT, "scripts", "ci", "check-pr-ready-state.mjs");
const CI_WORKFLOW = path.join(ROOT, ".github", "workflows", "ci.yml");
const WORKFLOW_DIR = path.join(ROOT, ".github", "workflows");

function readyPr(overrides = {}) {
  return {
    number: 123,
    title: "fix(checker): sample",
    body: "AgentName: TestAgent\n\n## Summary\nReady for review.\n",
    draft: false,
    labels: [],
    ...overrides,
  };
}

function withFixture(pr, fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-pr-ready-state-"));
  try {
    const fixture = path.join(dir, "pr.json");
    fs.writeFileSync(fixture, `${JSON.stringify(pr)}\n`);
    return fn(fixture);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function runFixture(pr) {
  return withFixture(pr, (fixture) => spawnSync(process.execPath, [SCRIPT, "--fixture", fixture], {
    cwd: ROOT,
    encoding: "utf8",
  }));
}

assert.deepEqual(readyStateFailures(readyPr()), []);

assert.deepEqual(
  normalizePullRequest({
    number: 456,
    title: "chore(ci): sample",
    body: "AgentName: TestAgent\n",
    draft: false,
    labels: [{ name: "agent:Studio-manager" }, { name: "ready-review" }],
  }),
  {
    number: 456,
    title: "chore(ci): sample",
    body: "AgentName: TestAgent\n",
    draft: false,
    labels: ["agent:Studio-manager", "ready-review"],
  },
);

assert.deepEqual(
  readyStateFailures(readyPr({ labels: ["WIP"] })),
  ["WIP label"],
);

assert.deepEqual(
  readyStateFailures(readyPr({ title: "[WIP] fix(checker): sample" })),
  ["[WIP] title marker"],
);

assert.deepEqual(
  readyStateFailures(readyPr({ body: "AgentName: TestAgent\n\nStatus: WIP pending verification\n" })),
  ["body WIP status line"],
);

assert.deepEqual(
  readyStateFailures(readyPr({ body: "AgentName: TestAgent\n\nBlocker: conformance aggregate is red\n" })),
  ["body blocker declaration"],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    body: "AgentName: TestAgent\n\nThis PR is blocked on lint while the branch is reviewed.\n",
  })),
  ["body WIP declaration"],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    body: "AgentName: TestAgent\n\nThis PR fixes a bug where ready-state checks were blocked by stale WIP labels.\n",
  })),
  [],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    body: "AgentName: TestAgent\n\nThis branch removes the blocker from the project-corpus gate.\n",
  })),
  [],
);

assert.deepEqual(
  readyStateFailures(readyPr({
    draft: true,
    labels: ["WIP"],
    title: "[WIP] fix(checker): sample",
    body: "AgentName: TestAgent\n\nStatus: WIP pending verification\n",
  })),
  [],
);

const failing = runFixture(readyPr({
  labels: ["WIP"],
  title: "[WIP] fix(checker): sample",
  body: "AgentName: TestAgent\n\nReadiness: blocked on lint\n",
}));
assert.equal(failing.status, 1, failing.stderr);
assert.match(failing.stderr, /Ready PRs must not carry WIP status/);
assert.match(failing.stderr, /WIP label/);
assert.match(failing.stderr, /\[WIP\] title marker/);
assert.match(failing.stderr, /body WIP status line/);
assert.match(failing.stderr, /Repair: remove WIP labels/);

const passingDraft = runFixture(readyPr({
  draft: true,
  labels: ["WIP"],
  title: "[WIP] fix(checker): sample",
}));
assert.equal(passingDraft.status, 0, passingDraft.stderr);
assert.match(passingDraft.stdout, /Ready-state WIP check passed/);

const ciWorkflow = fs.readFileSync(CI_WORKFLOW, "utf8");

assert.doesNotMatch(
  ciWorkflow,
  /signoff-gate|PR Signoff|context == "signoff"|scripts\/ci\/signoff\.sh/,
  "PR CI should not require the local signoff status",
);

assert.doesNotMatch(
  ciWorkflow,
  /TSZ_CI_EMERGENCY_SCALE_DOWN|CI Light Summary|gate-path-classifier|metadata-only|metadata_only/,
  "PR CI should not keep the old emergency metadata/light-summary mode",
);

assert.doesNotMatch(
  ciWorkflow,
  /cargo-shear|cargo-deny|dist-binaries|node-harness-prep|lsp-e2e|project-compile|wasm|bench-script-smoke|perf-tool-smoke|arch-tool-smoke|conformance-snapshot-gate|unit-checker-integration/,
  "PR CI should expose only the requested core checks",
);

for (const job of [
  "clippy",
  "unit",
  "conformance",
  "conformance-aggregate",
  "emit",
  "emit-aggregate",
  "fourslash",
  "fourslash-aggregate",
  "ci-summary",
]) {
  assert.match(
    ciWorkflow,
    new RegExp(`\\n\\s{2}${job}:\\n[\\s\\S]+?runs-on: ubuntu-latest`),
    `${job} should run on hosted Ubuntu`,
  );
}

assert.match(
  ciWorkflow,
  /name: clippy[\s\S]+?cargo fmt --all --check[\s\S]+?cargo check --workspace --all-targets[\s\S]+?cargo clippy --profile ci-lint --workspace[\s\S]+?cargo nextest run --workspace/,
  "The historical clippy check should enforce the complete rewrite foundation gate",
);

assert.match(
  ciWorkflow,
  /TSZ_CI_FOURSLASH_WORKERS:\s*1/,
  "Fourslash observation shards should stay bounded on hosted runners",
);

const fullCi = fs.readFileSync(
  path.join(ROOT, "scripts", "ci", "full-ci.sh"),
  "utf8",
);

assert.match(
  fullCi,
  /CARGO_INCREMENTAL=0 "\$ROOT_DIR\/scripts\/safe-run\.sh"[\s\S]+cargo build --profile dist-fast/,
  "build_test_binaries should force non-incremental dist-fast builds regardless of inherited CI environment",
);

assert.doesNotMatch(
  ciWorkflow,
  /scripts\/ci\/github-suite\.sh\s+(lint|checker-integration|dist-binaries|node-harness-prep|lsp-e2e|wasm-all)/,
  "PR CI should not invoke removed helper suites",
);

assert.equal(
  fs.existsSync(path.join(WORKFLOW_DIR, "campaign-flag-lane.yml")),
  false,
  "the retired implementation campaign workflow must stay deleted",
);

const installWorkflow = fs.readFileSync(
  path.join(WORKFLOW_DIR, "install-test.yml"),
  "utf8",
);
assert.match(
  installWorkflow,
  /^\s{2}pull_request:\n\s{4}paths:/m,
  "install-test.yml may run only as a path-scoped installer contract on PRs",
);
assert.match(
  installWorkflow,
  /unavailable-contract:[\s\S]+?Bash installer fails closed[\s\S]+?PowerShell installer fails closed/,
  "installer CI should prove both public installers fail closed during R0",
);
assert.doesNotMatch(
  installWorkflow,
  /installer-from-release|releases\/download|github\.event_name == 'release'/,
  "installer CI must not restore a historical release-asset path",
);

assert.match(
  ciWorkflow,
  /\n\s{2}ci-summary:\n[\s\S]+?needs:[\s\S]+?- clippy[\s\S]+?- unit[\s\S]+?- conformance[\s\S]+?- conformance-aggregate[\s\S]+?- emit[\s\S]+?- emit-aggregate[\s\S]+?- fourslash[\s\S]+?- fourslash-aggregate/,
  "CI Summary should wait on rewrite gates and full-corpus observation leaves",
);

assert.match(
  ciWorkflow,
  /jobs = \{[\s\S]+?"clippy"[\s\S]+?"unit"[\s\S]+?"conformance"[\s\S]+?"conformance-aggregate"[\s\S]+?"emit"[\s\S]+?"emit-aggregate"[\s\S]+?"fourslash"[\s\S]+?"fourslash-aggregate"/,
  "CI Summary should require the same core check set at runtime",
);

assert.doesNotMatch(
  ciWorkflow,
  /\n\s{2}ci-summary:\n[\s\S]+?jobs\.update\(/,
  "CI Summary must not require a raw emit shard leaf; the sharded emit suite is gated through emit-aggregate",
);
