#!/usr/bin/env node
/**
 * Guards the TypeScript 7 option invariant for the generated project-fixture
 * tsconfigs.
 *
 * TS7 removes `baseUrl` and the legacy `node`/`node10` module-resolution mode.
 * `paths` entries now resolve relative to the config directory directly, so
 * fixture writers must retain their structural mappings without carrying the
 * old `baseUrl` + `ignoreDeprecations: "6.0"` workaround.
 *
 * This test generates every fixture writer that previously used `baseUrl` and
 * verifies that its mappings survive while the TS6-only options do not. A
 * source-level audit also covers generated configs that are impractical to run
 * here (notably the Type Challenges corpus generator).
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

// Every fixture config writer whose `paths` mapping previously relied on
// `baseUrl`. TS7 resolves these mappings relative to the generated config.
const PATHS_CONFIG_WRITERS = [
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

const tmpBase = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-fixture-ts7-options-"));

try {
  console.log("test-project-fixture-deprecations: TypeScript 7 option audit");

  for (const writerFn of PATHS_CONFIG_WRITERS) {
    test(`${writerFn} keeps paths without TS6 baseUrl workarounds`, () => {
      const outDir = path.join(tmpBase, writerFn);
      fs.mkdirSync(outDir, { recursive: true });
      const config = generateConfig(writerFn, path.join(outDir, "tsconfig.tsz-guard.json"));
      const options = config.compilerOptions ?? {};
      assert.ok(
        options.paths && Object.keys(options.paths).length > 0,
        `${writerFn} must retain its structural paths mapping`,
      );
      assert.equal(
        options.baseUrl,
        undefined,
        `${writerFn} must not emit TS7's removed baseUrl option`,
      );
      assert.equal(
        options.ignoreDeprecations,
        undefined,
        `${writerFn} must not emit the obsolete TS6 deprecation workaround`,
      );
    });
  }

  test("project fixture source contains no TS7-removed option values", () => {
    const source = fs.readFileSync(PROJECT_FIXTURES_SCRIPT, "utf8");
    assert.doesNotMatch(source, /"baseUrl"\s*:/, "baseUrl has been removed in TS7");
    assert.doesNotMatch(
      source,
      /"moduleResolution"\s*:\s*"(?:node|node10|classic)"/i,
      "legacy node/node10/classic module resolution has been removed in TS7",
    );
    assert.doesNotMatch(
      source,
      /"ignoreDeprecations"\s*:\s*"6\.0"/,
      "fixture writers must not retain TS6-only option workarounds",
    );
  });
} finally {
  fs.rmSync(tmpBase, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
