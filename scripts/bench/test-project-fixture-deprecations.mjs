#!/usr/bin/env node
/**
 * Guards the TS 6.0 deprecation invariant for the generated project-fixture
 * tsconfigs.
 *
 * tsc 6.0.x emits TS5101 ("Option 'baseUrl' is deprecated and will stop
 * functioning in TypeScript 7.0. Specify '"ignoreDeprecations": "6.0"' to
 * silence this error.") whenever a tsconfig sets `baseUrl` without
 * `ignoreDeprecations`. The project-compatibility guard subtracts tsc's own
 * diagnostics from tsz's, so a fixture that omits `ignoreDeprecations` would
 * trip TS5101 on both sides — but only after the vendored TypeScript submodule
 * was bumped to 6.0. The io-ts row regressed exactly this way: it set
 * `baseUrl` but, unlike its sibling baseUrl-setting configs
 * (drizzle-orm/arktype/type-graphql), did not also set
 * `"ignoreDeprecations": "6.0"`.
 *
 * This test generates each baseUrl-setting fixture config and asserts the
 * structural rule: when a generated tsconfig sets `baseUrl`, it must also set
 * `"ignoreDeprecations": "6.0"`, so the corpus row stays tsc-clean (and
 * tsz-clean) on the deprecation diagnostic.
 *
 * The config writers only emit the tsconfig (plus any module stubs) into the
 * output directory; they do not read the cloned source, so the test runs
 * instantly with no network access or repo checkout.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const PROJECT_FIXTURES_SCRIPT = path.join(SCRIPT_DIR, "project-fixtures.sh");
const PROJECT_FIXTURE_STUBS = path.join(SCRIPT_DIR, "lib", "project-fixture-stubs.sh");

// Every fixture config writer that sets `baseUrl`. Keep this list in sync with
// the `"baseUrl"` lines in scripts/bench/project-fixtures.sh — a new
// baseUrl-setting config must also set `ignoreDeprecations: "6.0"`.
const BASE_URL_CONFIG_WRITERS = [
  "tsz_write_io_ts_config",
  "tsz_write_drizzle_orm_config",
  "tsz_write_arktype_config",
  "tsz_write_type_graphql_config",
];

function generateConfig(writerFn, outputPath) {
  const result = spawnSync(
    "bash",
    [
      "-c",
      'set -e; source "$STUBS"; source "$FIXTURES"; "$WRITER" "$OUTPUT"',
    ],
    {
      cwd: ROOT,
      env: {
        ...process.env,
        ROOT_DIR: ROOT,
        STUBS: PROJECT_FIXTURE_STUBS,
        FIXTURES: PROJECT_FIXTURES_SCRIPT,
        WRITER: writerFn,
        OUTPUT: outputPath,
      },
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  if (result.status !== 0) {
    throw new Error(
      `${writerFn} exited with status ${result.status}:\n${result.stderr}`,
    );
  }
  return JSON.parse(fs.readFileSync(outputPath, "utf8"));
}

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`  ✓ ${name}`);
    passed++;
  } catch (err) {
    console.error(`  ✗ ${name}`);
    console.error(`    ${err.message}`);
    failed++;
  }
}

const tmpBase = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-fixture-deprecations-"));

try {
  console.log("test-project-fixture-deprecations: baseUrl => ignoreDeprecations 6.0");

  for (const writerFn of BASE_URL_CONFIG_WRITERS) {
    test(`${writerFn} sets baseUrl with ignoreDeprecations 6.0`, () => {
      const outDir = path.join(tmpBase, writerFn);
      fs.mkdirSync(outDir, { recursive: true });
      const config = generateConfig(writerFn, path.join(outDir, "tsconfig.tsz-guard.json"));
      const options = config.compilerOptions ?? {};
      assert.ok(
        "baseUrl" in options,
        `${writerFn} should set baseUrl (this writer is listed as a baseUrl-setting config)`,
      );
      assert.equal(
        options.ignoreDeprecations,
        "6.0",
        `${writerFn} sets baseUrl, so it must also set ignoreDeprecations: "6.0" or tsc 6.0.x emits TS5101`,
      );
    });
  }
} finally {
  fs.rmSync(tmpBase, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
