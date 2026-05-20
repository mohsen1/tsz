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
const SOURCE_SHA = "0".repeat(64);

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

function manifestRows(rows) {
  return [
    "output\tsource\tsourceSha256\tid\tlevel\ttitle",
    ...rows.map((row) => row.join("\t")),
    "",
  ].join("\n");
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
      entry.challenge.sourceStem,
      entry.challenge.sourceSha256,
      entry.declarations,
      entry.semanticFamilies,
      entry.outputSha256,
    ]),
    [
      [
        "solutions/alpha.ts",
        "en/alpha.md",
        "1",
        "alpha",
        "b92e8b188e1db2e53945feafadad77e3a2bd59dce58b5750df294df841424e1f",
        ["Alpha"],
        ["unclassified"],
        "387b4bcfd901b7e11dfd6b3021e15427fa59c7b9a15708c5f4f72078e35534f1",
      ],
      [
        "solutions/beta.ts",
        "en/beta.md",
        "2",
        "beta",
        "1261ae9c2cf795dba24f57690913e2c078acaef2f0de673fdce6cd2e4ecd1d68",
        ["Beta", "beta"],
        ["unclassified"],
        "1319ba2703438e218b6516c23040532ae7faf9398b78ad176e522fbb9f44ce15",
      ],
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
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");

  const tsvPath = path.join(compileDir, "blank-ref.tsv");
  fs.writeFileSync(
    tsvPath,
    manifestRows([["solutions/alpha.ts", "en/alpha.md", SOURCE_SHA, "1", "easy", "Alpha"]]),
    "utf8",
  );
  const result = spawnSync(process.execPath, [MANIFEST_SCRIPT, tsvPath, manifestPath], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TYPE_CHALLENGES_SOLUTIONS_REPO:
        "https://example.invalid/type-challenges-solutions.git",
      TYPE_CHALLENGES_SOLUTIONS_REF: "   ",
      TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED: "1",
    },
  });
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    /missing Type Challenges solutions repository, ref, or expected count/,
  );
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(
    path.join(compileDir, "solutions", "remap.ts"),
    [
      "type Remap<T> = {",
      "  [K in keyof T as K]: T[K];",
      "};",
      "",
    ].join("\n"),
  );

  const tsvPath = path.join(compileDir, "entries.tsv");
  fs.writeFileSync(
    tsvPath,
    manifestRows([["solutions/remap.ts", "en/remap.md", SOURCE_SHA, "1", "medium", "Remap"]]),
    "utf8",
  );

  const result = runManifest(tsvPath, manifestPath);
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  assert.deepEqual(manifest.entries[0].semanticFamilies, [
    "mapped/key-remapped types",
    "indexed access",
  ]);
  assert.match(manifest.entries[0].outputSha256, /^[0-9a-f]{64}$/);
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");

  const tsvPath = path.join(compileDir, "entries.tsv");
  fs.writeFileSync(
    tsvPath,
    manifestRows([["solutions/alpha.ts", "en/alpha.md", SOURCE_SHA, "1", "easy", "Alpha"]]),
    "utf8",
  );

  const outsideManifest = path.join(dir, "outside-manifest.json");
  const outsideResult = runManifest(tsvPath, outsideManifest);
  assert.notEqual(outsideResult.status, 0);
  assert.match(
    outsideResult.stderr,
    /solution manifest path must stay inside the compile directory/,
  );
  assert.equal(fs.existsSync(outsideManifest), false);

  const inputResult = runManifest(tsvPath, tsvPath);
  assert.notEqual(inputResult.status, 0);
  assert.match(inputResult.stderr, /solution manifest path must not overwrite the TSV input/);

  const outputResult = runManifest(tsvPath, path.join(compileDir, "solutions", "alpha.ts"));
  assert.notEqual(outputResult.status, 0);
  assert.match(
    outputResult.stderr,
    /solution manifest path must not clobber generated solution outputs/,
  );
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");

  const tsvPath = path.join(compileDir, "entries.tsv");
  fs.writeFileSync(
    tsvPath,
    manifestRows([["solutions/alpha.ts", "en/alpha.md", SOURCE_SHA, "1", "easy", "Alpha"]]),
    "utf8",
  );

  const missingParent = path.join(compileDir, "missing", "manifest.json");
  const missingParentResult = runManifest(tsvPath, missingParent);
  assert.notEqual(missingParentResult.status, 0);
  assert.match(
    missingParentResult.stderr,
    /solution manifest parent directory does not exist/,
  );

  const directoryManifest = path.join(compileDir, "manifest-dir");
  fs.mkdirSync(directoryManifest);
  const directoryResult = runManifest(tsvPath, directoryManifest);
  assert.notEqual(directoryResult.status, 0);
  assert.match(directoryResult.stderr, /solution manifest path is not a file/);
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

  const outputTraversal = path.join(compileDir, "output-traversal.tsv");
  fs.writeFileSync(
    outputTraversal,
    manifestRows([["../outside.ts", "en/alpha.md", SOURCE_SHA, "1", "easy", "Alpha"]]),
    "utf8",
  );
  const outputResult = runManifest(outputTraversal, manifestPath);
  assert.notEqual(outputResult.status, 0);
  assert.match(outputResult.stderr, /unsafe manifest output path: \.\.\/outside\.ts/);

  const sourceTraversal = path.join(compileDir, "source-traversal.tsv");
  fs.writeFileSync(
    sourceTraversal,
    manifestRows([["solutions/alpha.ts", "../alpha.md", SOURCE_SHA, "1", "easy", "Alpha"]]),
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

  const nonMarkdownSource = path.join(compileDir, "non-markdown-source.tsv");
  fs.writeFileSync(
    nonMarkdownSource,
    manifestRows([["solutions/alpha.ts", "en/alpha.txt", SOURCE_SHA, "1", "easy", "Alpha"]]),
    "utf8",
  );
  const result = runManifest(nonMarkdownSource, manifestPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /solution source must be a Markdown file: en\/alpha\.txt/);
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions", "alpha.ts"), { recursive: true });

  const directoryOutput = path.join(compileDir, "directory-output.tsv");
  fs.writeFileSync(
    directoryOutput,
    manifestRows([["solutions/alpha.ts", "en/alpha.md", SOURCE_SHA, "1", "easy", "Alpha"]]),
    "utf8",
  );
  const result = runManifest(directoryOutput, manifestPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /manifest output is not a file: solutions\/alpha\.ts/);
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");

  const badLevel = path.join(compileDir, "bad-level.tsv");
  fs.writeFileSync(
    badLevel,
    manifestRows([["solutions/alpha.ts", "en/alpha.md", SOURCE_SHA, "1", "weird", "Alpha"]]),
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

  const badSourceHash = path.join(compileDir, "bad-source-hash.tsv");
  fs.writeFileSync(
    badSourceHash,
    manifestRows([["solutions/alpha.ts", "en/alpha.md", "not-a-hash", "1", "easy", "Alpha"]]),
    "utf8",
  );
  const result = runManifest(badSourceHash, manifestPath);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /sourceSha256 must be a lowercase sha256 hex digest/);
});

