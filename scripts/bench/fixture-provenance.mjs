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

const PROVENANCE_FILENAME = ".tsz-fixture-provenance.json";

/** Repo root relative to this file (scripts/bench/fixture-provenance.mjs). */
const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/**
 * Return the SHA-256 hex digest of `filePath`, or null when the file does not exist.
 * @param {string} filePath Absolute path to the file.
 * @returns {string | null}
 */
function sha256File(filePath) {
  try {
    const bytes = fs.readFileSync(filePath);
    return crypto.createHash("sha256").update(bytes).digest("hex");
  } catch {
    return null;
  }
}

/**
 * Return the installed npm version, or null if the query fails.
 * @returns {string | null}
 */
function npmVersion() {
  const result = spawnSync("npm", ["--version"], { encoding: "utf8" });
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
 * @returns {{ provenancePath: string, provenance: object }} The written provenance record.
 */
export function writeFixtureProvenance({ outputDir, generatorScript, templateName, dryRun, extraFiles = [] }) {
  const generatorScriptRelative = path.relative(REPO_ROOT, generatorScript).replace(/\\/g, "/");

  const standardFiles = ["package.json", "tsconfig.json", "package-lock.json"];
  const filesToHash = [...new Set([...standardFiles, ...extraFiles.map((f) => path.relative(outputDir, f).replace(/\\/g, "/"))])];

  const fileHashes = {};
  for (const relFile of filesToHash) {
    const absPath = path.join(outputDir, relFile);
    fileHashes[relFile] = sha256File(absPath);
  }

  const provenance = {
    generator_script: generatorScriptRelative,
    template_name: templateName,
    node_version: process.version,
    npm_version: dryRun ? null : npmVersion(),
    dry_run: dryRun,
    generated_at: new Date().toISOString(),
    file_hashes: fileHashes,
    reproduce: `node scripts/bench/${path.basename(generatorScript)} <output-dir>`,
  };

  const provenancePath = path.join(outputDir, PROVENANCE_FILENAME);
  fs.writeFileSync(provenancePath, `${JSON.stringify(provenance, null, 2)}\n`, "utf8");
  return { provenancePath, provenance };
}

export { PROVENANCE_FILENAME };
