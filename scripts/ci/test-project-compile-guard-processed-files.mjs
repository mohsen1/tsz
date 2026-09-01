#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const GUARD = path.join(ROOT, "scripts", "ci", "project-compile-guard.sh");

function writeFile(file, text, mode = 0o644) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text, { encoding: "utf8", mode });
  if (mode & 0o111) fs.chmodSync(file, mode);
}

function git(cwd, args) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}

function createFixtureRepo(dir, { importedError = false } = {}) {
  writeFile(
    path.join(dir, "src", "index.ts"),
    importedError
      ? 'import "./dep.spec";\nexport type Identity<T> = T;\n'
      : "export type Identity<T> = T;\n",
  );
  if (importedError) {
    // The guard config excludes *.spec.ts from roots, but TypeScript follows
    // this import into the source graph and reports the error in the dependency.
    writeFile(path.join(dir, "src", "dep.spec.ts"), 'const value: string = 1;\n');
  }
  git(dir, ["init", "--quiet"]);
  git(dir, ["add", "."]);
  git(dir, [
    "-c", "user.name=harness-test",
    "-c", "user.email=harness-test@example.invalid",
    "commit", "--quiet", "-m", "fixture",
  ]);
  return git(dir, ["rev-parse", "HEAD"]);
}

function fakeCompilerScript() {
  return `#!/usr/bin/env bash
set -euo pipefail
stats_file=""
while [[ "$#" -gt 0 ]]; do
  if [[ "$1" == "--perf-counters-json" ]]; then
    stats_file="$2"
    shift 2
    continue
  fi
  shift
done
if [[ -z "$stats_file" ]]; then
  echo "missing --perf-counters-json" >&2
  exit 90
fi
count=0
[[ -f "$FAKE_RUN_COUNT" ]] && count="$(<"$FAKE_RUN_COUNT")"
printf '%s' "$((count + 1))" > "$FAKE_RUN_COUNT"
write_stats() {
  node -e '
    const fs = require("node:fs");
    const root = process.env.FAKE_PROJECT_ROOT;
    const mode = process.env.FAKE_STATS_MODE;
    const semanticCompletion = ["deferred", "cycle", "limit"].includes(mode) ? mode : "complete";
    let roots = [root + "/src/index.ts", root + "/src/second.ts"];
    let sources = [
      root + "/src/index.ts", root + "/src/second.ts", root + "/src/three.ts",
      root + "/src/four.ts", root + "/src/five.ts", root + "/src/six.ts",
      root + "/src/seven.ts",
    ];
    if (mode === "zero") { roots = []; sources = []; }
    if (mode === "root-mismatch") roots.push(root + "/src/third-root.ts");
    if (mode === "source-mismatch") sources.pop();
    if (mode === "root-path-mismatch") roots[1] = root + "/src/wrong-root.ts";
    if (mode === "source-path-mismatch") sources[6] = root + "/src/wrong-source.ts";
    if (mode === "false-negative-import") {
      roots = [root + "/src/index.ts"];
      sources = [root + "/src/index.ts"];
    }
    fs.writeFileSync(process.env.FAKE_STATS_FILE, JSON.stringify({
      schema_version: 2,
      stats: {
        semantic_completion: semanticCompletion,
        root_files: roots.length,
        source_files: sources.length,
        files: sources.length,
        root_file_paths: roots,
        source_file_paths: sources,
      },
    }) + "\\n");
  '
}
case "$FAKE_STATS_MODE" in
  zero)
    FAKE_STATS_FILE="$stats_file" write_stats
    echo 'error TS18003: No inputs were found in config file.'
    exit 1
    ;;
  missing)
    exit 0
    ;;
  malformed)
    # The files field is an alias only; it may not substitute for the canonical pair.
    printf '%s\n' '{"schema_version":2,"stats":{"files":7}}' > "$stats_file"
    exit 0
    ;;
  malformed-paths)
    printf '%s\n' '{"schema_version":2,"stats":{"root_files":1,"source_files":1,"files":1,"root_file_paths":[],"source_file_paths":[]}}' > "$stats_file"
    exit 0
    ;;
  legacy-schema)
    printf '%s\n' '{"schema_version":1,"stats":{"root_files":0,"source_files":0,"files":0,"root_file_paths":[],"source_file_paths":[]}}' > "$stats_file"
    exit 0
    ;;
  deferred|cycle|limit)
    FAKE_STATS_FILE="$stats_file" write_stats
    exit 3
    ;;
  positive|root-mismatch|source-mismatch|root-path-mismatch|source-path-mismatch|oracle-timeout|source-oracle-timeout|graph-input-mutation|oracle-input-mutation)
    FAKE_STATS_FILE="$stats_file" write_stats
    exit 0
    ;;
  input-mutation)
    FAKE_STATS_FILE="$stats_file" write_stats
    printf '\n// changed while compiler result was being produced\n' >> "$FAKE_PROJECT_ROOT/src/index.ts"
    exit 0
    ;;
  diagnostic-parity)
    FAKE_STATS_FILE="$stats_file" write_stats
    echo 'error TS18003: No inputs were found in config file.'
    exit 1
    ;;
  exit-code-mismatch)
    FAKE_STATS_FILE="$stats_file" write_stats
    echo 'error TS18003: No inputs were found in config file.'
    exit 1
    ;;
  diagnostic-subset)
    FAKE_STATS_FILE="$stats_file" write_stats
    echo 'a/index.ts(1,1): error TS2322: shared diagnostic'
    exit 1
    ;;
  swapped-continuations)
    FAKE_STATS_FILE="$stats_file" write_stats
    echo 'a/index.ts(1,1): error TS2322: first diagnostic'
    echo '  reason owned by a'
    echo 'b/index.ts(2,1): error TS2345: second diagnostic'
    echo '  reason owned by b'
    exit 1
    ;;
  success-diagnostic-mismatch)
    FAKE_STATS_FILE="$stats_file" write_stats
    echo 'src/index.ts(1,1): warning TS6133: value is declared but never read.'
    exit 0
    ;;
  false-negative-import)
    FAKE_STATS_FILE="$stats_file" write_stats
    exit 0
    ;;
  unparsed-diagnostic)
    FAKE_STATS_FILE="$stats_file" write_stats
    echo 'tsz unknown diagnostic shape'
    exit 1
    ;;
  blank-nonzero)
    FAKE_STATS_FILE="$stats_file" write_stats
    exit 1
    ;;
  *)
    exit 91
    ;;
esac
`;
}

