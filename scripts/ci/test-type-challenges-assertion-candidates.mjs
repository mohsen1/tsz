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
  "type-challenges-assertion-candidates.mjs",
);

function withTempDir(fn) {
  const dir = fs.mkdtempSync(
    path.join(os.tmpdir(), "tsz-type-challenges-assertions-"),
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

function basePairingReport(overrides = {}) {
  return {
    fixture: "type-challenges-readiness-pairing",
    sources: {
      templates: { repository: "type", ref: "type-ref" },
      testCases: { repository: "type", ref: "type-ref" },
      solutions: { repository: "solutions", ref: "solutions-ref" },
    },
    counts: {
      pairedSolutions: 2,
    },
    pairedSolutions: [
      {
        id: "14",
        solution: {
          output: "solutions/easy-first.ts",
          source: "en/easy-first.md",
          challenge: { id: "14", level: "easy", slug: "first" },
          declarations: ["First"],
        },
        template: {
          output: "questions/00014-easy-first/template.ts",
          source: "questions/00014-easy-first/template.ts",
          challenge: { id: "14", level: "easy", slug: "first" },
        },
        testCase: {
          output: "questions/00014-easy-first/test-cases.ts",
          source: "questions/00014-easy-first/test-cases.ts",
          challenge: { id: "14", level: "easy", slug: "first" },
        },
      },
      {
        id: "189",
        solution: {
          output: "solutions/easy-awaited.ts",
          source: "en/easy-awaited.md",
          challenge: { id: "189", level: "easy", slug: "awaited" },
          declarations: ["Awaited"],
        },
        template: {
          output: "questions/00189-easy-awaited/template.ts",
          source: "questions/00189-easy-awaited/template.ts",
          challenge: { id: "189", level: "easy", slug: "awaited" },
        },
        testCase: {
          output: "questions/00189-easy-awaited/test-cases.ts",
          source: "questions/00189-easy-awaited/test-cases.ts",
          challenge: { id: "189", level: "easy", slug: "awaited" },
        },
      },
    ],
    ...overrides,
  };
}

withTempDir((dir) => {
  const typeCompile = path.join(dir, "type-challenges", ".tsz-compile");
  const solutionsCompile = path.join(
    dir,
    "type-challenges-solutions",
    ".tsz-compile",
  );
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  const pairingPath = path.join(dir, "pairing.json");

  writeFile(
    path.join(typeCompile, "utils", "index.d.ts"),
    "export type Expect<T extends true> = T;\nexport type Equal<X, Y> = true;\n",
  );
  writeFile(
    path.join(solutionsCompile, "solutions", "easy-first.ts"),
    "type First<T extends unknown[]> = T[0];\nexport {};\n",
  );
  writeFile(
    path.join(
      typeCompile,
      "test-cases",
      "questions",
      "00014-easy-first",
      "test-cases.ts",
    ),
    "import type { Equal, Expect } from '@type-challenges/utils'\ntype cases = [Expect<Equal<First<[1, 2]>, 1>>]\n",
  );
  writeFile(
    path.join(solutionsCompile, "solutions", "easy-awaited.ts"),
    "type Awaited<T> = T;\nexport {};\n",
  );
  writeFile(
    path.join(
      typeCompile,
      "test-cases",
      "questions",
      "00189-easy-awaited",
      "test-cases.ts",
    ),
    "import type { Equal, Expect } from '@type-challenges/utils'\ntype cases = [Expect<Equal<MyAwaited<Promise<string>>, string>>]\n",
  );

  writeJson(pairingPath, basePairingReport());

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, typeCompile, solutionsCompile, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  assert.equal(manifest.fixture, "type-challenges-assertion-candidates");
  assert.deepEqual(manifest.counts, {
    pairedSolutions: 2,
    generatedAssertions: 2,
    assertionsReferencingSolutionDeclaration: 1,
    assertionsMissingSolutionDeclarationReference: 1,
  });
  assert.deepEqual(
    manifest.entries.map((entry) => [
      entry.id,
      entry.output,
      entry.assertion.referencedSolutionDeclarations,
      entry.assertion.hasReferencedSolutionDeclaration,
    ]),
    [
      ["14", "assertions/14-easy-first.ts", ["First"], true],
      ["189", "assertions/189-easy-awaited.ts", [], false],
    ],
  );

  const firstCandidate = fs.readFileSync(
    path.join(outputDir, "assertions", "14-easy-first.ts"),
    "utf8",
  );
  assert.match(firstCandidate, /type First<T extends unknown\[]>/);
  assert.match(firstCandidate, /Expect<Equal<First<\[1, 2\]>, 1>>/);
  assert.ok(fs.existsSync(path.join(outputDir, "tsconfig.tsz-guard.json")));
  assert.ok(fs.existsSync(path.join(outputDir, "utils", "index.d.ts")));
});

withTempDir((dir) => {
  const pairingPath = path.join(dir, "pairing.json");
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  writeJson(pairingPath, basePairingReport({ fixture: "stale-pairing" }));

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, dir, dir, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /unexpected Type Challenges pairing report fixture/);
  assert.equal(fs.existsSync(manifestPath), false);
});

withTempDir((dir) => {
  const pairingPath = path.join(dir, "pairing.json");
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  writeJson(
    pairingPath,
    basePairingReport({
      counts: {
        pairedSolutions: 1,
      },
    }),
  );

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, dir, dir, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /pairing report count metadata is inconsistent/);
  assert.equal(fs.existsSync(manifestPath), false);
});

