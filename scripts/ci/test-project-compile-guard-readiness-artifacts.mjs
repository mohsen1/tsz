#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { REQUIRED_COMPATIBILITY_FIELDS } from "../bench/project-rows.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const GUARD_SCRIPT = path.join(ROOT, "scripts", "ci", "project-compile-guard.sh");
const GUARD_SOURCE = fs.readFileSync(GUARD_SCRIPT, "utf8");
const PROJECT_COMPATIBILITY_SCRIPT = path.join(
  ROOT,
  "scripts",
  "ci",
  "project-compatibility.mjs",
);

assert.doesNotMatch(GUARD_SOURCE, /node_modules\/\.bin\/tsc/);
assert.match(GUARD_SOURCE, /tsz_project_oracle_tsc_command/);
assert.match(GUARD_SOURCE, /done < <\(type_challenges_tsc_command\)/);

function withTempDir(fn) {
  const dir = fs.mkdtempSync(
    path.join(os.tmpdir(), "tsz-project-compile-guard-"),
  );
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeFile(file, text, mode = 0o644) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text, { encoding: "utf8", mode });
}

function writeJson(file, value) {
  writeFile(file, `${JSON.stringify(value, null, 2)}\n`);
}

function writeExecutable(file, text) {
  writeFile(file, text, 0o755);
  fs.chmodSync(file, 0o755);
}

function successfulFakeTsz(body = "", rootFiles = 1, sourceFiles = rootFiles) {
  return `#!/usr/bin/env bash
set -euo pipefail
${body}
stats_file=""
project_file=""
while [[ "$#" -gt 0 ]]; do
  if [[ "$1" == "--perf-counters-json" ]]; then
    stats_file="$2"
    shift 2
    continue
  fi
  if [[ "$1" == "-p" || "$1" == "--project" ]]; then
    project_file="$2"
    shift 2
    continue
  fi
  shift
done
[[ -n "$stats_file" ]]
[[ -n "$project_file" ]]
FAKE_STATS_FILE="$stats_file" FAKE_PROJECT_FILE="$project_file" \
FAKE_ROOT_FILES="${rootFiles}" FAKE_SOURCE_FILES="${sourceFiles}" node -e '
  const fs = require("node:fs");
  const path = require("node:path");
  const files = [];
  function walk(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (entry.name === "node_modules") continue;
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (/\\.(?:d\\.)?(?:m|c)?tsx?$/.test(entry.name)) files.push(file);
    }
  }
  walk(path.dirname(process.env.FAKE_PROJECT_FILE));
  const roots = files.slice(0, Number(process.env.FAKE_ROOT_FILES));
  const sources = files.slice(0, Number(process.env.FAKE_SOURCE_FILES));
  fs.writeFileSync(process.env.FAKE_STATS_FILE, JSON.stringify({
    schema_version: 2,
    stats: {
      semantic_completion: "complete",
      root_files: roots.length,
      source_files: sources.length,
      files: sources.length,
      root_file_paths: roots,
      source_file_paths: sources,
    },
  }) + "\\n");
'
exit 0
`;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: ROOT,
    encoding: "utf8",
    ...options,
  });
  assert.equal(
    result.status,
    0,
    [
      `${command} ${args.join(" ")} failed`,
      result.stdout,
      result.stderr,
    ].filter(Boolean).join("\n"),
  );
  return result;
}

function git(cwd, args) {
  return run("git", args, { cwd }).stdout.trim();
}

function createGitRepo(dir, files) {
  fs.mkdirSync(dir, { recursive: true });
  for (const [file, text] of Object.entries(files)) {
    writeFile(path.join(dir, file), text);
  }
  return initRepo(dir);
}

function initRepo(dir) {
  run("git", ["init", "--quiet"], { cwd: dir });
  run("git", ["add", "."], { cwd: dir });
  run(
    "git",
    [
      "-c",
      "user.name=smoke",
      "-c",
      "user.email=smoke@example.invalid",
      "commit",
      "--quiet",
      "-m",
      "init",
    ],
    { cwd: dir },
  );
  return run("git", ["rev-parse", "HEAD"], { cwd: dir }).stdout.trim();
}

