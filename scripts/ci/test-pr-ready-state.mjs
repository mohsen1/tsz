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
const GATE_CLASSIFIER = path.join(ROOT, "scripts", "ci", "gate-path-classifier.mjs");

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
const gateClassifier = fs.readFileSync(GATE_CLASSIFIER, "utf8");
assert.match(
  ciWorkflow,
  /pull_request:\s*\n\s*types:\s*\[[^\]]*\bedited\b[^\]]*\]/,
  "CI should rerun PR metadata gates after body/title edits",
);
assert.match(
  ciWorkflow,
  /if \[\[ "\$\{\{ github\.event\.action \}\}" == "edited" \]\]; then[\s\S]+?PR metadata edited[\s\S]+?should_run=false[\s\S]+?full_run=false[\s\S]+?metadata_only_skip=true[\s\S]+?compiler_checks_required=false[\s\S]+?fi/,
  "edited PR events should refresh body/ready-state gates without heavy CI",
);
assert.match(
  ciWorkflow,
  /accepted_summary_names = \("CI Summary",\) if required_summary else \("CI Summary", "CI Light Summary"\)[\s\S]+?job\.get\("name"\) in accepted_summary_names[\s\S]+?Metadata-only CI mirrors successful \{summary_name\}/,
  "metadata-only edited runs should require prior full summaries when publishing protected CI Summary",
);
assert.match(
  ciWorkflow,
  /id: metadata-active-suite[\s\S]+?if: github\.event_name == 'pull_request' && github\.event\.action == 'edited'[\s\S]+?actions\/workflows\/ci\.yml\/runs\?head_sha=\$\{PR_HEAD_SHA\}&event=pull_request[\s\S]+?\.status == "queued"[\s\S]+?\.status == "in_progress"[\s\S]+?\.id != \$current_run_id[\s\S]+?active_suite_found=true[\s\S]+?metadata CI will publish CI Light Summary[\s\S]+?METADATA_ACTIVE_SUITE_FOUND: \$\{\{ steps\.metadata-active-suite\.outputs\.active_suite_found \}\}[\s\S]+?PR metadata edited[\s\S]+?required_summary=false/,
  "metadata-only edited runs should publish CI Light Summary while the exact-head real suite is active",
);
assert.match(
  ciWorkflow,
  /metadata_active_suite_found: \$\{\{ steps\.gate\.outputs\.metadata_active_suite_found \}\}[\s\S]+?echo "metadata_active_suite_found=\$\{METADATA_ACTIVE_SUITE_FOUND:-false\}" >> "\$GITHUB_OUTPUT"[\s\S]+?CI_METADATA_ACTIVE_SUITE_FOUND: \$\{\{ needs\.gate\.outputs\.metadata_active_suite_found \}\}[\s\S]+?metadata_active_suite_found = os\.environ\.get\("CI_METADATA_ACTIVE_SUITE_FOUND"\) == "true"[\s\S]+?if not required_summary and metadata_active_suite_found:[\s\S]+?active exact-head CI suite[\s\S]+?return/,
  "metadata-only edited runs should pass CI Light Summary when an active exact-head suite has not published its summary yet",
);
assert.match(
  ciWorkflow,
  /accepted_summary_label = "CI Summary" if required_summary else "CI Summary or CI Light Summary"[\s\S]+?previous \{accepted_summary_label\}/,
  "metadata-only edited runs should report the accepted prior summary class when no mirror exists",
);

assert.match(
  ciWorkflow,
  /node scripts\/ci\/gate-path-classifier\.mjs/,
  "CI gate should use the shared path classifier instead of inline path regex copies",
);

assert.match(
  ciWorkflow,
  /TSZ_CI_EMERGENCY_SCALE_DOWN: "1"[\s\S]+?GitHub Actions-only cost-control mode is active[\s\S]+?should_run=false[\s\S]+?full_run=false[\s\S]+?compiler_checks_required=false/,
  "CI should force GitHub Actions-only cost-control mode and skip heavy CI fanout",
);
assert.doesNotMatch(
  ciWorkflow,
  /full_ci|TSZ_CI_MANUAL_FULL_CI|inputs\.full_ci/,
  "CI should not expose a manual full-CI escape hatch while GCP spend is shut down",
);
assert.match(
  ciWorkflow,
  /signoff-gate:[\s\S]+name: PR Signoff[\s\S]+context == "signoff"[\s\S]+Run scripts\/ci\/signoff\.sh locally/,
  "PR CI should require the local signoff commit status instead of starting heavy GCP-backed jobs",
);

assert.match(
  gateClassifier,
  /ci-resources\|full-ci\|github-suite\|suite-metadata\|build-dist\|dist\|wasm/,
  "ci-resources.sh changes must require compiler CI because they size dist/unit/wasm jobs",
);

assert.match(
  gateClassifier,
  /\.github\\\/workflows\\\/\(ci\|bench\)\\\.yml/,
  "CI workflow changes should continue to be classified as compiler-sensitive paths",
);

assert.doesNotMatch(
  ciWorkflow,
  /CI run superseded[\s\S]+?sys\.exit\(0\)/,
  "CI Summary must not pass when required heavy jobs were cancelled",
);

assert.doesNotMatch(
  ciWorkflow,
  /Treating as neutral/,
  "CI Summary must not treat cancelled required jobs as a neutral protected check",
);

// The project-compile-canary suite is advisory (continue-on-error + ALLOW_FAILURES)
// and now installs dependencies for ~20 real applications, so it runs for tens of
// minutes and queues on a busy pool. It must NOT be a dependency of, or required by,
// the CI Summary gate — otherwise the required CI Summary stalls "expected — waiting
// for status to be reported" on a suite that never gates correctness. It still runs
// and records its compatibility results out of band.
assert.doesNotMatch(
  ciWorkflow,
  /^\s+- project-compile-canary-aggregate\s*$/m,
  "CI Summary must NOT list project-compile-canary-aggregate in needs: the advisory canary must not block/delay the required gate",
);

for (const job of ["lint", "cargo-shear", "cargo-deny"]) {
  assert.match(
    ciWorkflow,
    new RegExp(`\\n\\s{2}${job}:\\n[\\s\\S]+?runs-on: ubuntu-latest`),
    `${job} should run on hosted Ubuntu so cheap gates do not wait on external runners`,
  );
}

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
  /TSZ_CI_EMERGENCY_SCALE_DOWN: "0"|manual full CI|manual_full_ci/,
  "CI should not contain a documented switch that re-enables GCP-backed heavy jobs",
);

assert.match(
  ciWorkflow,
  /\n\s{2}ci-summary:\n[\s\S]+?needs:[\s\S]+?- signoff-gate[\s\S]+?PR Signoff did not pass/,
  "CI Summary should include PR Signoff so missing local testing fails the required summary",
);

assert.match(
  ciWorkflow,
  /\n\s{2}ci-summary:\n[\s\S]+?needs:[\s\S]+?- emit-aggregate\s*\n\s+- fourslash-aggregate[\s\S]+?"emit-aggregate",\s*\n\s+"fourslash-aggregate",/,
  "CI Summary must require the emit-aggregate recombiner (not a raw emit shard) as the required emit leaf before reporting required full-run success",
);

assert.doesNotMatch(
  ciWorkflow,
  /\n\s{2}ci-summary:\n[\s\S]+?required\.update\(\{[\s\S]*?"emit",/,
  "CI Summary must not require a raw emit shard leaf; the sharded emit suite is gated through emit-aggregate",
);
