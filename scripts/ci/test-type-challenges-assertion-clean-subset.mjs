#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(
  ROOT,
  "scripts",
  "ci",
  "type-challenges-assertion-clean-subset.mjs",
);

function withTempDir(fn) {
  const dir = fs.mkdtempSync(
    path.join(os.tmpdir(), "tsz-type-challenges-clean-subset-"),
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

withTempDir((dir) => {
  const candidateDir = path.join(dir, "candidates");
  const outputDir = path.join(dir, "clean");
  const candidateManifestPath = path.join(candidateDir, "type-challenges-assertions-manifest.json");
  const classificationPath = path.join(candidateDir, "type-challenges-assertions-classification.json");
  const subsetManifestPath = path.join(outputDir, "type-challenges-assertions-tsc-clean-manifest.json");

  writeFile(
    path.join(candidateDir, "utils", "index.d.ts"),
    "export type Expect<T extends true> = T;\nexport type Equal<X, Y> = true;\n",
  );
  writeFile(
    path.join(candidateDir, "assertions", "14-easy-first.ts"),
    "type First<T extends unknown[]> = T[0];\ntype cases = [Expect<Equal<First<[1, 2]>, 1>>];\n",
  );
  writeFile(
    path.join(candidateDir, "assertions", "189-easy-awaited.ts"),
    "type Awaited<T> = T;\ntype cases = [Expect<Equal<MyAwaited<Promise<string>>, string>>];\n",
  );
  writeJson(candidateManifestPath, {
    fixture: "type-challenges-assertion-candidates",
    sources: {
      templates: { repository: "type", ref: "type-ref" },
      testCases: { repository: "type", ref: "type-ref" },
      solutions: { repository: "solutions", ref: "solutions-ref" },
    },
    counts: {
      pairedSolutions: 2,
      generatedAssertions: 2,
      assertionsReferencingSolutionDeclaration: 1,
      assertionsMissingSolutionDeclarationReference: 1,
    },
    entries: [
      {
        id: "14",
        output: "assertions/14-easy-first.ts",
        solution: { output: "solutions/easy-first.ts", source: "en/easy-first.md" },
        testCase: { output: "questions/00014-easy-first/test-cases.ts" },
        assertion: { hasReferencedSolutionDeclaration: true },
      },
      {
        id: "189",
        output: "assertions/189-easy-awaited.ts",
        solution: { output: "solutions/easy-awaited.ts", source: "en/easy-awaited.md" },
        testCase: { output: "questions/00189-easy-awaited/test-cases.ts" },
        assertion: { hasReferencedSolutionDeclaration: false },
      },
    ],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    compilers: {
      tsc: {
        status: "fail",
        candidateDiagnostics: {
          totalCandidates: 2,
          candidatesWithDiagnostics: 1,
          candidatesWithoutDiagnostics: 1,
          filesWithDiagnostics: ["assertions/189-easy-awaited.ts"],
          filesWithoutDiagnostics: ["assertions/14-easy-first.ts"],
        },
      },
      tsz: { status: "pass" },
    },
    comparison: { status: "tsz-accepts-tsc-rejected" },
  });

  const result = spawnSync(
    process.execPath,
    [SCRIPT, candidateDir, candidateManifestPath, classificationPath, outputDir, subsetManifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const manifest = JSON.parse(fs.readFileSync(subsetManifestPath, "utf8"));
  assert.equal(manifest.fixture, "type-challenges-assertions-tsc-clean");
  assert.deepEqual(manifest.sources, {
    templates: { repository: "type", ref: "type-ref" },
    testCases: { repository: "type", ref: "type-ref" },
    solutions: { repository: "solutions", ref: "solutions-ref" },
  });
  assert.deepEqual(manifest.counts, {
    totalCandidates: 2,
    tscAcceptedAssertions: 1,
    tscRejectedAssertions: 1,
    missingAcceptedManifestEntries: 0,
  });
  assert.deepEqual(
    manifest.entries.map((entry) => [entry.id, entry.output]),
    [["14", "assertions/14-easy-first.ts"]],
  );
  assert.equal(
    fs.existsSync(path.join(outputDir, "assertions", "14-easy-first.ts")),
    true,
  );
  assert.equal(
    fs.existsSync(path.join(outputDir, "assertions", "189-easy-awaited.ts")),
    false,
  );
  assert.equal(fs.existsSync(path.join(outputDir, "utils", "index.d.ts")), true);
  assert.equal(fs.existsSync(path.join(outputDir, "tsconfig.tsz-guard.json")), true);
});

withTempDir((dir) => {
  const candidateDir = path.join(dir, "candidates");
  const outputDir = path.join(dir, "clean");
  const candidateManifestPath = path.join(candidateDir, "type-challenges-assertions-manifest.json");
  const classificationPath = path.join(candidateDir, "type-challenges-assertions-classification.json");
  const subsetManifestPath = path.join(outputDir, "type-challenges-assertions-tsc-clean-manifest.json");

  writeFile(path.join(candidateDir, "utils", "index.d.ts"), "export {};\n");
  writeJson(candidateManifestPath, {
    fixture: "type-challenges-assertion-candidates",
    sources: {},
    counts: { generatedAssertions: 1 },
    entries: [{ id: "14", output: "assertions/14-easy-first.ts" }],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          filesWithoutDiagnostics: ["assertions/missing.ts"],
          filesWithDiagnostics: [],
        },
      },
      tsz: { status: "pass" },
    },
    comparison: { status: "both-pass" },
  });

  const result = spawnSync(
    process.execPath,
    [SCRIPT, candidateDir, candidateManifestPath, classificationPath, outputDir, subsetManifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classifier reported tsc-clean files missing from the candidate manifest/,
  );
  assert.equal(fs.existsSync(subsetManifestPath), false);
});

withTempDir((dir) => {
  const candidateDir = path.join(dir, "candidates");
  const outputDir = path.join(dir, "clean");
  const candidateManifestPath = path.join(candidateDir, "type-challenges-assertions-manifest.json");
  const classificationPath = path.join(candidateDir, "type-challenges-assertions-classification.json");
  const subsetManifestPath = path.join(outputDir, "type-challenges-assertions-tsc-clean-manifest.json");

  writeFile(path.join(candidateDir, "utils", "index.d.ts"), "export {};\n");
  writeJson(candidateManifestPath, {
    fixture: "type-challenges-assertion-candidates",
    sources: {},
    counts: { generatedAssertions: 1 },
    entries: [{ id: "14", output: "assertions/14-easy-first.ts" }],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          filesWithoutDiagnostics: [],
          filesWithDiagnostics: [],
        },
      },
      tsz: { status: "pass" },
    },
    comparison: { status: "both-pass" },
  });

  const result = spawnSync(
    process.execPath,
    [SCRIPT, candidateDir, candidateManifestPath, classificationPath, outputDir, subsetManifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /did not classify every candidate file with tsc diagnostics/,
  );
  assert.equal(fs.existsSync(subsetManifestPath), false);
});

