#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { fixtureStubEvidenceFor } from "./lib/fixture-stub-inventory.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const PREREQS = path.join(SCRIPT_DIR, "lib", "bench-vs-tsgo-prereqs.sh");
const EVIDENCE = path.join(SCRIPT_DIR, "lib", "project-evidence.sh");
const RESULTS = path.join(SCRIPT_DIR, "lib", "bench-vs-tsgo-results.sh");

function quote(value) {
  return `'${String(value).replaceAll("'", `'\\''`)}'`;
}

function writeExecutable(file, contents) {
  fs.writeFileSync(file, contents, { mode: 0o755 });
}

function fakeCompilers(dir, source) {
  const bin = path.join(dir, "bin");
  fs.mkdirSync(bin, { recursive: true });
  const tsc = path.join(bin, "tsc");
  const tsz = path.join(bin, "tsz");
  const tsgo = path.join(bin, "tsgo");
  const hyperfine = path.join(bin, "hyperfine");

  writeExecutable(tsc, `#!/usr/bin/env bash
set -u
for arg in "$@"; do
  if [[ "$arg" == "--showConfig" ]]; then
    printf '{"files":["./src/a.ts"]}\\n'
    exit "\${FAKE_TSC_SHOW_RC:-0}"
  fi
  if [[ "$arg" == "--listFilesOnly" ]]; then
    printf '%s\\n' "$FAKE_SOURCE"
    exit "\${FAKE_TSC_LIST_RC:-0}"
  fi
done
printf '%s' "\${FAKE_TSC_OUTPUT:-}"
exit "\${FAKE_TSC_RC:-0}"
`);

  writeExecutable(tsz, `#!/usr/bin/env bash
set -u
stats=""
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "--perf-counters-json" ]]; then
    stats="$2"
    shift 2
  else
    shift
  fi
done
case "\${FAKE_TSZ_CASE:-exact}" in
  missing) ;;
  malformed) printf '{' > "$stats" ;;
  legacy) printf '{"schema_version":1,"stats":{"files":1}}' > "$stats" ;;
  missing-completion)
    printf '{"schema_version":2,"stats":{"root_files":1,"source_files":1,"root_file_paths":["%s"],"source_file_paths":["%s"]}}' \
      "$FAKE_SOURCE" "$FAKE_SOURCE" > "$stats"
    ;;
  deferred)
    printf '{"schema_version":2,"stats":{"semantic_completion":"deferred","root_files":1,"source_files":1,"root_file_paths":["%s"],"source_file_paths":["%s"]}}' \
      "$FAKE_SOURCE" "$FAKE_SOURCE" > "$stats"
    ;;
  zero)
    printf '{"schema_version":2,"stats":{"semantic_completion":"complete","root_files":0,"source_files":0,"root_file_paths":[],"source_file_paths":[]}}' > "$stats"
    ;;
  wrong-paths)
    printf '{"schema_version":2,"stats":{"semantic_completion":"complete","root_files":1,"source_files":1,"root_file_paths":["%s"],"source_file_paths":["%s"]}}' \
      "$FAKE_WRONG_SOURCE" "$FAKE_WRONG_SOURCE" > "$stats"
    ;;
  exact)
    printf '{"schema_version":2,"stats":{"semantic_completion":"complete","root_files":1,"source_files":1,"root_file_paths":["%s"],"source_file_paths":["%s"]}}' \
      "$FAKE_SOURCE" "$FAKE_SOURCE" > "$stats"
    ;;
esac
printf '%s' "\${FAKE_TSZ_OUTPUT:-}"
exit "\${FAKE_TSZ_RC:-0}"
`);

  writeExecutable(tsgo, `#!/usr/bin/env bash
exit "\${FAKE_TSGO_RC:-0}"
`);

  writeExecutable(hyperfine, `#!/usr/bin/env bash