function readJsonl(file) {
  return fs.readFileSync(file, "utf8")
    .trim()
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function assertRequiredCompatibilityFields(row) {
  for (const field of [
    ...REQUIRED_COMPATIBILITY_FIELDS,
    "compile_input_stable",
    "evidence_protocol_fingerprint",
  ]) {
    assert.ok(
      Object.prototype.hasOwnProperty.call(row, field),
      `project compatibility row is missing ${field}`,
    );
  }
}

function runGuardRaw(env) {
  return spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TSZ_PROJECT_COMPILE_SET: "required",
      TSZ_PROJECT_COMPILE_FILTER: "^does-not-match-any-project$",
      TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
      ...env,
    },
  });
}

withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const fakeTsz = path.join(dir, "fake-tsz");
  const outsideJsonl = path.join(dir, "outside-project-compatibility.jsonl");
  writeExecutable(fakeTsz, "#!/usr/bin/env bash\nexit 0\n");

  const result = runGuardRaw({
    TSZ_BIN: fakeTsz,
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
    TSZ_PROJECT_COMPILE_COMPATIBILITY_JSONL: outsideJsonl,
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /project compatibility JSONL must stay inside fixture root/,
  );
  assert.equal(fs.existsSync(outsideJsonl), false);
});

withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const fakeTsz = path.join(dir, "fake-tsz");
  const sharedOutput = path.join(fixtureRoot, "project-compatibility.json");
  writeExecutable(fakeTsz, "#!/usr/bin/env bash\nexit 0\n");

  const result = runGuardRaw({
    TSZ_BIN: fakeTsz,
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
    TSZ_PROJECT_COMPILE_COMPATIBILITY_JSONL: sharedOutput,
    TSZ_PROJECT_COMPILE_COMPATIBILITY_SUMMARY: sharedOutput,
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /project compatibility JSONL and summary paths must be distinct/,
  );
  assert.equal(fs.existsSync(sharedOutput), false);
});

withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const fakeTsz = path.join(dir, "fake-tsz");
  const summaryDir = path.join(fixtureRoot, "project-compatibility-summary.json");
  writeExecutable(fakeTsz, "#!/usr/bin/env bash\nexit 0\n");
  fs.mkdirSync(summaryDir, { recursive: true });

  const result = runGuardRaw({
    TSZ_BIN: fakeTsz,
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
    TSZ_PROJECT_COMPILE_COMPATIBILITY_SUMMARY: summaryDir,
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /project compatibility summary path is not a file/,
  );
});

withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const sourceRoot = path.join(
    fixtureRoot,
    "type-challenges-solutions",
    ".tsz-compile",
    "solutions",
  );
  const source = path.join(sourceRoot, "00001-medium-remap.ts");
  const jsonl = path.join(fixtureRoot, "project-compatibility.jsonl");

  writeFile(
    source,
    [
      "type Remap<T> = {",
      "  [K in keyof T as K]: T[K];",
      "};",
      "",
    ].join("\n"),
  );

  run("node", [PROJECT_COMPATIBILITY_SCRIPT, "record"], {
    env: {
      ...process.env,
      COMPAT_JSONL_FILE: jsonl,
      COMPAT_NAME: "type-challenges-solutions-project",
      COMPAT_EXIT_CLASS: "nonzero exit",
      COMPAT_PHASE: "check",
      COMPAT_DIAGNOSTIC_STATUS: "diagnostic mismatch or compiler error",
      COMPAT_DIAGNOSTIC_DELTA: `tsz: ${source}(2,3): error TS2344: mapped failure`,
      COMPAT_TSCONFIG_PATH: path.join(
        fixtureRoot,
        "type-challenges-solutions",
        ".tsz-compile",
        "tsconfig.tsz-guard.json",
      ),
      COMPAT_SOURCE_ROOT: sourceRoot,
      COMPAT_FIXTURE_ROOT: fixtureRoot,
      COMPAT_TSZ_EXIT_CODES: "1",
      COMPAT_TSC_EXIT_CODES: "0",
    },
  });

  const rows = readJsonl(jsonl);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].name, "type-challenges-solutions-project");
  assert.equal(
    rows[0].primary_subsystem,
    "type-challenges mapped/key-remapped types",
  );
  assert.equal(
    rows[0].first_failure_class,
    "type-challenges mapped/key-remapped types",
  );
  assert.equal(
    rows[0].owner_track,
    "Track 2/3 Type Challenges type-level semantics",
  );
  assert.deepEqual(
    rows[0].diagnostic_subsystems.map((group) => group.subsystem),
    [
      "type-challenges mapped/key-remapped types",
      "type-challenges indexed access",
    ],
  );
  assert.deepEqual(rows[0].diagnostic_subsystems[0].codes, ["TS2344"]);
});

withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const fakeTsz = path.join(dir, "fake-tsz");
  const pairing = path.join(
    fixtureRoot,
    "type-challenges-readiness-pairing.json",
  );
  const assertionsDir = path.join(fixtureRoot, "type-challenges-assertions");
  const assertionManifest = path.join(
    assertionsDir,
    "type-challenges-assertions-manifest.json",
  );

  writeFile(fakeTsz, "#!/usr/bin/env bash\nexit 0\n");
  fs.chmodSync(fakeTsz, 0o755);

  writeJson(pairing, { fixture: "stale-pairing" });
  writeJson(assertionManifest, { fixture: "stale-assertions" });

  const result = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TSZ_BIN: fakeTsz,
      TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
      TSZ_PROJECT_COMPILE_SET: "required",
      TSZ_PROJECT_COMPILE_FILTER: "^does-not-match-any-project$",
      TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    },
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.equal(fs.existsSync(pairing), false);
  assert.equal(fs.existsSync(assertionsDir), false);
});

withTempDir((dir) => {
  const solutionsRepo = path.join(dir, "solutions");
  const fixtureRoot = path.join(dir, "fixture-root");
  const binDir = path.join(dir, "bin");
  const tszTouched = path.join(dir, "tsz-ran");

  writeFile(
    path.join(solutionsRepo, "en", "00014-easy-first.md"),
    [
      "id: 14",
      "title: First",
      "level: easy",
      "",
      "## Solution",
      "```ts",
      "type First<T extends unknown[]> = T[0]",
      "```",
      "",
    ].join("\n"),
  );
  const solutionsRef = initRepo(solutionsRepo);

  writeFile(
    path.join(binDir, "tsz"),
    successfulFakeTsz(`touch ${JSON.stringify(tszTouched)}`, 2),
    0o755,
  );
  writeFile(
    path.join(binDir, "tsc"),
    "#!/usr/bin/env bash\necho 'solutions/00014-easy-first.ts(1,1): error TS2344: oracle failed' >&2\nexit 1\n",
    0o755,
  );

  const result = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TSZ_BIN: path.join(binDir, "tsz"),
      TYPE_CHALLENGES_ASSERTION_TSC_BIN: path.join(binDir, "tsc"),
      TYPE_CHALLENGES_SOLUTIONS_REPO: solutionsRepo,
      TYPE_CHALLENGES_SOLUTIONS_REF: solutionsRef,
      TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED: "1",
      TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
      TSZ_PROJECT_COMPILE_SET: "required",
      TSZ_PROJECT_COMPILE_FILTER: "type-challenges-solutions-project",
      TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    },
  });

  assert.equal(result.status, 1, result.stdout || result.stderr);
  assert.match(result.stderr, /failed the tsc oracle check/);
  assert.equal(fs.existsSync(tszTouched), false);

  const rows = readJsonl(path.join(fixtureRoot, "project-compatibility.jsonl"));
  assert.equal(rows.length, 1);
  assertRequiredCompatibilityFields(rows[0]);
  assert.equal(rows[0].name, "type-challenges-solutions-project");
  assert.equal(rows[0].state, "gray");
  assert.equal(rows[0].exit_class, "fixture invalid");
  assert.equal(rows[0].phase, "fixture setup");
  assert.equal(rows[0].diagnostic_status, "tsc fixture failed");
  assert.deepEqual(rows[0].exit_codes.tsc, [1]);
  assert.deepEqual(rows[0].exit_codes.tsz, []);
});