withTempDir((dir) => {
  const pairingPath = path.join(dir, "pairing.json");
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  writeJson(
    pairingPath,
    basePairingReport({
      sources: {
        templates: { repository: "type", ref: "" },
        testCases: { repository: "type", ref: "type-ref" },
        solutions: { repository: "solutions", ref: "solutions-ref" },
      },
    }),
  );

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, dir, dir, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /missing templates source metadata/);
  assert.equal(fs.existsSync(manifestPath), false);
});

withTempDir((dir) => {
  const pairingPath = path.join(dir, "pairing.json");
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  const report = basePairingReport();
  report.pairedSolutions[0].testCase.challenge = {
    id: "189",
    level: "easy",
    slug: "awaited",
  };
  writeJson(pairingPath, report);

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, dir, dir, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /testCase challenge id mismatch/);
  assert.equal(fs.existsSync(manifestPath), false);
});

withTempDir((dir) => {
  const pairingPath = path.join(dir, "pairing.json");
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  writeJson(
    pairingPath,
    basePairingReport({
      pairedSolutions: [
        {
          id: "14",
          solution: {
            output: "solutions/easy-first.ts",
            source: "en/easy-first.md",
            challenge: { id: "14", level: "easy", slug: "first" },
            declarations: [],
          },
          template: {
            output: "questions/00014-easy-first/template.ts",
            source: "questions/00014-easy-first/template.ts",
            challenge: { id: "14", level: "easy", slug: "first" },
          },
          testCase: {
            output: "questions/00014-easy-first/test-cases.ts",
            source: "questions/00014-easy-first/test-cases.ts",
            challenge: { id: "14", level: "easy", slug: "first" },
          },
        },
      ],
      counts: { pairedSolutions: 1 },
    }),
  );

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, dir, dir, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /pair 0 has no solution declarations/);
  assert.equal(fs.existsSync(manifestPath), false);
});

withTempDir((dir) => {
  const pairingPath = path.join(dir, "pairing.json");
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  writeJson(
    pairingPath,
    basePairingReport({
      pairedSolutions: [
        {
          id: "14",
          solution: {
            output: "../solutions/easy-first.ts",
            source: "en/easy-first.md",
            challenge: { id: "14", level: "easy", slug: "first" },
            declarations: ["First"],
          },
          template: {
            output: "questions/00014-easy-first/template.ts",
            source: "questions/00014-easy-first/template.ts",
            challenge: { id: "14", level: "easy", slug: "first" },
          },
          testCase: {
            output: "questions/00014-easy-first/test-cases.ts",
            source: "questions/00014-easy-first/test-cases.ts",
            challenge: { id: "14", level: "easy", slug: "first" },
          },
        },
      ],
      counts: { pairedSolutions: 1 },
    }),
  );

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, dir, dir, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /solution\.output must be a relative path/);
  assert.equal(fs.existsSync(manifestPath), false);
});

withTempDir((dir) => {
  const pairingPath = path.join(dir, "pairing.json");
  const outputDir = path.join(dir, "assertions");
  const manifestPath = path.join(outputDir, "type-challenges-assertions-manifest.json");
  writeJson(
    pairingPath,
    basePairingReport({
      pairedSolutions: [
        {
          id: "14",
          solution: {
            output: "solutions/easy-first.ts",
            source: "en/easy-first.md",
            challenge: { id: "14", level: "easy", slug: "first" },
            declarations: ["First"],
          },
          template: {
            output: "questions/00014-easy-first/template.ts",
            source: "questions/00014-easy-first/template.ts",
            challenge: { id: "14", level: "easy", slug: "first" },
          },
          testCase: {
            output: "questions/00014-easy-first/test-cases.ts",
            source: "questions/00014-easy-first/test-cases.ts",
            challenge: { id: "14", level: "easy", slug: "first" },
          },
        },
        {
          id: "14!",
          solution: {
            output: "solutions/easy-first-alias.ts",
            source: "en/easy-first.md",
            challenge: { id: "14!", level: "easy", slug: "first" },
            declarations: ["FirstAlias"],
          },
          template: {
            output: "questions/00014-easy-first/template.ts",
            source: "questions/00014-easy-first/template.ts",
            challenge: { id: "14!", level: "easy", slug: "first" },
          },
          testCase: {
            output: "questions/00014-easy-first/test-cases.ts",
            source: "questions/00014-easy-first/test-cases.ts",
            challenge: { id: "14!", level: "easy", slug: "first" },
          },
        },
      ],
      counts: { pairedSolutions: 2 },
    }),
  );

  const result = spawnSync(
    process.execPath,
    [SCRIPT, pairingPath, dir, dir, outputDir, manifestPath],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /duplicate Type Challenges assertion candidate output/);
  assert.equal(fs.existsSync(manifestPath), false);
});