set -u
out=""
while [[ $# -gt 0 ]]; do
  if [[ "$1" == "--export-json" ]]; then
    out="$2"
    shift 2
  else
    shift
  fi
done
printf 'called\\n' >> "$FAKE_HYPERFINE_MARKER"
printf '{"results":[{"command":"tsz","mean":0.01,"exit_codes":[0]},{"command":"tsgo","mean":0.02,"exit_codes":[0]}]}' > "$out"
printf 'fake timing complete\\n'
`);

  return { bin, tsc, tsz, tsgo, hyperfine, source };
}

function runCase({
  caseName,
  rowName = "evidence-project",
  tszBinary = "fake",
  tszCase = "exact",
  tscOutput = "",
  tszOutput = "",
  tscRc = 0,
  tszRc = 0,
  tsgoRc = 0,
  exportArtifact = false,
}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `tsz-bench-evidence-${caseName}-`));
  const fixture = path.join(dir, "external", "fixture");
  const source = path.join(fixture, "src", "a.ts");
  const wrongSource = path.join(fixture, "src", "other.ts");
  const tsconfig = path.join(fixture, "tsconfig.json");
  const libDir = path.join(dir, "tsc-lib");
  const compat = path.join(dir, "compat.jsonl");
  const csv = path.join(dir, "result.csv");
  const marker = path.join(dir, "hyperfine.called");
  const artifactFile = path.join(dir, "bench.json");
  fs.mkdirSync(path.dirname(source), { recursive: true });
  fs.mkdirSync(libDir, { recursive: true });
  fs.writeFileSync(source, "export const value = 1;\n");
  fs.writeFileSync(tsconfig, '{"files":["src/a.ts"]}\n');
  const fake = fakeCompilers(dir, source);
  const selectedTsz = tszBinary === "true" ? "/usr/bin/true" : fake.tsz;

  const shell = `
set -uo pipefail
PROJECT_ROOT=${quote(ROOT)}
SCRIPT_DIR=${quote(SCRIPT_DIR)}
BENCH_TARGET_DIR=${quote(path.join(dir, "target"))}
TEMP_DIR=${quote(dir)}
EXTERNAL_BENCH_DIR=${quote(path.join(dir, "external"))}
source ${quote(PREREQS)}
source ${quote(EVIDENCE)}
source ${quote(RESULTS)}
run_with_timeout() { local ignored_timeout="$1"; shift; "$@"; }
project_tsconfig_stats() { printf '0 0 0\\n'; }
tsz_project_fixture_sources() { :; }
tsz_project_owner_families_json() { printf '{"evidence-project":"test","large-ts-repo":"test"}'; }
tsz_project_readme_candidates_json() { printf '{}'; }
PROJECT_EVIDENCE_TSC_BUILTIN_LIB_DIR=${quote(libDir)}
PROJECT_COMPATIBILITY_JSONL=${quote(compat)}
RESULTS_CSV=""
BENCHMARKS_RUN=0
FILTER=""
QUICK_MODE=true
TSZ_IS_OVERRIDE=true
BENCH_PGO_MARKER=""
BENCH_PGO_TSZ_TIMEOUT=0
JSON_OUTPUT=true
JSON_FILE=${quote(artifactFile)}
JSON_EXPORTED=false
BENCHMARK_SOURCES_JSONL=""
LARGE_TS_DIR=""; NEXTJS_DIR=""; NEXT_APP_BENCH_DIR=""; VITE_APP_BENCH_DIR=""
RXJS_DIR=""; TYPE_FEST_DIR=""; ZOD_DIR=""; UTILITY_TYPES_DIR=""
TS_TOOLBELT_DIR=""; TS_ESSENTIALS_DIR=""
WARMUP=0
MIN_RUNS=1
MAX_RUNS=1
BENCH_TIMEOUT=2
BENCH_COLD=0
LARGE_TS_NODE_OPTIONS=""
TSZ_LIB_DIR=""
TSZ_RUST_MIN_STACK=""
TSZ_USE_EMBEDDED_LIBS=1
TSZ=${quote(selectedTsz)}
TSGO=${quote(fake.tsgo)}
TSC=${quote(fake.tsc)}
BENCH_TIMEOUT_RUNNER=/usr/bin/true
RED=""; GREEN=""; YELLOW=""; BLUE=""; CYAN=""; BOLD=""; NC=""
run_project_benchmark ${quote(rowName)} ${quote(tsconfig)} ${quote(path.dirname(source))}
printf '%b' "$RESULTS_CSV" > ${quote(csv)}
if [ ${quote(exportArtifact ? "1" : "0")} = 1 ]; then export_results_json; fi
`;
  const result = spawnSync("bash", ["-c", shell], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fake.bin}:${process.env.PATH}`,
      FAKE_SOURCE: source,
      FAKE_WRONG_SOURCE: wrongSource,
      FAKE_TSZ_CASE: tszCase,
      FAKE_TSC_OUTPUT: tscOutput.replaceAll("SRC", source),
      FAKE_TSZ_OUTPUT: tszOutput.replaceAll("SRC", source),
      FAKE_TSC_RC: String(tscRc),
      FAKE_TSZ_RC: String(tszRc),
      FAKE_TSGO_RC: String(tsgoRc),
      FAKE_HYPERFINE_MARKER: marker,
    },
  });
  const rows = fs.existsSync(compat)
    ? fs.readFileSync(compat, "utf8").trim().split(/\r?\n/).filter(Boolean).map(JSON.parse)
    : [];
  const output = fs.existsSync(csv) ? fs.readFileSync(csv, "utf8") : "";
  const artifact = fs.existsSync(artifactFile)
    ? JSON.parse(fs.readFileSync(artifactFile, "utf8"))
    : null;
  return {
    dir,
    result,
    row: rows.at(-1) ?? null,
    output,
    artifact,
    hyperfineCalled: fs.existsSync(marker),
  };
}