withTempDir((dir) => {
  const solutionsRepo = path.join(dir, "solutions");
  const fixtureRoot = path.join(dir, "fixture-root");
  const binDir = path.join(dir, "bin");
  const tszTouched = path.join(dir, "tsz-ran");

  writeFile(
    path.join(solutionsRepo, "en", "00014-easy-first.md"),
    [
      "id: 14",
      "title: First",
      "level: easy",
      "",
      "## Solution",
      "```ts",
      "type First<T extends unknown[]> = T[0]",
      "```",
      "",
    ].join("\n"),
  );
  const solutionsRef = initRepo(solutionsRepo);

  writeFile(
    path.join(binDir, "tsz"),
    successfulFakeTsz(`touch ${JSON.stringify(tszTouched)}`, 2),
    0o755,
  );
  writeFile(
    path.join(binDir, "tsc"),
    "#!/usr/bin/env bash\nexit 0\n",
    0o755,
  );

  const result = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TSZ_BIN: path.join(binDir, "tsz"),
      TYPE_CHALLENGES_ASSERTION_TSC_BIN: path.join(binDir, "tsc"),
      TYPE_CHALLENGES_SOLUTIONS_REPO: solutionsRepo,
      TYPE_CHALLENGES_SOLUTIONS_REF: solutionsRef,
      TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED: "1",
      TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
      TSZ_PROJECT_COMPILE_SET: "required",
      TSZ_PROJECT_COMPILE_FILTER: "type-challenges-solutions-project",
      TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    },
  });

  assert.equal(result.status, 0, result.stdout || result.stderr);
  assert.equal(fs.existsSync(tszTouched), true);

  const rows = readJsonl(path.join(fixtureRoot, "project-compatibility.jsonl"));
  assert.equal(rows.length, 1);
  assertRequiredCompatibilityFields(rows[0]);
  assert.equal(rows[0].name, "type-challenges-solutions-project");
  assert.equal(rows[0].state, "gray", "a run without a verified build manifest is non-evidence");
  assert.ok(rows[0].evidence_failures.includes("build_manifest_sha256"));
  assert.equal(rows[0].exit_class, "exit success");
  assert.equal(rows[0].phase, "check");
  assert.equal(rows[0].diagnostic_status, "none");
  assert.deepEqual(rows[0].exit_codes.tsc, [0]);
  assert.deepEqual(rows[0].exit_codes.tsz, [0]);
});

