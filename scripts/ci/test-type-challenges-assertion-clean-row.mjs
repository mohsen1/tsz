#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const GUARD = path.join(ROOT, "scripts", "ci", "project-compile-guard.sh");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-type-challenges-clean-row-"));
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

withTempDir((dir) => {
  const typeRepo = path.join(dir, "type-challenges");
  const solutionsRepo = path.join(dir, "solutions");
  const binDir = path.join(dir, "bin");
  const fixtureRoot = path.join(dir, "fixtures");

  writeFile(
    path.join(typeRepo, "questions", "00014-easy-first", "template.ts"),
    "type First<T extends unknown[]> = T[0];\n",
  );
  writeFile(
    path.join(typeRepo, "questions", "00014-easy-first", "test-cases.ts"),
    [
      'import type { Equal, Expect } from "@type-challenges/utils";',
      "type cases = [Expect<Equal<First<[1, 2]>, 1>>];",
      "",
    ].join("\n"),
  );
  writeFile(
    path.join(typeRepo, "utils", "index.d.ts"),
    "export type Expect<T extends true> = T;\nexport type Equal<X, Y> = true;\n",
  );
  const typeRef = initRepo(typeRepo);

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

  writeFile(path.join(binDir, "tsz"), "#!/usr/bin/env bash\nexit 0\n", 0o755);
  writeFile(path.join(binDir, "tsc"), "#!/usr/bin/env bash\nexit 0\n", 0o755);

  const result = run(GUARD, [], {
    env: {
      ...process.env,
      TSZ_BIN: path.join(binDir, "tsz"),
      TYPE_CHALLENGES_ASSERTION_TSC_BIN: path.join(binDir, "tsc"),
      TYPE_CHALLENGES_REPO: typeRepo,
      TYPE_CHALLENGES_REF: typeRef,
      TYPE_CHALLENGES_EXPECTED_GENERATED: "1",
      TYPE_CHALLENGES_EXPECTED_TEST_CASES: "1",
      TYPE_CHALLENGES_SOLUTIONS_REPO: solutionsRepo,
      TYPE_CHALLENGES_SOLUTIONS_REF: solutionsRef,
      TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED: "1",
      TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
      TSZ_PROJECT_COMPILE_SET: "canary",
      TSZ_PROJECT_COMPILE_FILTER: "type-challenges-assertions-tsc-clean",
      TSZ_PROJECT_COMPILE_ALLOW_FAILURES: "1",
    },
  });
  assert.match(result.stdout, /type-challenges-assertions-tsc-clean compiled successfully/);

  const rows = readJsonl(path.join(fixtureRoot, "project-compatibility.jsonl"));
  assert.equal(rows.length, 1);
  assert.equal(rows[0].name, "type-challenges-assertions-tsc-clean");
  assert.equal(rows[0].state, "green");
  assert.equal(rows[0].phase, "check");
  assert.equal(rows[0].files_reached, 1);
  assert.equal(
    rows[0].repro.tsconfig_path,
    "type-challenges-assertions-tsc-clean/tsconfig.tsz-guard.json",
  );
  assert.equal(
    rows[0].assertion_clean_subset.manifest_path,
    "type-challenges-assertions-tsc-clean/type-challenges-assertions-tsc-clean-manifest.json",
  );
  assert.equal(
    rows[0].assertion_clean_subset.classification_path,
    "type-challenges-assertions-tsc-clean/type-challenges-assertions-tsc-clean-classification.json",
  );
  assert.equal(rows[0].assertion_clean_subset.total_candidates, 1);
  assert.equal(rows[0].assertion_clean_subset.generated_assertions, 1);
  assert.equal(
    rows[0].assertion_clean_subset.assertions_referencing_solution_declaration,
    1,
  );
  assert.equal(
    rows[0].assertion_clean_subset.assertions_missing_solution_declaration_reference,
    0,
  );
  assert.equal(rows[0].assertion_clean_subset.rejected_from_full_corpus, 0);
  assert.equal(rows[0].assertion_clean_subset.tsc_status, "pass");
  assert.equal(rows[0].assertion_clean_subset.tsz_status, "pass");
  assert.equal(rows[0].assertion_clean_subset.tsc_diagnostic_free, 1);
  assert.equal(rows[0].assertion_clean_subset.tsz_diagnostic_free, 1);

  assert.equal(
    fs.existsSync(
      path.join(
        fixtureRoot,
        "type-challenges-assertions-tsc-clean",
        "type-challenges-assertions-tsc-clean-classification.json",
      ),
    ),
    true,
  );
});