function finish(run) {
  fs.rmSync(run.dir, { recursive: true, force: true });
}

const emptyFingerprint = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

for (const [caseName, config] of [
  ["true-no-stats", { tszBinary: "true", missingReason: "compiler stats missing" }],
  ["missing-stats", { tszCase: "missing", missingReason: "compiler stats missing" }],
  ["malformed-stats", { tszCase: "malformed", missingReason: "compiler stats malformed" }],
  ["legacy-stats", { tszCase: "legacy", missingReason: "compiler stats malformed" }],
  ["missing-completion", { tszCase: "missing-completion", missingReason: "compiler stats malformed" }],
  ["deferred-completion", { tszCase: "deferred", missingReason: "compiler stats malformed" }],
  ["zero-stats", { tszCase: "zero" }],
  ["equal-count-wrong-paths", { tszCase: "wrong-paths" }],
]) {
  const run = runCase({ caseName, ...config });
  try {
    assert.equal(run.result.status, 0, `${caseName}: ${run.result.stderr}`);
    assert.equal(run.hyperfineCalled, false, `${caseName}: hyperfine must be unreachable`);
    assert.ok(run.row, `${caseName}: compatibility row must remain visible`);
    assert.equal(run.row.evidence_schema, null, `${caseName}: invalid proof cannot claim schema 2`);
    assert.notEqual(run.row.state, "green", `${caseName}: invalid proof cannot be green`);
    assert.match(run.output, /project evidence unavailable/);
    if (config.missingReason) {
      assert.equal(run.row.files_reached, null, `${caseName}: missing proof is not a zero-file proof`);
      assert.equal(run.row.files_reached_reason, config.missingReason);
    }
  } finally {
    finish(run);
  }
}
console.log("project evidence rejects true/missing/malformed/legacy/zero/wrong-path proofs");

for (const rowName of ["msw-project", "effect-project", "drizzle-orm-project"]) {
  const expected = fixtureStubEvidenceFor(ROOT, rowName);
  const run = runCase({ caseName: `stubbed-${rowName}`, rowName });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, false, `${rowName}: stubs make timing unreachable`);
    assert.equal(run.row.state, "gray");
    assert.equal(run.row.evidence_schema, null);
    assert.equal(run.row.stub_inventory_schema, 1);
    assert.equal(run.row.stubbed_modules, expected.stubbedModules);
    assert.equal(run.row.stubbed_any_members, expected.stubbedAnyMembers);
    assert.equal(run.row.stub_inventory_fingerprint, expected.stubInventoryFingerprint);
    assert.ok(run.row.stubbed_modules > 0 || run.row.stubbed_any_members > 0);
    assert.match(run.output, /fixture dependency stubs erase semantic coverage/);
  } finally {
    finish(run);
  }
}
console.log("project evidence rejects source-derived msw/effect/drizzle stub inventories");

{
  const sourceToken = "SRC";
  const first = `${sourceToken}(1,1): error TS2322: first mismatch.\n`;
  const second = `${sourceToken}(2,1): error TS2345: second mismatch.\n`;
  const run = runCase({
    caseName: "diagnostic-subset",
    tscOutput: first + second,
    tszOutput: first,
    tscRc: 1,
    tszRc: 1,
  });
  try {
    // Normalize the placeholder to each case's actual source path before the
    // fake compiler sees it by checking that even two same-exit diagnostic
    // multisets of different multiplicity are rejected.
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, false);
    assert.equal(run.row.evidence_schema, null);
    assert.equal(run.row.state, "yellow");
  } finally {
    finish(run);
  }
}