withTempDir((dir) => {
  const solutionsRepo = path.join(dir, "solutions");
  const fixtureRoot = path.join(dir, "fixture-root");
  const binDir = path.join(dir, "bin");
  const tszTouched = path.join(dir, "tsz-ran");

  writeFile(
    path.join(solutionsRepo, "en", "00014-easy-first.md"),
    [
      "id: 14",
      "title: First",
      "level: easy",
      "",
      "## Solution",
      "```ts",
      "type First<T extends unknown[]> = T[0]",
      "```",
      "",
    ].join("\n"),
  );
  const solutionsRef = initRepo(solutionsRepo);

  writeFile(
    path.join(binDir, "tsz"),
    `#!/usr/bin/env bash\ntouch ${JSON.stringify(tszTouched)}\nexit 0\n`,
    0o755,
  );

  const result = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TSZ_BIN: path.join(binDir, "tsz"),
      TYPE_CHALLENGES_ASSERTION_TSC_BIN: path.join(binDir, "missing-tsc"),
      TYPE_CHALLENGES_SOLUTIONS_REPO: solutionsRepo,
      TYPE_CHALLENGES_SOLUTIONS_REF: solutionsRef,
      TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED: "1",
      TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
      TSZ_PROJECT_COMPILE_SET: "required",
      TSZ_PROJECT_COMPILE_FILTER: "type-challenges-solutions-project",
      TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    },
  });

  assert.equal(result.status, 1, result.stdout || result.stderr);
  assert.match(result.stderr, /requires a tsc oracle/);
  assert.equal(fs.existsSync(tszTouched), false);

  const rows = readJsonl(path.join(fixtureRoot, "project-compatibility.jsonl"));
  assert.equal(rows.length, 1);
  assertRequiredCompatibilityFields(rows[0]);
  assert.equal(rows[0].name, "type-challenges-solutions-project");
  assert.equal(rows[0].state, "gray");
  assert.equal(rows[0].exit_class, "fixture invalid");
  assert.equal(rows[0].phase, "fixture setup");
  assert.equal(rows[0].diagnostic_status, "tsc oracle unavailable");
  assert.deepEqual(rows[0].exit_codes.tsc, [127]);
  assert.deepEqual(rows[0].exit_codes.tsz, []);
});

// Opt-in result cache: second guard run skips tsz when every conservative input is unchanged.
withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const fakeRepo = path.join(dir, "fake-utility-types");
  const fakeTsz = path.join(dir, "tsz");
  const runCountFile = path.join(dir, "tsz-run-count");

  const fakeRef = createGitRepo(fakeRepo, {
    "src/index.ts": "export type Id<T> = T;\n",
  });

  writeExecutable(
    fakeTsz,
    successfulFakeTsz(`
count=0
[ -f ${JSON.stringify(runCountFile)} ] && count=$(cat ${JSON.stringify(runCountFile)})
printf '%s' "$((count+1))" > ${JSON.stringify(runCountFile)}
`),
  );

  const env = {
    ...process.env,
    TSZ_BIN: fakeTsz,
    UTILITY_TYPES_REPO: fakeRepo,
    UTILITY_TYPES_REF: fakeRef,
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
    TSZ_PROJECT_COMPILE_SET: "required",
    TSZ_PROJECT_COMPILE_FILTER: "^utility-types-project$",
    TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    TSZ_PROJECT_COMPILE_RESULT_CACHE: "1",
  };

  const r1 = spawnSync("bash", [GUARD_SCRIPT], { cwd: ROOT, encoding: "utf8", env });
  assert.equal(r1.status, 0, `first run failed:\n${r1.stderr}\n${r1.stdout}`);
  assert.equal(
    fs.readFileSync(runCountFile, "utf8"),
    "1",
    "tsz should run on first pass",
  );

  const r2 = spawnSync("bash", [GUARD_SCRIPT], { cwd: ROOT, encoding: "utf8", env });
  assert.equal(r2.status, 0, `second run failed:\n${r2.stderr}\n${r2.stdout}`);
  assert.equal(
    fs.readFileSync(runCountFile, "utf8"),
    "1",
    "tsz must not re-run when binary, tsconfig, and fixture ref are unchanged",
  );

  const rows = readJsonl(
    path.join(fixtureRoot, "project-compatibility.jsonl"),
  );
  assert.equal(rows.length, 1);
  assertRequiredCompatibilityFields(rows[0]);
  assert.equal(rows[0].name, "utility-types-project");
  assert.equal(rows[0].state, "gray", "a run without a verified build manifest is non-evidence");
  assert.ok(rows[0].evidence_failures.includes("build_manifest_sha256"));
  assert.equal(rows[0].exit_class, "exit success");
});

