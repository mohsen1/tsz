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