{
  const primary = "SRC(1,1): error TS2322: same primary.\n";
  const run = runCase({
    caseName: "continuation-owner-mismatch",
    tscOutput: `${primary}  Type 'left' is not assignable.\n`,
    tszOutput: `${primary}  Type 'right' is not assignable.\n`,
    tscRc: 1,
    tszRc: 1,
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, false);
    assert.equal(run.row.evidence_schema, null);
    assert.equal(run.row.state, "yellow");
  } finally {
    finish(run);
  }
}

{
  const run = runCase({
    caseName: "unparsed-output",
    tszOutput: "compiler said success but emitted an unknown banner\n",
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, false);
    assert.equal(run.row.state, "gray");
    assert.match(run.row.diagnostic_status, /evidence unavailable/);
  } finally {
    finish(run);
  }
}

{
  const diagnostic = "src/a.ts(1,1): error TS2322: exact same diagnostic.\n";
  const run = runCase({
    caseName: "exit-mismatch",
    tscOutput: diagnostic,
    tszOutput: diagnostic,
    tscRc: 1,
    tszRc: 2,
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, false);
    assert.equal(run.row.evidence_schema, null);
    assert.equal(run.row.state, "yellow");
    assert.match(run.row.diagnostic_status, /exit mismatch/);
  } finally {
    finish(run);
  }
}

{
  const run = runCase({ caseName: "exact-pass", exportArtifact: true });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, true, "exact proof is the only path to hyperfine");
    assert.equal(run.row.state, "green");
    assert.equal(run.row.evidence_schema, 2);
    assert.equal(run.row.semantic_completion, "complete");
    assert.equal(run.row.root_files, 1);
    assert.equal(run.row.source_files, 1);
    assert.equal(run.row.files_reached, 1, "files_reached comes from TSZ, not fixture display count 0");
    assert.equal(run.row.root_file_fingerprint, run.row.oracle_root_file_fingerprint);
    assert.equal(run.row.source_file_fingerprint, run.row.oracle_source_file_fingerprint);
    assert.equal(run.row.diagnostic_records, 0);
    assert.equal(run.row.diagnostic_fingerprint, emptyFingerprint);
    assert.equal(run.row.diagnostic_fingerprint, run.row.oracle_diagnostic_fingerprint);
    assert.equal(run.row.oracle_classification, "both-pass");
    assert.equal(run.row.stub_inventory_schema, 1);
    assert.equal(run.row.stubbed_modules, 0);
    assert.equal(run.row.stubbed_any_members, 0);
    assert.match(run.row.stub_inventory_fingerprint, /^[0-9a-f]{64}$/);
    assert.match(run.output, /evidence-project,0,0,[0-9.]+,[0-9.]+/);
    const published = run.artifact?.results?.[0]?.compatibility;
    assert.ok(published, "final benchmark artifact retains compatibility evidence");
    assert.equal(published.evidence_schema, 2);
    assert.equal(published.semantic_completion, "complete");
    assert.equal(published.files_reached, 1);
    assert.equal(published.root_file_fingerprint, published.oracle_root_file_fingerprint);
    assert.equal(published.diagnostic_fingerprint, published.oracle_diagnostic_fingerprint);
    assert.equal(published.stub_inventory_schema, 1);
    assert.equal(published.stubbed_modules, 0);
    assert.equal(published.stubbed_any_members, 0);
    assert.equal(published.stub_inventory_fingerprint, run.row.stub_inventory_fingerprint);
    assert.equal(run.artifact.totals.green_tsz_wins, 1);
  } finally {
    finish(run);
  }
}

{
  const diagnostic = "src/a.ts(1,1): error TS2322: exact same diagnostic.\n  The nested reason also agrees.\n";
  const run = runCase({
    caseName: "exact-nonzero",
    tscOutput: diagnostic,
    tszOutput: diagnostic,
    tscRc: 1,
    tszRc: 1,
    tsgoRc: 1,
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, false, "nonzero parity is green but not timing-eligible");
    assert.equal(run.row.state, "green");
    assert.equal(run.row.evidence_schema, 2);
    assert.equal(run.row.diagnostic_records, 1);
    assert.equal(run.row.diagnostic_fingerprint, run.row.oracle_diagnostic_fingerprint);
    assert.equal(run.row.oracle_classification, "both-fail-same");
  } finally {
    finish(run);
  }
}