// Result cache disabled: tsz always runs when TSZ_PROJECT_COMPILE_RESULT_CACHE=0.
withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const fakeRepo = path.join(dir, "fake-utility-types");
  const fakeTsz = path.join(dir, "tsz");
  const runCountFile = path.join(dir, "tsz-run-count");

  const fakeRef = createGitRepo(fakeRepo, {
    "src/index.ts": "export type Id<T> = T;\n",
  });

  writeExecutable(
    fakeTsz,
    successfulFakeTsz(`
count=0
[ -f ${JSON.stringify(runCountFile)} ] && count=$(cat ${JSON.stringify(runCountFile)})
printf '%s' "$((count+1))" > ${JSON.stringify(runCountFile)}
`),
  );

  const env = {
    ...process.env,
    TSZ_BIN: fakeTsz,
    UTILITY_TYPES_REPO: fakeRepo,
    UTILITY_TYPES_REF: fakeRef,
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
    TSZ_PROJECT_COMPILE_SET: "required",
    TSZ_PROJECT_COMPILE_FILTER: "^utility-types-project$",
    TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    TSZ_PROJECT_COMPILE_RESULT_CACHE: "0",
  };

  const r1 = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env,
  });
  assert.equal(
    r1.status,
    0,
    `first cache-disabled run failed:\n${r1.stderr}\n${r1.stdout}`,
  );
  const r2 = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env,
  });
  assert.equal(
    r2.status,
    0,
    `second cache-disabled run failed:\n${r2.stderr}\n${r2.stdout}`,
  );

  assert.equal(
    fs.readFileSync(runCountFile, "utf8"),
    "2",
    "tsz must re-run on every pass when cache is disabled",
  );
});

// Result cache miss: tsz re-runs after tsz binary changes.
withTempDir((dir) => {
  const fixtureRoot = path.join(dir, "fixture-root");
  const fakeRepo = path.join(dir, "fake-utility-types");
  const tszV1 = path.join(dir, "tsz-v1");
  const tszV2 = path.join(dir, "tsz-v2");
  const runCountFile = path.join(dir, "tsz-run-count");

  const fakeRef = createGitRepo(fakeRepo, {
    "src/index.ts": "export type Id<T> = T;\n",
  });

  const counterScript = (tag) => successfulFakeTsz(
    `# ${tag}\ncount=0\n[ -f ${JSON.stringify(runCountFile)} ] && count=$(cat ${JSON.stringify(runCountFile)})\nprintf '%s' "$((count+1))" > ${JSON.stringify(runCountFile)}`,
  );

  writeExecutable(tszV1, counterScript("v1"));
  writeExecutable(tszV2, counterScript("v2"));

  const commonEnv = {
    ...process.env,
    UTILITY_TYPES_REPO: fakeRepo,
    UTILITY_TYPES_REF: fakeRef,
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
    TSZ_PROJECT_COMPILE_SET: "required",
    TSZ_PROJECT_COMPILE_FILTER: "^utility-types-project$",
    TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    TSZ_PROJECT_COMPILE_RESULT_CACHE: "1",
  };

  // First run with v1 — cache miss, tsz runs.
  const r1 = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: { ...commonEnv, TSZ_BIN: tszV1 },
  });
  assert.equal(r1.status, 0, r1.stderr);
  assert.equal(fs.readFileSync(runCountFile, "utf8"), "1");

  // Second run with v1 — cache hit, tsz skipped.
  const r2 = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: { ...commonEnv, TSZ_BIN: tszV1 },
  });
  assert.equal(r2.status, 0, r2.stderr);
  assert.equal(
    fs.readFileSync(runCountFile, "utf8"),
    "1",
    "same binary: cache hit",
  );

  // Third run with v2 — binary changed, cache miss, tsz re-runs.
  const r3 = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env: { ...commonEnv, TSZ_BIN: tszV2 },
  });
  assert.equal(r3.status, 0, r3.stderr);
  assert.equal(
    fs.readFileSync(runCountFile, "utf8"),
    "2",
    "new binary: cache miss",
  );
});
