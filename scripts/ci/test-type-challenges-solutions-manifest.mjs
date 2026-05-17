#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const PROJECT_FIXTURES = path.join(ROOT, "scripts", "bench", "project-fixtures.sh");
const MANIFEST_SCRIPT = path.join(
  ROOT,
  "scripts",
  "ci",
  "type-challenges-solutions-manifest.mjs",
);

function shellQuote(value) {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-type-challenges-manifest-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeSolution(sourceDir, fileName, { id, title, level, fence, declaration }) {
  fs.writeFileSync(
    path.join(sourceDir, "en", fileName),
    `id: ${id}
title: ${title}
level: ${level}

## Solution

\`\`\`${fence}
${declaration}
\`\`\`
`,
    "utf8",
  );
}

function runManifest(tsvPath, manifestPath) {
  return spawnSync(process.execPath, [MANIFEST_SCRIPT, tsvPath, manifestPath], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TYPE_CHALLENGES_SOLUTIONS_REPO:
        "https://example.invalid/type-challenges-solutions.git",
      TYPE_CHALLENGES_SOLUTIONS_REF: "fixture-ref",
      TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED: "1",
    },
  });
}

withTempDir((dir) => {
  const sourceDir = path.join(dir, "source");
  const compileDir = path.join(dir, "compile");
  fs.mkdirSync(path.join(sourceDir, "en"), { recursive: true });

  writeSolution(sourceDir, "alpha.md", {
    id: "1",
    title: "Alpha Challenge",
    level: "easy",
    fence: "ts",
    declaration: "type Alpha = string;",
  });
  writeSolution(sourceDir, "beta.md", {
    id: "2",
    title: "Beta Challenge",
    level: "medium",
    fence: "typescript",
    declaration: `interface Beta { value: number }
declare function beta(value: Beta): void;`,
  });

  const script = `
set -euo pipefail
TYPE_CHALLENGES_SOLUTIONS_REPO=https://example.invalid/type-challenges-solutions.git
TYPE_CHALLENGES_SOLUTIONS_REF=fixture-ref
TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED=2
source ${shellQuote(PROJECT_FIXTURES)}
tsz_write_type_challenges_solutions_config ${shellQuote(sourceDir)} ${shellQuote(compileDir)}
`;
  const result = spawnSync("bash", ["-c", script], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  assert.equal(manifest.fixture, "type-challenges-solutions-project");
  assert.equal(manifest.source.repository, "https://example.invalid/type-challenges-solutions.git");
  assert.equal(manifest.source.ref, "fixture-ref");
  assert.equal(manifest.expectedGenerated, 2);
  assert.equal(manifest.generated, 2);
  assert.deepEqual(
    manifest.entries.map((entry) => [
      entry.output,
      entry.source,
      entry.challenge.id,
      entry.declarations,
    ]),
    [
      ["solutions/alpha.ts", "en/alpha.md", "1", ["Alpha"]],
      ["solutions/beta.ts", "en/beta.md", "2", ["Beta", "beta"]],
    ],
  );

  assert.match(
    fs.readFileSync(path.join(compileDir, "solutions", "alpha.ts"), "utf8"),
    /type Alpha = string;/,
  );
  assert.match(
    fs.readFileSync(path.join(compileDir, "solutions", "beta.ts"), "utf8"),
    /interface Beta \{ value: number \}/,
  );
  assert.ok(fs.existsSync(path.join(compileDir, "tsconfig.tsz-guard.json")));
});

withTempDir((dir) => {
  const sourceDir = path.join(dir, "source");
  const compileDir = path.join(dir, "compile");
  fs.mkdirSync(path.join(sourceDir, "en"), { recursive: true });

  writeSolution(sourceDir, "custom.md", {
    id: "custom-shape",
    title: "Custom Challenge",
    level: "easy",
    fence: "ts",
    declaration: "type Custom = string;",
  });

  const script = `
set -euo pipefail
TYPE_CHALLENGES_SOLUTIONS_REPO=https://example.invalid/type-challenges-solutions.git
TYPE_CHALLENGES_SOLUTIONS_REF=fixture-ref
TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED=1
source ${shellQuote(PROJECT_FIXTURES)}
tsz_write_type_challenges_solutions_config ${shellQuote(sourceDir)} ${shellQuote(compileDir)}
`;
  const result = spawnSync("bash", ["-c", script], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unparseable challenge id/);
});

withTempDir((dir) => {
  const sourceDir = path.join(dir, "source");
  const compileDir = path.join(dir, "compile");
  fs.mkdirSync(path.join(sourceDir, "en"), { recursive: true });

  writeSolution(sourceDir, "alpha.md", {
    id: "001",
    title: "Alpha Challenge",
    level: "easy",
    fence: "ts",
    declaration: "type Alpha = string;",
  });
  writeSolution(sourceDir, "beta.md", {
    id: "1",
    title: "Beta Challenge",
    level: "medium",
    fence: "ts",
    declaration: "type Beta = number;",
  });

  const script = `
set -euo pipefail
TYPE_CHALLENGES_SOLUTIONS_REPO=https://example.invalid/type-challenges-solutions.git
TYPE_CHALLENGES_SOLUTIONS_REF=fixture-ref
TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED=2
source ${shellQuote(PROJECT_FIXTURES)}
tsz_write_type_challenges_solutions_config ${shellQuote(sourceDir)} ${shellQuote(compileDir)}
`;
  const result = spawnSync("bash", ["-c", script], {
    cwd: ROOT,
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicate Type Challenges solution challenge id 1/);
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");
  fs.writeFileSync(path.join(dir, "outside.ts"), "type Outside = string;\n");

  const outputTraversal = path.join(dir, "output-traversal.tsv");
  fs.writeFileSync(
    outputTraversal,
    "output\tsource\tid\tlevel\ttitle\n../outside.ts\ten/alpha.md\t1\teasy\tAlpha\n",
    "utf8",
  );
  const outputResult = runManifest(outputTraversal, manifestPath);
  assert.notEqual(outputResult.status, 0);
  assert.match(outputResult.stderr, /unsafe manifest output path: \.\.\/outside\.ts/);

  const sourceTraversal = path.join(dir, "source-traversal.tsv");
  fs.writeFileSync(
    sourceTraversal,
    "output\tsource\tid\tlevel\ttitle\nsolutions/alpha.ts\t../alpha.md\t1\teasy\tAlpha\n",
    "utf8",
  );
  const sourceResult = runManifest(sourceTraversal, manifestPath);
  assert.notEqual(sourceResult.status, 0);
  assert.match(sourceResult.stderr, /unsafe manifest source path: \.\.\/alpha\.md/);
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");

  const badLevel = path.join(dir, "bad-level.tsv");
  fs.writeFileSync(
    badLevel,
    "output\tsource\tid\tlevel\ttitle\nsolutions/alpha.ts\ten/alpha.md\t1\tweird\tAlpha\n",
    "utf8",
  );
  const result = runManifest(badLevel, manifestPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unknown challenge level weird/);
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");
  fs.writeFileSync(path.join(compileDir, "solutions", "beta.ts"), "type Beta = string;\n");

  const duplicateOutput = path.join(dir, "duplicate-output.tsv");
  fs.writeFileSync(
    duplicateOutput,
    [
      "output\tsource\tid\tlevel\ttitle",
      "solutions/alpha.ts\ten/alpha.md\t1\teasy\tAlpha",
      "solutions/alpha.ts\ten/beta.md\t2\teasy\tBeta",
      "",
    ].join("\n"),
    "utf8",
  );
  const outputResult = runManifest(duplicateOutput, manifestPath);
  assert.notEqual(outputResult.status, 0);
  assert.match(
    outputResult.stderr,
    /duplicate Type Challenges solution output solutions\/alpha\.ts/,
  );

  const duplicateSource = path.join(dir, "duplicate-source.tsv");
  fs.writeFileSync(
    duplicateSource,
    [
      "output\tsource\tid\tlevel\ttitle",
      "solutions/alpha.ts\ten/alpha.md\t1\teasy\tAlpha",
      "solutions/beta.ts\ten/alpha.md\t2\teasy\tBeta",
      "",
    ].join("\n"),
    "utf8",
  );
  const sourceResult = runManifest(duplicateSource, manifestPath);
  assert.notEqual(sourceResult.status, 0);
  assert.match(
    sourceResult.stderr,
    /duplicate Type Challenges solution source en\/alpha\.md/,
  );
});
