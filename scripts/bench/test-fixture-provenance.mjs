#!/usr/bin/env node
/**
 * Tests that the generated fixture scripts write a well-formed
 * `.tsz-fixture-provenance.json` file when run with `--dry-run`.
 *
 * Dry-run mode skips `npm install`, so these tests run instantly and
 * require no network access.  They prove the provenance fields are
 * present and correctly typed independently of package installation.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { PROVENANCE_FILENAME } from "./fixture-provenance.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const VITE_GENERATOR = path.join(SCRIPT_DIR, "generate-vite-app-fixture.mjs");
const NEXT_GENERATOR = path.join(SCRIPT_DIR, "generate-next-app-fixture.mjs");
const PROJECT_FIXTURES_SCRIPT = path.join(SCRIPT_DIR, "project-fixtures.sh");

function runGenerator(generatorPath, outputDir) {
  const result = spawnSync(process.execPath, [generatorPath, "--dry-run", outputDir], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(
      `Generator ${path.basename(generatorPath)} exited with status ${result.status}:\n${result.stderr}`,
    );
  }
  return result;
}

function runGeneratorWithFakeNpm(generatorPath, outputDir) {
  const fakeNpmPath = path.join(tmpBase, "fake-npm.mjs");
  fs.writeFileSync(fakeNpmPath, `#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

if (process.argv.includes("--version")) {
  console.log("10.0.0-test");
  process.exit(0);
}

fs.writeFileSync(path.join(process.cwd(), "package-lock.json"), JSON.stringify({
  lockfileVersion: 3,
  packages: {},
}, null, 2) + "\\n", "utf8");
fs.mkdirSync(path.join(process.cwd(), "node_modules"), { recursive: true });
`, "utf8");
  fs.chmodSync(fakeNpmPath, 0o755);

  const result = spawnSync(process.execPath, [generatorPath, outputDir], {
    encoding: "utf8",
    env: {
      ...process.env,
      TSZ_FIXTURE_GENERATOR_NPM_BIN: fakeNpmPath,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(
      `Generator ${path.basename(generatorPath)} exited with status ${result.status}:\n${result.stderr}`,
    );
  }
  return result;
}

function projectFixtureSources(rowName, env = {}) {
  const result = spawnSync("bash", ["-lc", 'source "$PROJECT_FIXTURES_SCRIPT"; tsz_project_fixture_sources "$ROW_NAME"'], {
    cwd: ROOT,
    env: {
      ...process.env,
      PROJECT_FIXTURES_SCRIPT,
      ROW_NAME: rowName,
      ...env,
    },
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(
      `tsz_project_fixture_sources ${rowName} exited with status ${result.status}:\n${result.stderr}`,
    );
  }
  return result.stdout
    .trim()
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => {
      const [name, repository, ref] = line.split("|");
      return { name, repository, ref };
    });
}

function assertProvenanceShape(provenancePath, expectedTemplateName, generatorBasename) {
  assert.ok(fs.existsSync(provenancePath), `provenance file should exist at ${provenancePath}`);

  const raw = fs.readFileSync(provenancePath, "utf8");
  let provenance;
  try {
    provenance = JSON.parse(raw);
  } catch {
    assert.fail(`provenance file is not valid JSON: ${raw}`);
  }

  assert.equal(typeof provenance.generator_script, "string", "generator_script should be a string");
  assert.ok(
    provenance.generator_script.endsWith(generatorBasename),
    `generator_script should end with ${generatorBasename}, got: ${provenance.generator_script}`,
  );
  assert.ok(
    provenance.generator_script.startsWith("scripts/bench/"),
    `generator_script should be relative to repo root (scripts/bench/...), got: ${provenance.generator_script}`,
  );

  assert.equal(provenance.template_name, expectedTemplateName, "template_name mismatch");
  assert.equal(typeof provenance.node_version, "string", "node_version should be a string");
  assert.ok(provenance.node_version.startsWith("v"), `node_version should start with 'v', got: ${provenance.node_version}`);

  assert.equal(provenance.dry_run, true, "dry_run should be true in --dry-run mode");
  assert.equal(provenance.npm_version, null, "npm_version should be null in --dry-run mode");

  assert.equal(typeof provenance.generated_at, "string", "generated_at should be a string");
  assert.ok(!Number.isNaN(Date.parse(provenance.generated_at)), "generated_at should be a valid ISO 8601 timestamp");

  assert.equal(typeof provenance.file_hashes, "object", "file_hashes should be an object");
  assert.ok("package.json" in provenance.file_hashes, "file_hashes should include package.json");
  assert.ok("tsconfig.json" in provenance.file_hashes, "file_hashes should include tsconfig.json");
  assert.ok("package-lock.json" in provenance.file_hashes, "file_hashes should include package-lock.json (null in dry-run)");
  assert.equal(
    provenance.file_hashes["package-lock.json"],
    null,
    "package-lock.json hash should be null in --dry-run (not installed yet)",
  );
  assert.equal(
    typeof provenance.file_hashes["package.json"],
    "string",
    "package.json hash should be a hex string (file is always written)",
  );
  assert.match(
    provenance.file_hashes["package.json"] ?? "",
    /^[0-9a-f]{64}$/,
    "package.json hash should be a 64-char SHA-256 hex string",
  );

  assert.equal(typeof provenance.reproduce, "string", "reproduce should be a string");
  assert.ok(
    provenance.reproduce.includes(generatorBasename),
    `reproduce should reference ${generatorBasename}`,
  );
  return provenance;
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

const tmpBase = fs.mkdtempSync(path.join(os.tmpdir(), "tsz-provenance-test-"));

try {
  console.log("test-fixture-provenance: Vite generator");
  const viteDir = path.join(tmpBase, "vite");
  let viteProvenance;

  test("Vite generator runs successfully with --dry-run", () => {
    runGenerator(VITE_GENERATOR, viteDir);
  });

  test("Vite provenance file is written", () => {
    viteProvenance = assertProvenanceShape(
      path.join(viteDir, PROVENANCE_FILENAME),
      "vite-vanilla-ts",
      "generate-vite-app-fixture.mjs",
    );
  });

  test("Vite package.json is written (dry-run still writes source files)", () => {
    assert.ok(
      fs.existsSync(path.join(viteDir, "package.json")),
      "package.json should exist in dry-run output",
    );
  });

  test("Vite tsconfig.json is written in dry-run", () => {
    assert.ok(
      fs.existsSync(path.join(viteDir, "tsconfig.json")),
      "tsconfig.json should exist in dry-run output",
    );
  });

  test("Vite node_modules absent in dry-run", () => {
    assert.ok(
      !fs.existsSync(path.join(viteDir, "node_modules")),
      "node_modules should NOT exist in dry-run output",
    );
  });

  test("Vite live-run provenance hashes package-lock.json after install", () => {
    const liveDir = path.join(tmpBase, "vite-live");
    runGeneratorWithFakeNpm(VITE_GENERATOR, liveDir);
    const provenance = JSON.parse(
      fs.readFileSync(path.join(liveDir, PROVENANCE_FILENAME), "utf8"),
    );
    assert.equal(provenance.dry_run, false, "dry_run should be false in live mode");
    assert.equal(provenance.npm_version, "10.0.0-test", "npm_version should come from the install command");
    assert.match(
      provenance.file_hashes["package-lock.json"] ?? "",
      /^[0-9a-f]{64}$/,
      "package-lock.json hash should be captured after live install writes it",
    );
  });

  test("Vite generated fixture source uses package-lock provenance", () => {
    const liveDir = path.join(tmpBase, "vite-live-source");
    runGeneratorWithFakeNpm(VITE_GENERATOR, liveDir);
    const sources = projectFixtureSources("vite-vanilla-ts-app", {
      VITE_APP_BENCH_DIR: liveDir,
    });
    assert.deepEqual(sources, [
      {
        name: "vite-vanilla-ts",
        repository: "generated:scripts/bench/generate-vite-app-fixture.mjs",
        ref: sources[0]?.ref,
      },
    ]);
    assert.match(
      sources[0]?.ref ?? "",
      /^package-lock:[0-9a-f]{64}$/,
      "generated Vite fixture source should use the package-lock hash",
    );
  });

  console.log("\ntest-fixture-provenance: Next.js generator");
  const nextDir = path.join(tmpBase, "next");
  let nextProvenance;

  test("Next.js generator runs successfully with --dry-run", () => {
    runGenerator(NEXT_GENERATOR, nextDir);
  });

  test("Next.js provenance file is written", () => {
    nextProvenance = assertProvenanceShape(
      path.join(nextDir, PROVENANCE_FILENAME),
      "next-app-router",
      "generate-next-app-fixture.mjs",
    );
  });

  test("Next.js package.json is written (dry-run still writes source files)", () => {
    assert.ok(
      fs.existsSync(path.join(nextDir, "package.json")),
      "package.json should exist in dry-run output",
    );
  });

  test("Next.js tsconfig.json is written in dry-run", () => {
    assert.ok(
      fs.existsSync(path.join(nextDir, "tsconfig.json")),
      "tsconfig.json should exist in dry-run output",
    );
  });

  test("Next.js node_modules absent in dry-run", () => {
    assert.ok(
      !fs.existsSync(path.join(nextDir, "node_modules")),
      "node_modules should NOT exist in dry-run output",
    );
  });

  test("Next.js generated fixture source falls back to package.json provenance before install", () => {
    const sources = projectFixtureSources("nextjs-fresh-app", {
      NEXT_APP_BENCH_DIR: nextDir,
    });
    assert.deepEqual(sources, [
      {
        name: "next-app-router",
        repository: "generated:scripts/bench/generate-next-app-fixture.mjs",
        ref: sources[0]?.ref,
      },
    ]);
    assert.match(
      sources[0]?.ref ?? "",
      /^package-json:[0-9a-f]{64}$/,
      "dry-run generated Next fixture source should use the package.json hash",
    );
  });

  console.log("\ntest-fixture-provenance: field consistency across generators");

  test("Both generators produce the same provenance schema keys", () => {
    const viteKeys = Object.keys(viteProvenance).sort().join(",");
    const nextKeys = Object.keys(nextProvenance).sort().join(",");
    assert.equal(viteKeys, nextKeys, "Provenance schema keys should be identical across generators");
  });

  test("Generators produce different template_name values", () => {
    assert.notEqual(
      viteProvenance.template_name,
      nextProvenance.template_name,
      "Each generator should declare a distinct template_name",
    );
  });
} finally {
  fs.rmSync(tmpBase, { recursive: true, force: true });
}

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