withTempDir((dir) => {
  const compileDir = path.join(dir, "compile");
  const manifestPath = path.join(compileDir, "type-challenges-solutions-manifest.json");
  fs.mkdirSync(path.join(compileDir, "solutions"), { recursive: true });
  fs.writeFileSync(path.join(compileDir, "solutions", "alpha.ts"), "type Alpha = string;\n");
  fs.writeFileSync(path.join(compileDir, "solutions", "beta.ts"), "type Beta = string;\n");

  const duplicateOutput = path.join(compileDir, "duplicate-output.tsv");
  fs.writeFileSync(
    duplicateOutput,
    manifestRows([
      ["solutions/alpha.ts", "en/alpha.md", SOURCE_SHA, "1", "easy", "Alpha"],
      ["solutions/alpha.ts", "en/beta.md", SOURCE_SHA, "2", "easy", "Beta"],
    ]),
    "utf8",
  );
  const outputResult = runManifest(duplicateOutput, manifestPath);
  assert.notEqual(outputResult.status, 0);
  assert.match(
    outputResult.stderr,
    /duplicate Type Challenges solution output solutions\/alpha\.ts/,
  );

  const duplicateSource = path.join(compileDir, "duplicate-source.tsv");
  fs.writeFileSync(
    duplicateSource,
    manifestRows([
      ["solutions/alpha.ts", "en/alpha.md", SOURCE_SHA, "1", "easy", "Alpha"],
      ["solutions/beta.ts", "en/alpha.md", SOURCE_SHA, "2", "easy", "Beta"],
    ]),
    "utf8",
  );
  const sourceResult = runManifest(duplicateSource, manifestPath);
  assert.notEqual(sourceResult.status, 0);
  assert.match(
    sourceResult.stderr,
    /duplicate Type Challenges solution source en\/alpha\.md/,
  );
});
