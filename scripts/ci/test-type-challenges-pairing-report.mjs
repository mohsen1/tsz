#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const REPORT_SCRIPT = path.join(
  ROOT,
  "scripts",
  "ci",
  "type-challenges-pairing-report.mjs",
);

function withTempDir(fn) {
  const dir = fs.mkdtempSync(
    path.join(os.tmpdir(), "tsz-type-challenges-pairing-"),
  );
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function manifest(sourcePath, entries, extraEntryFields = () => ({}), options = {}) {
  return {
    fixture: options.fixture || "type-challenges-project",
    source: {
      repository: options.repository || "https://example.invalid/repo.git",
      ref: options.ref || "fixture-ref",
      path: sourcePath,
    },
    expectedGenerated: entries.length,
    generated: entries.length,
    entries: entries.map(({ id, level = "medium", slug = `case-${id}`, source }) => {
      return {
        output: source,
        source,
        challenge: { id, level, slug },
        ...extraEntryFields({ id, level, slug, source }),
      };
    }),
  };
}

function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest("questions/**/template.ts", [
      { id: "2", source: "questions/00002-medium-return-type/template.ts" },
      {
        id: "13",
        level: "warm",
        source: "questions/00013-warm-hello-world/template.ts",
      },
      {
        id: "189",
        level: "easy",
        source: "questions/00189-easy-awaited/template.ts",
      },
    ]),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "2", source: "questions/00002-medium-return-type/test-cases.ts" },
      {
        id: "13",
        level: "warm",
        source: "questions/00013-warm-hello-world/test-cases.ts",
      },
      {
        id: "189",
        level: "easy",
        source: "questions/00189-easy-awaited/test-cases.ts",
      },
    ]),
  );
  writeJson(
    solutions,
    manifest(
      "en/*.md",
      [
        { id: "13", level: "warm", source: "en/hello-world.md" },
        { id: "189", level: "easy", source: "en/awaited.md" },
      ],
      ({ id }) => ({
        declarations: id === "13" ? ["HelloWorld"] : ["MyAwaited"],
      }),
      { fixture: "type-challenges-solutions-project" },
    ),
  );

  const result = spawnSync(
    process.execPath,
    [REPORT_SCRIPT, templates, testCases, solutions, output],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const report = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(report.fixture, "type-challenges-readiness-pairing");
  assert.equal(report.counts.templates, 3);
  assert.equal(report.counts.testCases, 3);
  assert.equal(report.counts.solutions, 2);
  assert.equal(report.counts.pairedSolutions, 2);
  assert.equal(report.counts.solutionsMissingTemplates, 0);
  assert.equal(report.counts.solutionsMissingTestCases, 0);
  assert.equal(report.counts.testCasesMissingSolutions, 1);
  assert.deepEqual(
    report.pairedSolutions.map((entry) => [
      entry.id,
      entry.solution.source,
      entry.solution.declarations,
      entry.template.source,
      entry.testCase.source,
    ]),
    [
      [
        "13",
        "en/hello-world.md",
        ["HelloWorld"],
        "questions/00013-warm-hello-world/template.ts",
        "questions/00013-warm-hello-world/test-cases.ts",
      ],
      [
        "189",
        "en/awaited.md",
        ["MyAwaited"],
        "questions/00189-easy-awaited/template.ts",
        "questions/00189-easy-awaited/test-cases.ts",
      ],
    ],
  );
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest("questions/**/template.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/template.ts" },
    ]),
  );
  writeJson(
    testCases,
    manifest(
      "questions/**/test-cases.ts",
      [{ id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" }],
      () => ({}),
      { ref: "different-fixture-ref" },
    ),
  );
  writeJson(
    solutions,
    manifest(
      "en/*.md",
      [{ id: "13", level: "warm", source: "en/hello-world.md" }],
      () => ({ declarations: ["HelloWorld"] }),
      { fixture: "type-challenges-solutions-project" },
    ),
  );

  const result = spawnSync(
    process.execPath,
    [REPORT_SCRIPT, templates, testCases, solutions, output],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /template and test-case manifests must come from the same official snapshot/,
  );
  assert.equal(fs.existsSync(output), false);
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest("questions/**/template.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/template.ts" },
    ]),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest(
      "en/*.md",
      [{ id: "13", level: "warm", source: "en/hello-world.md" }],
      () => ({ declarations: ["HelloWorld"] }),
      { fixture: "type-challenges-project" },
    ),
  );

  const result = spawnSync(
    process.execPath,
    [REPORT_SCRIPT, templates, testCases, solutions, output],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /unexpected solution manifest fixture/);
  assert.equal(fs.existsSync(output), false);
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");
  const solutionManifest = manifest(
    "en/*.md",
    [{ id: "13", level: "warm", source: "en/hello-world.md" }],
    () => ({ declarations: ["HelloWorld"] }),
    { fixture: "type-challenges-solutions-project" },
  );
  delete solutionManifest.generated;

  writeJson(
    templates,
    manifest("questions/**/template.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/template.ts" },
    ]),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
    ]),
  );
  writeJson(solutions, solutionManifest);

  const result = spawnSync(
    process.execPath,
    [REPORT_SCRIPT, templates, testCases, solutions, output],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 1);
  assert.match(result.stderr, /solution manifest is missing generated count metadata/);
  assert.equal(fs.existsSync(output), false);
});
