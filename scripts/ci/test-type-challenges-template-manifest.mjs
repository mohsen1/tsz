#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const MANIFEST_SCRIPT = path.join(ROOT, "scripts", "ci", "type-challenges-template-manifest.mjs");

function withTempDir(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-type-challenges-template-"));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeTemplate(root, rel) {
  const source = path.join(root, "source", rel);
  const output = path.join(root, "compile", rel);
  fs.mkdirSync(path.dirname(source), { recursive: true });
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(source, "type Solution = unknown;\n", "utf8");
  fs.writeFileSync(output, "type Solution = unknown;\nexport {};\n", "utf8");
}

withTempDir((dir) => {
  writeTemplate(dir, "questions/00013-warm-hello-world/template.ts");
  writeTemplate(dir, "questions/00189-easy-awaited/template.ts");

  const manifestPath = path.join(dir, "compile", "type-challenges-template-manifest.json");
  const result = spawnSync(
    process.execPath,
    [
      MANIFEST_SCRIPT,
      path.join(dir, "source"),
      path.join(dir, "compile"),
      manifestPath,
    ],
    {
      cwd: ROOT,
      encoding: "utf8",
      env: {
        ...process.env,
        TYPE_CHALLENGES_REPO: "https://example.invalid/type-challenges.git",
        TYPE_CHALLENGES_REF: "fixture-ref",
        TYPE_CHALLENGES_EXPECTED_GENERATED: "2",
      },
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  assert.equal(manifest.fixture, "type-challenges-project");
  assert.equal(manifest.source.repository, "https://example.invalid/type-challenges.git");
  assert.equal(manifest.source.ref, "fixture-ref");
  assert.equal(manifest.expectedGenerated, 2);
  assert.equal(manifest.generated, 2);
  assert.deepEqual(
    manifest.entries.map((entry) => [
      entry.source,
      entry.challenge.id,
      entry.challenge.level,
      entry.challenge.slug,
    ]),
    [
      ["questions/00013-warm-hello-world/template.ts", "13", "warm", "hello-world"],
      ["questions/00189-easy-awaited/template.ts", "189", "easy", "awaited"],
    ],
  );
});

withTempDir((dir) => {
  writeTemplate(dir, "questions/custom-shape/template.ts");

  const result = spawnSync(
    process.execPath,
    [
      MANIFEST_SCRIPT,
      path.join(dir, "source"),
      path.join(dir, "compile"),
      path.join(dir, "compile", "type-challenges-template-manifest.json"),
    ],
    {
      cwd: ROOT,
      encoding: "utf8",
      env: {
        ...process.env,
        TYPE_CHALLENGES_REPO: "https://example.invalid/type-challenges.git",
        TYPE_CHALLENGES_REF: "fixture-ref",
        TYPE_CHALLENGES_EXPECTED_GENERATED: "1",
      },
    },
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unparseable challenge directory/);
});

withTempDir((dir) => {
  writeTemplate(dir, "questions/00013-warm-hello-world/template.ts");
  writeTemplate(dir, "questions/013-easy-duplicate-hello/template.ts");

  const result = spawnSync(
    process.execPath,
    [
      MANIFEST_SCRIPT,
      path.join(dir, "source"),
      path.join(dir, "compile"),
      path.join(dir, "compile", "type-challenges-template-manifest.json"),
    ],
    {
      cwd: ROOT,
      encoding: "utf8",
      env: {
        ...process.env,
        TYPE_CHALLENGES_REPO: "https://example.invalid/type-challenges.git",
        TYPE_CHALLENGES_REF: "fixture-ref",
        TYPE_CHALLENGES_EXPECTED_GENERATED: "2",
      },
    },
  );
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /duplicate Type Challenges template challenge id 13/);
});
