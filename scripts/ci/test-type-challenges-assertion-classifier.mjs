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
  "type-challenges-assertion-classifier.mjs",
);

function withTempDir(fn) {
  const dir = fs.mkdtempSync(
    path.join(os.tmpdir(), "tsz-type-challenges-classifier-"),
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

withTempDir((dir) => {
  const candidates = path.join(dir, "assertions");
  const manifest = path.join(candidates, "type-challenges-assertions-manifest.json");
  const output = path.join(candidates, "type-challenges-assertions-classification.json");
  const fakeTsc = path.join(dir, "fake-tsc.js");
  const fakeTsz = path.join(dir, "fake-tsz.js");

  writeJson(path.join(candidates, "tsconfig.tsz-guard.json"), {
    compilerOptions: { noEmit: true },
    include: ["assertions/**/*.ts"],
  });
  writeFile(
    path.join(candidates, "assertions", "one.ts"),
    [
      "type Parse<T extends string> = T extends `${infer Head}.${infer Tail}`",
      "  ? [Head, ...Parse<Tail>]",
      "  : [T];",
      "",
    ].join("\n"),
  );
  writeFile(
    path.join(candidates, "assertions", "two.ts"),
    [
      "type Remap<T> = {",
      "  [K in keyof T as `get${Capitalize<string & K>}`]: T[K];",
      "};",
      "",
    ].join("\n"),
  );
  writeJson(manifest, {
    fixture: "type-challenges-assertion-candidates",
    counts: {
      pairedSolutions: 2,
      generatedAssertions: 2,
      assertionsReferencingSolutionDeclaration: 1,
      assertionsMissingSolutionDeclarationReference: 1,
    },
    entries: [
      {
        id: "one",
        output: "assertions/one.ts",
      },
      {
        id: "two",
        output: "assertions/two.ts",
      },
    ],
  });
  writeExecutable(
    fakeTsc,
    [
      "#!/usr/bin/env node",
      "console.error(\"assertions/one.ts(1,1): error TS2344: mismatch\")",
      "console.error(\"assertions/two.ts(2,3): error TS2304: missing\")",
      "process.exit(1)",
      "",
    ].join("\n"),
  );
  writeExecutable(
    fakeTsz,
    [
      "#!/usr/bin/env node",
      "console.log(\"ok\")",
      "process.exit(0)",
      "",
    ].join("\n"),
  );

  const result = spawnSync(process.execPath, [SCRIPT, candidates, manifest, output], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TYPE_CHALLENGES_ASSERTION_TSC_BIN: fakeTsc,
      TSZ_BIN: fakeTsz,
      TYPE_CHALLENGES_ASSERTION_CLASSIFIER_TIMEOUT_MS: "5000",
    },
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.fixture, "type-challenges-assertion-classification");
  assert.deepEqual(report.candidateManifest.counts, {
    pairedSolutions: 2,
    generatedAssertions: 2,
    assertionsReferencingSolutionDeclaration: 1,
    assertionsMissingSolutionDeclarationReference: 1,
  });
  assert.deepEqual(
    report.candidateManifest.semanticFamilies.map((entry) => [
      entry.family,
      entry.candidateCount,
      entry.files,
    ]),
    [
      ["template literal inference", 2, ["assertions/one.ts", "assertions/two.ts"]],
      ["distributive conditionals", 1, ["assertions/one.ts"]],
      ["indexed access", 1, ["assertions/two.ts"]],
      ["inference cache/session behavior", 1, ["assertions/one.ts"]],
      ["mapped/key-remapped types", 1, ["assertions/two.ts"]],
      ["recursive conditionals", 1, ["assertions/one.ts"]],
      ["tuple recursion", 1, ["assertions/one.ts"]],
    ],
  );
  assert.equal(report.compilers.tsc.status, "fail");
  assert.equal(report.compilers.tsc.exitCode, 1);
  assert.equal(report.compilers.tsc.diagnostics.errorCount, 2);
  assert.deepEqual(report.compilers.tsc.diagnostics.byCode, [
    { key: "TS2304", count: 1 },
    { key: "TS2344", count: 1 },
  ]);
  assert.deepEqual(report.compilers.tsc.diagnostics.byFile, [
    { key: "assertions/one.ts", count: 1 },
    { key: "assertions/two.ts", count: 1 },
  ]);
  assert.deepEqual(
    report.compilers.tsc.diagnostics.bySemanticFamily.map((entry) => [
      entry.family,
      entry.errorCount,
      entry.files,
    ]),
    [
      ["template literal inference", 2, ["assertions/one.ts", "assertions/two.ts"]],
      ["distributive conditionals", 1, ["assertions/one.ts"]],
      ["indexed access", 1, ["assertions/two.ts"]],
      ["inference cache/session behavior", 1, ["assertions/one.ts"]],
      ["mapped/key-remapped types", 1, ["assertions/two.ts"]],
      ["recursive conditionals", 1, ["assertions/one.ts"]],
      ["tuple recursion", 1, ["assertions/one.ts"]],
    ],
  );
  assert.deepEqual(report.compilers.tsc.command.slice(1), [
    "--noEmit",
    "-p",
    path.join(candidates, "tsconfig.tsz-guard.json"),
    "--pretty",
    "false",
  ]);
  assert.equal(report.compilers.tsz.status, "pass");
  assert.equal(report.compilers.tsz.exitCode, 0);
  assert.deepEqual(report.comparison, {
    status: "tsz-accepts-tsc-rejected",
    tscStatus: "fail",
    tszStatus: "pass",
    errorCountDelta: -2,
    byCodeDelta: [
      { key: "TS2304", tsc: 1, tsz: 0, delta: -1 },
      { key: "TS2344", tsc: 1, tsz: 0, delta: -1 },
    ],
  });
});

withTempDir((dir) => {
  const candidates = path.join(dir, "assertions");
  const manifest = path.join(candidates, "type-challenges-assertions-manifest.json");
  const output = path.join(candidates, "type-challenges-assertions-classification.json");

  writeJson(path.join(candidates, "tsconfig.tsz-guard.json"), {
    compilerOptions: { noEmit: true },
  });
  writeJson(manifest, {
    fixture: "type-challenges-assertion-candidates",
    counts: {},
  });

  const result = spawnSync(process.execPath, [SCRIPT, candidates, manifest, output], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TYPE_CHALLENGES_ASSERTION_TSC_BIN: "",
      TSZ_BIN: "",
    },
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.compilers.tsc.status, "unavailable");
  assert.deepEqual(report.compilers.tsc.diagnostics.bySemanticFamily, []);
  assert.equal(report.compilers.tsz.status, "unavailable");
  assert.deepEqual(report.compilers.tsz.diagnostics.bySemanticFamily, []);
  assert.deepEqual(report.comparison, {
    status: "unavailable",
    tscStatus: "unavailable",
    tszStatus: "unavailable",
    errorCountDelta: null,
    byCodeDelta: [],
  });
});