{
  const run = runCase({
    caseName: "large-no-proof",
    rowName: "large-ts-repo",
    tszBinary: "true",
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    assert.equal(run.hyperfineCalled, false, "large-ts-repo has no proof bypass");
    assert.equal(run.row.state, "gray");
    assert.equal(run.row.evidence_schema, null);
  } finally {
    finish(run);
  }
}

// The library-level fail-fast guards must stop before attempting mktemp or a
// compiler launch. These are separate from `/usr/bin/true`, which exercises a
// present command that omits its required stats output.
{
  const shell = `
set -uo pipefail
PROJECT_ROOT=${quote(ROOT)}
BENCH_TIMEOUT=1
source ${quote(path.join(ROOT, "scripts", "ci", "lib", "project-compile-fingerprint.sh"))}
source ${quote(path.join(ROOT, "scripts", "ci", "lib", "project-tsc-oracle.sh"))}
source ${quote(EVIDENCE)}
run_with_timeout() { return 99; }
unset PROJECT_EVIDENCE_TSZ_CMD PROJECT_EVIDENCE_TSC_CMD
collect_project_evidence guard /missing /missing /missing /missing || printf '%s\n' "$PROJECT_EVIDENCE_REASON"
PROJECT_EVIDENCE_TSZ_CMD=()
PROJECT_EVIDENCE_TSC_CMD=()
collect_project_evidence guard /missing /missing /missing /missing || printf '%s\\n' "$PROJECT_EVIDENCE_REASON"
PROJECT_EVIDENCE_TSZ_CMD=(/usr/bin/true)
PROJECT_EVIDENCE_TSC_CMD=(/usr/bin/true)
PROJECT_EVIDENCE_STATS_READER=/missing/reader
collect_project_evidence guard /missing /missing /missing /missing || printf '%s\\n' "$PROJECT_EVIDENCE_REASON"
PROJECT_EVIDENCE_STATS_READER=${quote(path.join(ROOT, "scripts", "ci", "project-compile-stats.mjs"))}
PROJECT_EVIDENCE_STUB_INVENTORY_READER=/missing/stub-inventory
collect_project_evidence guard /missing /missing /missing /missing || printf '%s\\n' "$PROJECT_EVIDENCE_REASON"
`;
  const result = spawnSync("bash", ["-c", shell], { cwd: ROOT, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(result.stdout.trim().split(/\r?\n/), [
    "compiler command unavailable",
    "compiler command unavailable",
    "project stats reader unavailable",
    "fixture stub inventory reader unavailable",
  ]);
}

// A present reader that cannot produce source-derived row evidence is just as
// non-authoritative as a missing reader. Reject it before either compiler is
// launched, rather than treating an unreadable inventory as zero stubs.
{
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-stub-reader-guard-"));
  try {
    const malformedReader = path.join(dir, "malformed-stub-reader.mjs");
    fs.writeFileSync(
      malformedReader,
      'process.stdout.write("1\\t0\\t0\\tnot-a-fingerprint\\n");\n',
    );
    const shell = `
set -uo pipefail
PROJECT_ROOT=${quote(ROOT)}
BENCH_TIMEOUT=1
source ${quote(path.join(ROOT, "scripts", "ci", "lib", "project-compile-fingerprint.sh"))}
source ${quote(path.join(ROOT, "scripts", "ci", "lib", "project-tsc-oracle.sh"))}
source ${quote(EVIDENCE)}
run_with_timeout() { printf 'compiler launched\\n'; return 99; }
PROJECT_EVIDENCE_TSZ_CMD=(/usr/bin/true)
PROJECT_EVIDENCE_TSC_CMD=(/usr/bin/true)
PROJECT_EVIDENCE_STUB_INVENTORY_READER=${quote(malformedReader)}
collect_project_evidence guard /missing /missing /missing /missing || printf '%s\\n' "$PROJECT_EVIDENCE_REASON"
`;
    const result = spawnSync("bash", ["-c", shell], { cwd: ROOT, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(result.stdout.trim(), "fixture stub inventory malformed");
    assert.doesNotMatch(result.stdout, /compiler launched/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

console.log("bench-vs-tsgo project evidence tests passed");
