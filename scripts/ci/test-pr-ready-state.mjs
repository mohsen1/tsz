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
  gateClassifier,
  /ci-resources\|gcp-full-ci\|github-suite\|gcp-cache\|suite-metadata\|build-dist\|dist\|wasm/,
  "ci-resources.sh changes must require compiler CI because they size dist/unit/wasm jobs",
);

assert.match(
  gateClassifier,
  /\.github\\\/workflows\\\/\(ci\|bench\)\\\.yml/,
  "CI workflow changes must require compiler CI because they route native merge queue and Cloud Run jobs",
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

assert.match(
  ciWorkflow,
  /\n\s{2}ci-summary:\n[\s\S]+?needs:[\s\S]+?- project-compile-guard\s*\n\s+- project-compile-canary[\s\S]+?"project-compile-guard",\s*\n\s+"project-compile-canary",/,
  "CI Summary must wait for the project compile canary before reporting required full-run success",
);

for (const job of ["lint", "cargo-shear", "cargo-deny"]) {
  assert.match(
    ciWorkflow,
    new RegExp(`\\n\\s{2}${job}:\\n[\\s\\S]+?runs-on: ubuntu-latest`),
    `${job} should run on hosted Ubuntu so cheap gates are not blocked by the self-hosted pool`,
  );
}

assert.match(
  ciWorkflow,
  /\n\s{2}unit:\n[\s\S]+?runs-on: \[self-hosted, tsz-cloud-run\][\s\S]+?TSZ_CI_UNIT_SKIP_CHECKER_INTEGRATION: "1"[\s\S]+?Run unit suite on Cloud Run runner[\s\S]+?scripts\/ci\/github-suite\.sh unit/,
  "unit should run the Cloud Run-safe unit slice directly on the Cloud Run runner",
);

assert.doesNotMatch(
  ciWorkflow,
  /\n\s{2}unit:\n[\s\S]+?gcloud builds submit[\s\S]+?cloudbuild-unit\.yaml/,
  "unit should not submit to Cloud Build",
);

assert.match(
  ciWorkflow,
  /\n\s{2}unit-checker-integration:\n[\s\S]+?Submit checker integration suite to Cloud Build pool[\s\S]+?--config=scripts\/cloudbuild\/cloudbuild-checker-integration\.yaml/,
  "checker integration linking should stay on Cloud Build as the heavy unit exception",
);

assert.match(
  ciWorkflow,
  /\n\s{2}unit-cloudbuild:\n[\s\S]+?needs: \[gate, unit-checker-integration\][\s\S]+?UNIT_RESULT: \$\{\{ needs\.unit-checker-integration\.result \}\}[\s\S]+?Required Cloud Build checker integration job did not pass/,
  "legacy unit-cloudbuild context should mirror the required checker integration Cloud Build job",
);

assert.match(
  ciWorkflow,
  /\n\s{2}ci-summary:\n[\s\S]+?needs:[\s\S]+?- unit\s*\n\s+- unit-checker-integration[\s\S]+?required\.update\(\{"dist-binaries", "unit", "unit-checker-integration"\}\)/,
  "CI Summary should require both the Cloud Run unit slice and checker integration heavy slice",
);