withTempDir((dir) => {
  const candidates = path.join(dir, "assertions");
  const manifest = path.join(candidates, "type-challenges-assertions-manifest.json");
  const output = path.join(candidates, "type-challenges-assertions-classification.json");
  const fakeTsc = path.join(dir, "fake-tsc.js");
  const fakeTsz = path.join(dir, "fake-tsz.js");

  writeJson(path.join(candidates, "tsconfig.tsz-guard.json"), {
    compilerOptions: { noEmit: true },
  });
  writeJson(manifest, {
    fixture: "type-challenges-assertion-candidates",
    counts: {},
    entries: [],
  });
  writeExecutable(
    fakeTsc,
    ["#!/usr/bin/env node", "process.exit(0)", ""].join("\n"),
  );
  writeExecutable(
    fakeTsz,
    [
      "#!/usr/bin/env node",
      "console.error(\"assertions/three.ts(1,1): error TS2589: deep\")",
      "console.error(\"assertions/three.ts(2,1): error TS2589: deep again\")",
      "process.exit(1)",
      "",
    ].join("\n"),
  );

  const result = spawnSync(process.execPath, [SCRIPT, candidates, manifest, output], {
    cwd: ROOT,
    encoding: "utf8",
    env: {
      ...process.env,
      TYPE_CHALLENGES_ASSERTION_TSC_BIN: fakeTsc,
      TSZ_BIN: fakeTsz,
    },
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.deepEqual(report.comparison, {
    status: "tsz-rejects-tsc-accepted",
    tscStatus: "pass",
    tszStatus: "fail",
    errorCountDelta: 2,
    byCodeDelta: [{ key: "TS2589", tsc: 0, tsz: 2, delta: 2 }],
  });
});
