#!/usr/bin/env node
/**
 * Shared helper for writing `.tsz-fixture-provenance.json` alongside generated
 * benchmark fixtures.  Called by both `generate-vite-app-fixture.mjs` and
 * `generate-next-app-fixture.mjs` after all fixture files have been written.
 *
 * The provenance file captures everything needed to reproduce the fixture
 * locally without checking in generated dependency trees:
 *   - Which generator script produced the fixture (repo-root-relative path)
 *   - Template name (e.g. "vite-vanilla-ts", "next-app-router")
 *   - Node.js version used
 *   - npm version used (when --dry-run is not active)
 *   - SHA-256 hashes of the key generated files
 *
 * In --dry-run mode the package-lock.json hash is recorded as null because
 * `npm install` is skipped.  All other fields are always present.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

export const PROVENANCE_FILENAME = ".tsz-fixture-provenance.json";

/** Repo-root-relative directory prefix for all generator scripts. */
export const GENERATOR_SCRIPTS_PREFIX = "scripts/bench/";

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/**
 * Parse the standard `[--dry-run] <output-dir>` CLI arguments used by fixture generators.
 * @returns {{ dryRun: boolean, outputDir: string }}
 */
export function parseGeneratorArgs() {
  const dryRun = process.argv.includes("--dry-run");
  const positional = process.argv.slice(2).filter((a) => a !== "--dry-run");
  return { dryRun, outputDir: positional[0] };
}

function sha256File(filePath) {
  try {
    const bytes = fs.readFileSync(filePath);
    return crypto.createHash("sha256").update(bytes).digest("hex");
  } catch {
    return null;
  }
}

function npmVersion(npmCommand) {
  const result = spawnSync(npmCommand, ["--version"], { encoding: "utf8" });
  if (result.status !== 0 || !result.stdout) {
    return null;
  }
  return result.stdout.trim();
}

/**
 * Write `.tsz-fixture-provenance.json` to `outputDir`.
 *
 * @param {object} options
 * @param {string}   options.outputDir       Absolute path to the generated fixture directory.
 * @param {string}   options.generatorScript Absolute path to the generator script.
 * @param {string}   options.templateName    Short identifier for the template, e.g. "vite-vanilla-ts".
 * @param {boolean}  options.dryRun          Whether `npm install` was skipped.
 * @param {string[]} [options.extraFiles]    Additional file paths (absolute) to hash, beyond the
 *                                           standard set (package.json, tsconfig.json, package-lock.json).
 * @param {string}   [options.npmCommand]    npm-compatible command used for version capture.
 * @returns {{ provenancePath: string, provenance: object }} The written provenance record.
 */
export function writeFixtureProvenance({ outputDir, generatorScript, templateName, dryRun, extraFiles = [], npmCommand = "npm" }) {
  const generatorScriptRelative = path.relative(REPO_ROOT, generatorScript).replace(/\\/g, "/");

  const standardFiles = ["package.json", "tsconfig.json", "package-lock.json"];
  const filesToHash = [...new Set([...standardFiles, ...extraFiles.map((f) => path.relative(outputDir, f).replace(/\\/g, "/"))])];

  const fileHashes = {};
  for (const relFile of filesToHash) {
    fileHashes[relFile] = sha256File(path.join(outputDir, relFile));
  }

  const provenance = {
    generator_script: generatorScriptRelative,
    template_name: templateName,
    node_version: process.version,
    npm_version: dryRun ? null : npmVersion(npmCommand),
    dry_run: dryRun,
    generated_at: new Date().toISOString(),
    file_hashes: fileHashes,
    reproduce: `node ${GENERATOR_SCRIPTS_PREFIX}${path.basename(generatorScript)} <output-dir>`,
  };

  const provenancePath = path.join(outputDir, PROVENANCE_FILENAME);
  fs.writeFileSync(provenancePath, `${JSON.stringify(provenance, null, 2)}\n`, "utf8");
  return { provenancePath, provenance };
}