function readOnlyRow(fixtureRoot) {
  const file = path.join(fixtureRoot, "project-compatibility.jsonl");
  const rows = fs.readFileSync(file, "utf8")
    .trim()
    .split(/\r?\n/)
    .filter(Boolean)
    .map(JSON.parse);
  assert.equal(rows.length, 1);
  return rows[0];
}

function runCase(
  mode,
  {
    repeat = false,
    oracle = true,
    allowFailure,
    cacheExpected = true,
    resultCache = true,
    tamperOracleLog = false,
  } = {},
) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `tsz-processed-${mode}-`));
  try {
    const fixtureRepo = path.join(dir, "fixture-repo");
    const fixtureRoot = path.join(dir, "guard-fixtures");
    const compiler = path.join(dir, "fake-tsz");
    const oracleCompiler = path.join(dir, "fake-tsc");
    const oracleLibDir = path.join(dir, "fake-typescript-lib");
    const runCount = path.join(dir, "run-count");
    const oracleRunCount = path.join(dir, "oracle-run-count");
    const ref = createFixtureRepo(fixtureRepo, {
      importedError: mode === "false-negative-import",
    });
    writeFile(compiler, fakeCompilerScript(), 0o755);
    fs.mkdirSync(oracleLibDir, { recursive: true });
    writeFile(
      path.join(oracleLibDir, "lib.es2022.d.ts"),
      "interface Array<T> { readonly length: number; }\n",
    );
    writeFile(
      oracleCompiler,
      `#!/usr/bin/env bash
if [[ " $* " == *" --noLib "* ]]; then
  echo "error TS5053: Option 'lib' cannot be specified with option 'noLib'."
  exit 1
fi
if [[ " $* " == *" --showConfig "* ]]; then
  if [[ "$FAKE_STATS_MODE" == "false-negative-import" ]]; then
    printf '%s\n' '{"files":["./src/index.ts"]}'
  else
    printf '%s\n' '{"files":["./src/index.ts","./src/second.ts"]}'
  fi
  exit 0
fi
if [[ " $* " == *" --listFilesOnly "* ]]; then
  if [[ "$FAKE_STATS_MODE" == "source-oracle-timeout" ]]; then
    exit 124
  fi
  printf '%s\n' "$FAKE_ORACLE_LIB_DIR/lib.es2022.d.ts"
  if [[ "$FAKE_STATS_MODE" == "false-negative-import" ]]; then
    printf '%s\n' "$FAKE_PROJECT_ROOT/src/index.ts" "$FAKE_PROJECT_ROOT/src/dep.spec.ts"
  else
    printf '%s\n' \
      "$FAKE_PROJECT_ROOT/src/index.ts" "$FAKE_PROJECT_ROOT/src/second.ts" \
      "$FAKE_PROJECT_ROOT/src/three.ts" "$FAKE_PROJECT_ROOT/src/four.ts" \
      "$FAKE_PROJECT_ROOT/src/five.ts" "$FAKE_PROJECT_ROOT/src/six.ts" \
      "$FAKE_PROJECT_ROOT/src/seven.ts"
  fi
  if [[ "$FAKE_STATS_MODE" == "graph-input-mutation" ]]; then
    printf '\n// changed while graph evidence was being produced\n' >> "$FAKE_PROJECT_ROOT/src/index.ts"
  fi
  exit 0
fi
count=0
[[ -f "$FAKE_ORACLE_RUN_COUNT" ]] && count="$(<"$FAKE_ORACLE_RUN_COUNT")"
printf '%s' "$((count + 1))" > "$FAKE_ORACLE_RUN_COUNT"
case "$FAKE_STATS_MODE" in
  diagnostic-parity)
    echo 'error TS18003: No inputs were found in config file.'
    exit 1
    ;;
  exit-code-mismatch)
    echo 'error TS18003: No inputs were found in config file.'
    exit 2
    ;;
  diagnostic-subset)
    echo 'a/index.ts(1,1): error TS2322: shared diagnostic'
    echo 'b/index.ts(2,1): error TS7006: missing diagnostic'
    exit 1
    ;;
  swapped-continuations)
    echo 'b/index.ts(2,1): error TS2345: second diagnostic'
    echo '  reason owned by a'
    echo 'a/index.ts(1,1): error TS2322: first diagnostic'
    echo '  reason owned by b'
    exit 1
    ;;
  false-negative-import)
    echo 'src/dep.spec.ts(1,7): error TS2322: Type number is not assignable to type string.'
    exit 1
    ;;
  oracle-timeout)
    exit 124
    ;;
  unparsed-diagnostic)
    echo 'tsc other unknown diagnostic shape'
    exit 1
    ;;
  blank-nonzero)
    exit 1
    ;;
  oracle-input-mutation)
    printf '\n// changed while diagnostic oracle evidence was being produced\n' >> "$FAKE_PROJECT_ROOT/src/index.ts"
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
`,
      0o755,
    );

    const env = {
      ...process.env,
      TSZ_BIN: compiler,
      UTILITY_TYPES_REPO: fixtureRepo,
      UTILITY_TYPES_REF: ref,
      TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
      TSZ_PROJECT_COMPILE_SET: "required",
      TSZ_PROJECT_COMPILE_FILTER: "^utility-types-project$",
      TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
      TSZ_PROJECT_COMPILE_RESULT_CACHE: resultCache ? "1" : "0",
      TSZ_PROJECT_COMPILE_TSC_ORACLE_CACHE: "1",
      TSZ_PROJECT_COMPILE_TSC_ORACLE: oracle ? "1" : "0",
      TSZ_PROJECT_TSC_ORACLE_BIN: oracleCompiler,
      TSZ_PROJECT_TSC_ORACLE_BUILTIN_LIB_DIR: oracleLibDir,
      TSZ_PROJECT_COMPILE_ALLOW_FAILURES:
        allowFailure ?? (mode === "positive" || mode === "diagnostic-parity" ? "0" : "1"),
      FAKE_STATS_MODE: mode,
      FAKE_RUN_COUNT: runCount,
      FAKE_ORACLE_RUN_COUNT: oracleRunCount,
      FAKE_ORACLE_LIB_DIR: oracleLibDir,
      FAKE_PROJECT_ROOT: path.join(fixtureRoot, "utility-types"),
    };
    const invoke = () => spawnSync("bash", [GUARD], {
      cwd: ROOT,
      encoding: "utf8",
      env,
    });

    const first = invoke();
    assert.equal(first.status, 0, `${mode}:\n${first.stdout}\n${first.stderr}`);
    const firstRow = readOnlyRow(fixtureRoot);
    const oracleCache = path.join(
      fixtureRoot,
      ".tsc-oracle-cache",
      "utility-types-project",
    );
    if (mode === "graph-input-mutation") {
      assert.equal(
        fs.existsSync(`${oracleCache}.graph-counts`),
        false,
        "moving graph inputs must not publish evidence under the pre-run fingerprint",
      );
    }
    if (mode === "oracle-input-mutation") {
      assert.equal(
        fs.existsSync(oracleCache),
        false,
        "moving diagnostic inputs must not publish evidence under the pre-run fingerprint",
      );
      assert.equal(
        fs.existsSync(`${oracleCache}.log`),
        false,
        "moving diagnostic inputs must not publish an uncommitted oracle log",
      );
    }
    if (!repeat) return { row: firstRow, output: `${first.stdout}\n${first.stderr}` };

    if (tamperOracleLog) {
      const metadata = fs.readFileSync(oracleCache, "utf8");
      assert.match(metadata, /^SCHEMA=2$/m);
      assert.match(metadata, /^LOG_SHA256=[0-9a-f]{64}$/m);
      fs.writeFileSync(`${oracleCache}.log`, "stale log from interrupted publication\n");
    }

    const second = invoke();
    assert.equal(second.status, 0, `${mode} cached:\n${second.stdout}\n${second.stderr}`);
    const cacheFile = path.join(
      fixtureRoot,
      ".result-cache",
      "utility-types-project",
    );
    if (tamperOracleLog) {
      assert.doesNotMatch(second.stdout, /result cache hit/);
      assert.doesNotMatch(second.stdout, /tsc oracle cache hit/);
      assert.equal(fs.readFileSync(runCount, "utf8"), "2");
      assert.equal(fs.readFileSync(oracleRunCount, "utf8"), "2");
      const metadata = fs.readFileSync(oracleCache, "utf8");
      const expectedSha = metadata.match(/^LOG_SHA256=([0-9a-f]{64})$/m)?.[1];
      const actualSha = crypto
        .createHash("sha256")
        .update(fs.readFileSync(`${oracleCache}.log`))
        .digest("hex");
      assert.equal(actualSha, expectedSha, "metadata commits only the matching complete log");
      return {
        firstRow,
        row: readOnlyRow(fixtureRoot),
        cacheText: metadata,
        output: `${second.stdout}\n${second.stderr}`,
      };
    }
    if (!cacheExpected) {
      assert.doesNotMatch(second.stdout, /result cache hit/);
      assert.equal(fs.readFileSync(runCount, "utf8"), "2", "non-evidence must rerun tsz");
      assert.equal(fs.existsSync(cacheFile), false, "non-evidence must not enter result cache");
      if (mode === "unparsed-diagnostic" || mode === "blank-nonzero") {
        assert.match(second.stdout, /tsc oracle cache hit/);
        assert.equal(
          fs.readFileSync(oracleRunCount, "utf8"),
          "1",
          "an ordinary cached oracle exit is reparsed without rerunning tsc",
        );
      }
      if (mode === "oracle-timeout") {
        assert.equal(
          fs.existsSync(path.join(fixtureRoot, ".tsc-oracle-cache", "utility-types-project")),
          false,
          "transient diagnostic-oracle exits must not enter its cache",
        );
      }
      if (mode === "source-oracle-timeout") {
        assert.equal(
          fs.existsSync(path.join(
            fixtureRoot,
            ".tsc-oracle-cache",
            "utility-types-project.graph-counts",
          )),
          false,
          "incomplete source-graph evidence must not enter its cache",
        );
      }
      return {
        firstRow,
        row: readOnlyRow(fixtureRoot),
        cacheText: "",
        output: `${second.stdout}\n${second.stderr}`,
      };
    }
    const secondRow = readOnlyRow(fixtureRoot);
    const cacheText = fs.existsSync(cacheFile) ? fs.readFileSync(cacheFile, "utf8") : "";
    assert.match(
      second.stdout,
      /result cache hit/,
      JSON.stringify({
        firstFingerprint: firstRow.compile_input_fingerprint,
        secondFingerprint: secondRow.compile_input_fingerprint,
        cachedFingerprint: cacheText.match(/^FINGERPRINT=(.*)$/m)?.[1] ?? null,
      }),
    );
    assert.equal(fs.readFileSync(runCount, "utf8"), "1", "cache hit must not rerun tsz");
    return {
      firstRow,
      row: secondRow,
      cacheText,
      output: `${second.stdout}\n${second.stderr}`,
    };
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

{
  const { row } = runCase("positive");
  assert.equal(row.state, "gray", "a fake binary without a build manifest is non-evidence");
  assert.ok(row.evidence_failures.includes("build_manifest_sha256"));
  assert.equal(row.compile_input_stable, true);
  assert.match(row.evidence_protocol_fingerprint, /^[0-9a-f]{64}$/);
  assert.equal(row.files_reached, 7, "source_files, not a directory walk, is recorded");
  assert.equal(row.files_reached_reason, null);
  assert.deepEqual(row.exit_codes.tsc, [0], "green requires a clean pinned oracle run");
}

{
  const { firstRow, row, cacheText } = runCase("input-mutation", {
    repeat: true,
    cacheExpected: false,
  });
  for (const [label, value] of [["fresh", firstRow], ["repeated", row]]) {
    assert.equal(value.state, "gray", `${label}: moving compile inputs are non-evidence`);
    assert.equal(value.compile_input_stable, false);
    assert.ok(value.evidence_failures.includes("compile_input_stable"));
  }
  assert.equal(cacheText, "", "a moving project tree never enters the result cache");
}

for (const mode of ["graph-input-mutation", "oracle-input-mutation"]) {
  const { row } = runCase(mode, { resultCache: false });
  assert.equal(row.state, "gray", `${mode}: moving oracle inputs are non-evidence`);
  assert.equal(row.compile_input_stable, false);
  assert.ok(row.evidence_failures.includes("compile_input_stable"));
}

{
  const { firstRow, row, cacheText } = runCase("zero", { repeat: true });
  assert.equal(firstRow.files_reached_reason, "zero source files processed");
  assert.equal(row.state, "gray");
  assert.equal(row.exit_class, "fixture invalid");
  assert.equal(row.files_reached, null);
  assert.equal(row.files_reached_reason, "zero source files processed");
  assert.deepEqual(row.tsz_diagnostic_codes, ["TS18003"]);
  assert.equal(row.repro.first_failure_code, "TS18003");
  assert.equal(row.repro.first_failure_path, null);
  assert.ok(
    row.diagnostic_deltas.some((line) => line.includes("error TS18003")),
    "pathless TS18003 must remain visible on a zero-file row",
  );
  assert.match(cacheText, /^FILES_REASON=zero source files processed$/m);
}

for (const [mode, reason] of [
  ["missing", "compiler stats missing"],
  ["malformed", "compiler stats malformed"],
  ["malformed-paths", "compiler stats malformed"],
  ["legacy-schema", "compiler stats malformed"],
]) {
  const { row } = runCase(mode);
  assert.equal(row.state, "gray", `${mode} stats are non-evidence`);
  assert.equal(row.exit_class, "runner error");
  assert.equal(row.files_reached, null);
  assert.equal(row.files_reached_reason, reason);
  assert.deepEqual(row.exit_codes.tsz, [0], "the underlying compiler really returned RC0");
}

{
  const { row, cacheText } = runCase("positive", { repeat: true });
  assert.equal(row.files_reached, 7, "cache replay preserves source_files");
  assert.equal(row.files_reached_reason, null);
  assert.match(cacheText, /^ROOT_FILES=2$/m);
  assert.match(cacheText, /^SOURCE_FILES=7$/m);
  assert.match(cacheText, /^ROOT_FINGERPRINT=[0-9a-f]{64}$/m);
  assert.match(cacheText, /^SOURCE_FINGERPRINT=[0-9a-f]{64}$/m);
}

for (const mode of ["missing", "malformed", "malformed-paths", "legacy-schema"]) {
  const { firstRow, row } = runCase(mode, {
    repeat: true,
    cacheExpected: false,
  });
  assert.equal(firstRow.files_reached, null);
  assert.equal(row.files_reached, null);
  assert.equal(
    row.files_reached_reason,
    mode === "missing" ? "compiler stats missing" : "compiler stats malformed",
    `${mode} machine stats stay non-evidence after a fresh rerun`,
  );
}

for (const semanticCompletion of ["deferred", "cycle", "limit"]) {
  const { firstRow, row, output } = runCase(semanticCompletion, {
    repeat: true,
    cacheExpected: false,
  });
  for (const [label, value] of [["fresh", firstRow], ["rerun", row]]) {
    assert.equal(value.state, "red", `${semanticCompletion} ${label}: producer RC3 is a red row`);
    assert.equal(value.exit_class, "nonzero exit");
    assert.equal(value.evidence_schema, null, "typed telemetry is not exact admission proof");
    assert.equal(value.semantic_completion, semanticCompletion);
    assert.equal(value.root_files, 2);
    assert.equal(value.source_files, 7);
    assert.equal(value.files_reached, 7);
    assert.equal(value.files_reached_reason, null);
    assert.match(value.root_file_fingerprint, /^[0-9a-f]{64}$/);
    assert.match(value.source_file_fingerprint, /^[0-9a-f]{64}$/);
    assert.equal(value.oracle_root_files, 2);
    assert.equal(value.oracle_source_files, 7);
    assert.equal(value.root_file_fingerprint, value.oracle_root_file_fingerprint);
    assert.equal(value.source_file_fingerprint, value.oracle_source_file_fingerprint);
    assert.equal(value.diagnostic_status, `semantic completion ${semanticCompletion}`);
    assert.deepEqual(value.exit_codes.tsz, [3]);
  }
  assert.match(output, /result is non-evidence/);
  assert.match(output, /result not cached/);
}

{
  const { row } = runCase("positive", {
    oracle: false,
    allowFailure: "1",
  });
  assert.equal(row.state, "gray");
  assert.equal(row.exit_class, "oracle unavailable");
  assert.equal(row.files_reached, 7);
  assert.match(row.diagnostic_status, /TypeScript 7 evidence unavailable/);
}

for (const mode of ["oracle-timeout", "source-oracle-timeout"]) {
  const { firstRow, row } = runCase(mode, {
    repeat: true,
    cacheExpected: false,
  });
  for (const value of [firstRow, row]) {
    assert.equal(value.state, "gray");
    assert.equal(value.exit_class, "oracle unavailable");
  }
}

{
  const { firstRow, row, cacheText } = runCase("diagnostic-parity", {
    repeat: true,
    oracle: true,
  });
  for (const [label, value] of [["fresh", firstRow], ["cached", row]]) {
    assert.equal(value.state, "gray", `${label}: parity without build provenance remains gray`);
    assert.equal(
      value.oracle_classification,
      "both-fail-same",
      `${label}: matching diagnostic codes survive cache replay`,
    );
    assert.deepEqual(
      value.exit_codes.tsz,
      [1],
      `${label}: a green parity verdict must preserve tsz's real nonzero exit`,
    );
    assert.deepEqual(value.exit_codes.tsc, [1]);
    assert.ok(
      value.diagnostic_deltas.some((line) => line.includes("error TS18003")),
      `${label}: matching pathless diagnostic context survives recording`,
    );
  }
  assert.match(cacheText, /^TSC_SOURCE_FILES=7$/m);
}

for (const [mode, expectedReason] of [
  ["unparsed-diagnostic", "unparsed compiler diagnostic output"],
  ["blank-nonzero", "nonzero compiler exit without parsed diagnostics"],
]) {
  const { firstRow, row } = runCase(mode, {
    repeat: true,
    oracle: true,
    cacheExpected: false,
  });
  for (const [label, value] of [["fresh", firstRow], ["cached oracle", row]]) {
    assert.equal(value.state, "gray", `${label}: unknown output is non-evidence`);
    assert.equal(value.exit_class, "oracle unavailable");
    assert.match(value.diagnostic_status, /TypeScript 7 evidence unavailable/);
    assert.ok(
      value.diagnostic_deltas.some((line) => line.includes(expectedReason)),
      `${label}: the fail-closed reason remains visible`,
    );
  }
}

{
  const { firstRow, row, cacheText } = runCase("diagnostic-parity", {
    repeat: true,
    oracle: true,
    resultCache: false,
    tamperOracleLog: true,
  });
  for (const [label, value] of [["fresh", firstRow], ["after interrupted publication", row]]) {
    assert.equal(value.state, "gray", `${label}: a complete pair without build provenance remains gray`);
    assert.deepEqual(value.exit_codes.tsz, [1]);
    assert.deepEqual(value.exit_codes.tsc, [1]);
  }
  assert.match(cacheText, /^SCHEMA=2$/m);
  assert.match(cacheText, /^LOG_SHA256=[0-9a-f]{64}$/m);
}

{
  const { firstRow, row } = runCase("swapped-continuations", {
    repeat: true,
    oracle: true,
  });
  for (const [label, value] of [["fresh", firstRow], ["cached", row]]) {
    assert.equal(value.state, "yellow", `${label}: reason chains stay bound to primaries`);
    assert.deepEqual(value.exit_codes.tsz, [1]);
    assert.deepEqual(value.exit_codes.tsc, [1]);
    assert.match(value.diagnostic_status, /exact diagnostic mismatch/);
    assert.ok(
      value.diagnostic_deltas.some((line) => line.includes("continuation ownership mismatch")),
      `${label}: continuation ownership divergence remains visible`,
    );
  }
}

{
  const { row } = runCase("root-mismatch", { oracle: true });
  assert.equal(row.state, "yellow", "a compiler root-graph mismatch is parity evidence");
  assert.equal(row.files_reached, 7);
  assert.match(row.diagnostic_status, /root-file diagnostic mismatch/);
  assert.ok(
    row.diagnostic_deltas.some(
      (line) => line.includes("root file count mismatch (tsz=3, TypeScript7=2)"),
    ),
  );
}

for (const [mode, kind, count] of [
  ["root-path-mismatch", "root", 2],
  ["source-path-mismatch", "source", 7],
]) {
  const { firstRow, row, cacheText } = runCase(mode, {
    repeat: true,
    oracle: true,
  });
  for (const [label, value] of [["fresh", firstRow], ["cached", row]]) {
    assert.equal(value.state, "yellow", `${label}: equal-count ${kind} path divergence blocks green`);
    assert.equal(value.files_reached, 7);
    assert.ok(
      value.diagnostic_deltas.some(
        (line) => line.includes(`${kind} path sequence mismatch at equal count ${count}`),
      ),
      `${label}: exact normalized ${kind} graph mismatch remains visible`,
    );
  }
  assert.match(cacheText, /^ROOT_FINGERPRINT=[0-9a-f]{64}$/m);
  assert.match(cacheText, /^SOURCE_FINGERPRINT=[0-9a-f]{64}$/m);
  assert.match(cacheText, /^TSC_ROOT_FINGERPRINT=[0-9a-f]{64}$/m);
  assert.match(cacheText, /^TSC_SOURCE_FINGERPRINT=[0-9a-f]{64}$/m);
}

{
  const { row } = runCase("exit-code-mismatch", { oracle: true });
  assert.equal(row.state, "yellow");
  assert.deepEqual(row.exit_codes.tsz, [1]);
  assert.deepEqual(row.exit_codes.tsc, [2]);
  assert.ok(
    row.diagnostic_deltas.some(
      (line) => line.includes("compiler exit mismatch (tsz=1, TypeScript7=2)"),
    ),
    "identical diagnostics with different ordinary exits cannot become green",
  );
}

{
  const { row } = runCase("diagnostic-subset", { oracle: true });
  assert.equal(row.state, "yellow");
  assert.deepEqual(row.tsz_diagnostic_codes, []);
  assert.deepEqual(row.tsc_diagnostic_codes, ["TS7006"]);
  assert.ok(
    row.diagnostic_deltas.some(
      (line) => line.includes("tsc: b/index.ts(2,1): error TS7006"),
    ),
    "a diagnostic missing from tsz must survive symmetric multiset comparison",
  );
}

{
  const { row } = runCase("source-mismatch", { oracle: true });
  assert.equal(row.state, "yellow");
  assert.equal(row.exit_class, "exit success");
  assert.equal(row.files_reached, 6);
  assert.deepEqual(row.exit_codes.tsz, [0]);
  assert.deepEqual(row.exit_codes.tsc, [0]);
  assert.equal(row.oracle_classification, "both-pass");
  assert.equal(row.diagnostic_status, "project source-file diagnostic mismatch");
  assert.ok(
    row.diagnostic_deltas.some(
      (line) => line.includes("source file count mismatch (tsz=6, TypeScript7=7)"),
    ),
    "source graph count divergence blocks green even when both compilers exit cleanly",
  );
}

{
  const { firstRow, row } = runCase("success-diagnostic-mismatch", {
    repeat: true,
    oracle: true,
  });
  for (const [label, value] of [["fresh", firstRow], ["cached", row]]) {
    assert.equal(value.state, "yellow", `${label}: RC0 extra diagnostic blocks green`);
    assert.equal(value.exit_class, "exit success");
    assert.deepEqual(value.exit_codes.tsz, [0]);
    assert.deepEqual(value.exit_codes.tsc, [0]);
    assert.deepEqual(value.tsz_diagnostic_codes, ["TS6133"]);
    assert.match(value.diagnostic_status, /diagnostic mismatch after tsz exit success/);
  }
}

{
  const { firstRow, row, cacheText } = runCase("false-negative-import", {
    repeat: true,
    oracle: true,
  });
  for (const [label, value] of [["fresh", firstRow], ["cached", row]]) {
    assert.equal(value.state, "yellow", `${label}: false negative must block green`);
    assert.equal(value.exit_class, "exit success", `${label}: tsz itself returned RC0`);
    assert.equal(value.files_reached, 1, `${label}: tsz admitted only the root file`);
    assert.deepEqual(value.exit_codes.tsz, [0]);
    assert.deepEqual(value.exit_codes.tsc, [1]);
    assert.equal(value.oracle_classification, "tsc-fails-only");
    assert.match(value.diagnostic_status, /graph and false-negative diagnostic mismatch/);
    assert.deepEqual(value.tsc_diagnostic_codes, ["TS2322"]);
    assert.ok(
      value.diagnostic_deltas.some(
        (line) => line.includes("source file count mismatch (tsz=1, TypeScript7=2)"),
      ),
      `${label}: the resolved import graph mismatch must be visible`,
    );
    assert.ok(
      value.diagnostic_deltas.some(
        (line) => line.includes("dep.spec.ts(1,7): error TS2322"),
      ),
      `${label}: a tsc error in an imported non-root dependency must block green`,
    );
  }
  assert.match(cacheText, /^TSC_SOURCE_FILES=2$/m);
}

console.log("test-project-compile-guard-processed-files: all tests passed");
