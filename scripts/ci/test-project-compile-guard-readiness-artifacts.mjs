#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const GUARD_SCRIPT = path.join(ROOT, "scripts", "ci", "project-compile-guard.sh");

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

function writeFile(file, text) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text, "utf8");
}

function writeJson(file, value) {
  writeFile(file, `${JSON.stringify(value, null, 2)}\n`);
}

function writeExecutable(file, text) {
  writeFile(file, text);
  fs.chmodSync(file, 0o755);
}

function git(cwd, args) {
  const result = spawnSync("git", args, {
    cwd,
    encoding: "utf8",
  });
  assert.equal(
    result.status,
    0,
    `git ${args.join(" ")} failed\n${result.stderr || result.stdout}`,
  );
  return result.stdout.trim();
}

function createGitRepo(dir, files) {
  fs.mkdirSync(dir, { recursive: true });
  git(dir, ["init", "-q"]);
  git(dir, ["config", "user.email", "tsz@example.invalid"]);
  git(dir, ["config", "user.name", "tsz test"]);
  for (const [file, text] of Object.entries(files)) {
    writeFile(path.join(dir, file), text);
  }
  git(dir, ["add", "."]);
  git(dir, ["commit", "-q", "-m", "fixture"]);
  return git(dir, ["rev-parse", "HEAD"]);
}

function manifest(entry) {
  return {
    fixture: "type-challenges-project",
    source: {
      repository: "https://example.invalid/type-challenges.git",
      ref: "stale-ref",
      path: "questions/**/template.ts",
    },
    expectedGenerated: 1,
    generated: 1,
    entries: [
      {
        output: entry.output,
        source: entry.source,
        challenge: {
          id: entry.id,
          level: "warm",
          slug: "hello-world",
        },
        declarations: entry.declarations,
      },
    ],
  };
}

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
  const typeCompile = path.join(fixtureRoot, "type-challenges", ".tsz-compile");
  const solutionsCompile = path.join(
    fixtureRoot,
    "type-challenges-solutions",
    ".tsz-compile",
  );

  writeFile(fakeTsz, "#!/usr/bin/env bash\nexit 0\n");
  fs.chmodSync(fakeTsz, 0o755);

  writeJson(pairing, { fixture: "stale-pairing" });
  writeJson(assertionManifest, { fixture: "stale-assertions" });
  writeJson(
    path.join(typeCompile, "type-challenges-template-manifest.json"),
    manifest({
      id: "13",
      output: "questions/00013-warm-hello-world/template.ts",
      source: "questions/00013-warm-hello-world/template.ts",
    }),
  );
  writeJson(
    path.join(typeCompile, "type-challenges-test-cases-manifest.json"),
    manifest({
      id: "13",
      output: "questions/00013-warm-hello-world/test-cases.ts",
      source: "questions/00013-warm-hello-world/test-cases.ts",
    }),
  );
  writeJson(
    path.join(solutionsCompile, "type-challenges-solutions-manifest.json"),
    manifest({
      id: "13",
      output: "solutions/hello-world.ts",
      source: "en/hello-world.md",
      declarations: ["HelloWorld"],
    }),
  );
  writeFile(
    path.join(typeCompile, "utils", "index.d.ts"),
    "export type Expect<T extends true> = T;\nexport type Equal<X, Y> = true;\n",
  );
  writeFile(
    path.join(
      typeCompile,
      "test-cases",
      "questions",
      "00013-warm-hello-world",
      "test-cases.ts",
    ),
    "import type { Equal, Expect } from '@type-challenges/utils'\ntype cases = [Expect<Equal<HelloWorld, string>>]\n",
  );
  writeFile(
    path.join(solutionsCompile, "solutions", "hello-world.ts"),
    "type HelloWorld = string;\nexport {};\n",
  );

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
  const fixtureRoot = path.join(dir, "fixture-root");
  const targetDir = path.join(dir, "target");
  const fakeTsz = path.join(targetDir, "dist-fast", "tsz");
  const fakeTsc = path.join(dir, "fake-tsc");
  const fakeTszLog = path.join(dir, "fake-tsz.log");
  const typeChallengesRepo = path.join(dir, "type-challenges-repo");
  const solutionsRepo = path.join(dir, "type-challenges-solutions-repo");

  writeExecutable(
    fakeTsz,
    [
      "#!/usr/bin/env bash",
      'printf "%s\\t%s\\n" "$PWD" "$*" >> "$TSZ_FAKE_LOG"',
      "exit 0",
      "",
    ].join("\n"),
  );
  writeExecutable(
    fakeTsc,
    ["#!/usr/bin/env bash", "exit 0", ""].join("\n"),
  );

  const typeChallengesRef = createGitRepo(typeChallengesRepo, {
    "questions/00013-warm-hello-world/template.ts": "type HelloWorld = string;\n",
    "questions/00013-warm-hello-world/test-cases.ts":
      "import type { Equal, Expect } from '@type-challenges/utils'\ntype cases = [Expect<Equal<HelloWorld, string>>]\n",
    "utils/index.d.ts":
      "export type Expect<T extends true> = T;\nexport type Equal<X, Y> = true;\n",
  });
  const solutionsRef = createGitRepo(solutionsRepo, {
    "en/hello-world.md": [
      "id: 13",
      "title: Hello World",
      "level: warm",
      "",
      "## Solution",
      "```ts",
      "type HelloWorld = string",
      "```",
      "",
    ].join("\n"),
  });

  const env = {
    ...process.env,
    CARGO_TARGET_DIR: targetDir,
    TSZ_FAKE_LOG: fakeTszLog,
    TSZ_PROJECT_COMPILE_FIXTURE_ROOT: fixtureRoot,
    TSZ_PROJECT_COMPILE_SET: "canary",
    TSZ_PROJECT_COMPILE_FILTER:
      "type-challenges-project|type-challenges-solutions-project|type-challenges-assertion-candidates",
    TSZ_PROJECT_COMPILE_INCLUDE_GENERATED_APPS: "0",
    TYPE_CHALLENGES_REPO: typeChallengesRepo,
    TYPE_CHALLENGES_REF: typeChallengesRef,
    TYPE_CHALLENGES_EXPECTED_GENERATED: "1",
    TYPE_CHALLENGES_EXPECTED_TEST_CASES: "1",
    TYPE_CHALLENGES_SOLUTIONS_REPO: solutionsRepo,
    TYPE_CHALLENGES_SOLUTIONS_REF: solutionsRef,
    TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED: "1",
    TYPE_CHALLENGES_ASSERTION_TSC_BIN: fakeTsc,
    TYPE_CHALLENGES_ASSERTION_CLASSIFIER_TIMEOUT_MS: "5000",
  };
  delete env.TSZ_BIN;

  const result = spawnSync("bash", [GUARD_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
    env,
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);

  const classification = JSON.parse(
    fs.readFileSync(
      path.join(
        fixtureRoot,
        "type-challenges-assertions",
        "type-challenges-assertions-classification.json",
      ),
      "utf8",
    ),
  );
  assert.equal(classification.compilers.tsc.status, "pass");
  assert.equal(classification.compilers.tsz.status, "pass");
  assert.equal(classification.compilers.tsz.command[0], fakeTsz);

  const log = fs.readFileSync(fakeTszLog, "utf8");
  assert.match(log, /type-challenges-assertions/);
});
