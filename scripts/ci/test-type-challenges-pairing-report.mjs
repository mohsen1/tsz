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

function manifest(
  sourcePath,
  entries,
  extraEntryFields = () => ({}),
  sourceOverrides = {},
  manifestOverrides = {},
) {
  return {
    fixture: sourcePath === "en/*.md"
      ? "type-challenges-solutions-project"
      : "type-challenges-project",
    source: {
      repository: "https://example.invalid/repo.git",
      ref: "fixture-ref",
      path: sourcePath,
      ...sourceOverrides,
    },
    expectedGenerated: entries.length,
    generated: entries.length,
    entries: entries.map(({ id, level = "medium", slug = `case-${id}`, source }) => {
      const extra = extraEntryFields({ id, level, slug, source });
      const defaultDeclarations = sourcePath === "en/*.md" &&
        !Object.prototype.hasOwnProperty.call(extra, "declarations")
        ? { declarations: [`Solution${id}`] }
        : {};
      return {
        output: source,
        source,
        challenge: { id, level, slug },
        ...defaultDeclarations,
        ...extra,
      };
    }),
    ...manifestOverrides,
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
      { ref: "different-official-ref" },
    ),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [{ id: "13", source: "en/hello-world.md" }]),
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
    /template and test-case manifests come from different source snapshots/,
  );
  assert.ok(!fs.existsSync(output));
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest(
      "questions/**/template.ts",
      [{ id: "13", source: "questions/00013-warm-hello-world/template.ts" }],
      () => ({}),
      { ref: "" },
    ),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [{ id: "13", source: "en/hello-world.md" }]),
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
  assert.match(result.stderr, /<missing ref>/);
  assert.ok(!fs.existsSync(output));
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
      [{ id: "13", source: "en/hello-world.md" }],
      () => ({}),
      { repository: "" },
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
  assert.match(result.stderr, /solution manifest is missing pinned source metadata/);
  assert.match(result.stderr, /<missing repository>/);
  assert.ok(!fs.existsSync(output));
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
    manifest("questions/**/template.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/template.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest(
      "en/*.md",
      [{ id: "13", source: "en/hello-world.md" }],
      () => ({}),
      {},
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
  assert.match(result.stderr, /test-case manifest has unexpected fixture metadata/);
  assert.match(result.stderr, /questions\/\*\*\/test-cases\.ts/);
  assert.ok(!fs.existsSync(output));
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest(
      "questions/**/template.ts",
      [{ id: "13", source: "questions/00013-warm-hello-world/template.ts" }],
      () => ({}),
      {},
      { generated: 2 },
    ),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [{ id: "13", source: "en/hello-world.md" }]),
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
  assert.match(result.stderr, /template manifest count metadata is inconsistent/);
  assert.match(result.stderr, /generated: 2/);
  assert.ok(!fs.existsSync(output));
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest(
      "questions/**/template.ts",
      [
        { id: "13", source: "questions/00013-warm-hello-world/template.ts" },
        { id: "189", source: ".\\questions\\00013-warm-hello-world\\template.ts" },
      ],
      ({ id }) => ({ challenge: { id, level: "easy", slug: `case-${id}` } }),
    ),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
      { id: "189", source: "questions/00189-easy-awaited/test-cases.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [
      { id: "13", source: "en/hello-world.md" },
      { id: "189", source: "en/awaited.md" },
    ]),
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
    /template manifest contains duplicate output path questions\/00013-warm-hello-world\/template\.ts/,
  );
  assert.ok(!fs.existsSync(output));
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
      () => ({ output: "" }),
    ),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [{ id: "13", source: "en/hello-world.md" }]),
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
  assert.match(result.stderr, /test-case manifest entry 1 has no output path/);
  assert.ok(!fs.existsSync(output));
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest(
      "questions/**/template.ts",
      [{ id: "13", source: "questions/00013-warm-hello-world/template.ts" }],
      () => ({ source: "" }),
    ),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [{ id: "13", source: "en/hello-world.md" }]),
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
  assert.match(result.stderr, /template manifest entry 1 has no source path/);
  assert.ok(!fs.existsSync(output));
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
      { id: "189", source: "questions/00189-easy-awaited/template.ts" },
    ]),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
      { id: "189", source: "questions/00189-easy-awaited/test-cases.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest(
      "en/*.md",
      [
        { id: "13", source: "en/hello-world.md" },
        { id: "189", source: ".\\en\\hello-world.md" },
      ],
      ({ id }) => ({ output: `solutions/${id}.ts` }),
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
    /solution manifest contains duplicate source path en\/hello-world\.md/,
  );
  assert.ok(!fs.existsSync(output));
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest(
      "questions/**/template.ts",
      [],
      () => ({}),
      {},
      { expectedGenerated: 0, generated: 0 },
    ),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      { id: "13", source: "questions/00013-warm-hello-world/test-cases.ts" },
    ]),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [{ id: "13", source: "en/hello-world.md" }]),
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
  assert.match(result.stderr, /template manifest has no entries/);
  assert.ok(!fs.existsSync(output));
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
      [{ id: "13", source: "en/hello-world.md" }],
      () => ({ declarations: [] }),
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
  assert.match(result.stderr, /solution manifest entry has no declarations/);
  assert.match(result.stderr, /en\/hello-world\.md/);
  assert.ok(!fs.existsSync(output));
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest("questions/**/template.ts", [
      {
        id: "13",
        level: "warm",
        slug: "hello-world",
        source: "questions/00013-warm-hello-world/template.ts",
      },
    ]),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      {
        id: "13",
        level: "easy",
        slug: "hello-world",
        source: "questions/00013-easy-hello-world/test-cases.ts",
      },
    ]),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [
      { id: "13", level: "warm", source: "en/hello-world.md" },
    ]),
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
  assert.match(result.stderr, /paired source metadata mismatch for challenge id 13/);
  assert.match(result.stderr, /template\/test-case level: warm vs easy/);
  assert.ok(!fs.existsSync(output));
});

withTempDir((dir) => {
  const templates = path.join(dir, "templates.json");
  const testCases = path.join(dir, "test-cases.json");
  const solutions = path.join(dir, "solutions.json");
  const output = path.join(dir, "pairing.json");

  writeJson(
    templates,
    manifest("questions/**/template.ts", [
      {
        id: "13",
        level: "warm",
        source: "questions/00013-warm-hello-world/template.ts",
      },
    ]),
  );
  writeJson(
    testCases,
    manifest("questions/**/test-cases.ts", [
      {
        id: "13",
        level: "warm",
        source: "questions/00013-warm-hello-world/test-cases.ts",
      },
    ]),
  );
  writeJson(
    solutions,
    manifest("en/*.md", [
      { id: "13", level: "warm", source: "en/hello-world.md" },
      { id: "189", level: "easy", source: "en/awaited.md" },
    ]),
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
    /solution entries are missing official assertion sources/,
  );
  assert.match(result.stderr, /solutions without templates: 1/);
  assert.match(result.stderr, /solutions without test cases: 1/);
  assert.ok(!fs.existsSync(output));
});
