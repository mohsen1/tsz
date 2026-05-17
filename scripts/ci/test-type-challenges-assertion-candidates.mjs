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

  writeJson(pairingPath, {
    fixture: "type-challenges-readiness-pairing",
    sources: {
      templates: { repository: "type", ref: "type-ref" },
      testCases: { repository: "type", ref: "type-ref" },
      solutions: { repository: "solutions", ref: "solutions-ref" },
    },
    pairedSolutions: [
      {
        id: "14",
        solution: {
          output: "solutions/easy-first.ts",
          source: "en/easy-first.md",
          declarations: ["First"],
        },
        template: { output: "questions/00014-easy-first/template.ts" },
        testCase: {
          output: "questions/00014-easy-first/test-cases.ts",
          source: "questions/00014-easy-first/test-cases.ts",
        },
      },
      {
        id: "189",
        solution: {
          output: "solutions/easy-awaited.ts",
          source: "en/easy-awaited.md",
          declarations: ["Awaited"],
        },
        template: { output: "questions/00189-easy-awaited/template.ts" },
        testCase: {
          output: "questions/00189-easy-awaited/test-cases.ts",
          source: "questions/00189-easy-awaited/test-cases.ts",
        },
      },
    ],
  });

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
