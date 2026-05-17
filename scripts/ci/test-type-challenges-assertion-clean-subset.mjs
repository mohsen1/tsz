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
        template: { output: "questions/00014-easy-first/template.ts" },
        testCase: {
          output: "questions/00014-easy-first/test-cases.ts",
          source: "questions/00014-easy-first/test-cases.ts",
        },
        assertion: {
          hasReferencedSolutionDeclaration: true,
          referencedSolutionDeclarations: ["First"],
        },
      },
      {
        id: "189",
        output: "assertions/189-easy-awaited.ts",
        solution: { output: "solutions/easy-awaited.ts", source: "en/easy-awaited.md" },
        template: { output: "questions/00189-easy-awaited/template.ts" },
        testCase: {
          output: "questions/00189-easy-awaited/test-cases.ts",
          source: "questions/00189-easy-awaited/test-cases.ts",
        },
        assertion: {
          hasReferencedSolutionDeclaration: false,
          referencedSolutionDeclarations: [],
        },
      },
    ],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 2 },
    },
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
    tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
    tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
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
  writeFile(
    path.join(candidateDir, "assertions", "189-easy-awaited.ts"),
    "type Awaited<T> = T;\ntype cases = [];\n",
  );
  writeJson(candidateManifestPath, {
    fixture: "type-challenges-assertion-candidates",
    counts: {
      generatedAssertions: 1,
      assertionsReferencingSolutionDeclaration: 0,
      assertionsMissingSolutionDeclarationReference: 1,
    },
    entries: [
      {
        id: "189",
        output: "assertions/189-easy-awaited.ts",
        solution: { output: "solutions/easy-awaited.ts", source: "en/easy-awaited.md" },
        testCase: {
          output: "questions/00189-easy-awaited/test-cases.ts",
          source: "questions/00189-easy-awaited/test-cases.ts",
        },
        assertion: {
          hasReferencedSolutionDeclaration: false,
          referencedSolutionDeclarations: [],
        },
      },
    ],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 1 },
    },
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          totalCandidates: 1,
          candidatesWithDiagnostics: 0,
          candidatesWithoutDiagnostics: 1,
          filesWithDiagnostics: [],
          filesWithoutDiagnostics: ["assertions/189-easy-awaited.ts"],
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
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const manifest = JSON.parse(fs.readFileSync(subsetManifestPath, "utf8"));
  assert.deepEqual(manifest.counts, {
    totalCandidates: 1,
    tscAcceptedAssertions: 1,
    tscAcceptedAssertionsReferencingSolutionDeclaration: 0,
    tscAcceptedAssertionsMissingSolutionDeclarationReference: 1,
    tscRejectedAssertions: 0,
    missingAcceptedManifestEntries: 0,
  });
  assert.deepEqual(
    manifest.entries.map((entry) => entry.assertion.hasReferencedSolutionDeclaration),
    [false],
  );
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
    counts: { generatedAssertions: 1 },
    entries: [
      {
        id: "14",
        output: "assertions/14-easy-first.ts",
        solution: { output: "solutions/easy-first.ts", source: "en/easy-first.md" },
        assertion: {
          hasReferencedSolutionDeclaration: true,
          referencedSolutionDeclarations: ["First"],
        },
      },
    ],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 1 },
    },
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          filesWithoutDiagnostics: ["assertions/14-easy-first.ts"],
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
  assert.match(result.stderr, /selected entries\[0\]\.testCase\.output/);
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
    counts: { generatedAssertions: 1 },
    entries: [{ id: "14", output: "assertions/14-easy-first.ts" }],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 2 },
    },
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          filesWithoutDiagnostics: ["assertions/14-easy-first.ts"],
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
    /classification candidate manifest counts\.generatedAssertions/,
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
    fixture: "stale-candidates",
    counts: { generatedAssertions: 0 },
    entries: [],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    compilers: {
      tsc: { status: "pass", candidateDiagnostics: { filesWithoutDiagnostics: [] } },
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
  assert.match(result.stderr, /unexpected assertion candidate manifest fixture/);
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
    counts: { generatedAssertions: 2 },
    entries: [
      { id: "14", output: "assertions/14-easy-first.ts" },
      { id: "14-copy", output: "assertions/14-easy-first.ts" },
    ],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          filesWithoutDiagnostics: ["assertions/14-easy-first.ts"],
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
    /assertion candidate manifest reported duplicate candidate outputs:[\s\S]*assertions\/14-easy-first\.ts/,
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
    counts: { generatedAssertions: 1 },
    entries: [{ id: "escape", output: "../outside.ts" }],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 1 },
    },
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          filesWithoutDiagnostics: ["../outside.ts"],
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
  assert.match(result.stderr, /must stay inside the assertion candidate directory/);
  assert.equal(fs.existsSync(subsetManifestPath), false);
  assert.equal(fs.existsSync(path.join(dir, "outside.ts")), false);
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
    counts: { generatedAssertions: 2 },
    entries: [
      { id: "14", output: "assertions/14-easy-first.ts" },
      { id: "14-copy", output: "assertions/14-easy-first.ts" },
    ],
  });
  writeJson(classificationPath, {
    fixture: "type-challenges-assertion-classification",
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 2 },
    },
    compilers: {
      tsc: {
        status: "pass",
        candidateDiagnostics: {
          filesWithoutDiagnostics: ["assertions/14-easy-first.ts"],
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
  assert.match(result.stderr, /duplicate candidate outputs/);
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
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 1 },
    },
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
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 1 },
    },
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
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 1 },
    },
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
    candidateManifest: {
      fixture: "type-challenges-assertion-candidates",
      counts: { generatedAssertions: 0 },
    },
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
  assert.equal(result.status, 1);
  assert.match(result.stderr, /entries must include at least one assertion candidate/);
  assert.equal(fs.existsSync(subsetManifestPath), false);
});