withTempDir((dir) => {
  const candidateDir = path.join(dir, "candidates");
  const outputDir = path.join(dir, "clean");
  const candidateManifestPath = path.join(candidateDir, "type-challenges-assertions-manifest.json");
  const classificationPath = path.join(candidateDir, "type-challenges-assertions-classification.json");
  const subsetManifestPath = path.join(outputDir, "type-challenges-assertions-tsc-clean-manifest.json");

  writeFile(path.join(candidateDir, "utils", "index.d.ts"), "export {};\n");
  writeFile(path.join(candidateDir, "assertions", "14-easy-first.ts"), "export {};\n");
  writeJson(candidateManifestPath, {
    fixture: "type-challenges-assertion-candidates",
    sources: {},
    counts: { generatedAssertions: 1 },
    entries: [{ id: "14", output: "assertions/14-easy-first.ts" }],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    compilers: {
      tsc: {
        status: "fail",
        candidateDiagnostics: {
          filesWithoutDiagnostics: ["assertions/14-easy-first.ts"],
          filesWithDiagnostics: ["assertions/14-easy-first.ts"],
        },
      },
      tsz: { status: "pass" },
    },
    comparison: { status: "tsz-accepts-tsc-rejected" },
  });

  const result = spawnSync(
    process.execPath,
    [SCRIPT, candidateDir, candidateManifestPath, classificationPath, outputDir, subsetManifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /both tsc-clean and tsc-diagnostic/,
  );
  assert.equal(fs.existsSync(subsetManifestPath), false);
});

withTempDir((dir) => {
  const candidateDir = path.join(dir, "candidates");
  const outputDir = path.join(dir, "clean");
  const candidateManifestPath = path.join(candidateDir, "type-challenges-assertions-manifest.json");
  const classificationPath = path.join(candidateDir, "type-challenges-assertions-classification.json");
  const subsetManifestPath = path.join(outputDir, "type-challenges-assertions-tsc-clean-manifest.json");

  writeFile(path.join(candidateDir, "utils", "index.d.ts"), "export {};\n");
  writeJson(candidateManifestPath, {
    fixture: "type-challenges-assertion-candidates",
    sources: {},
    counts: { generatedAssertions: 0 },
    entries: [],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    compilers: {
      tsc: { status: "unavailable", candidateDiagnostics: { totalCandidates: 0 } },
      tsz: { status: "unavailable" },
    },
    comparison: { status: "unavailable" },
  });

  const result = spawnSync(
    process.execPath,
    [SCRIPT, candidateDir, candidateManifestPath, classificationPath, outputDir, subsetManifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const manifest = JSON.parse(fs.readFileSync(subsetManifestPath, "utf8"));
  assert.equal(manifest.counts.tscAcceptedAssertions, 0);
  assert.equal(manifest.counts.tscRejectedAssertions, null);
  assert.deepEqual(manifest.entries, []);
  assert.equal(fs.existsSync(path.join(outputDir, "tsconfig.tsz-guard.json")), true);
});
