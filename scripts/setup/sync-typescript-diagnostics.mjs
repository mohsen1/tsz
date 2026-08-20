#!/usr/bin/env node

import { spawnSync } from "child_process";
import { createHash } from "crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "path";
import { fileURLToPath } from "url";
import { gunzipSync } from "zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDir, "../..");
const vendorRoot = join(root, "vendor/typescript-go");
const manifestPath = join(vendorRoot, "artifacts.json");
const versionsPath = join(root, "scripts/conformance/typescript-versions.json");
const generatorPath = join(root, "scripts/gen_diagnostics.mjs");
const outputLocaleRoot = "crates/tsz-core/data/locales";

const EXPECTED_LOCALES = [
  ["cs", "cs-CZ.json.gz", "cs.json"],
  ["de", "de-DE.json.gz", "de.json"],
  ["es", "es-ES.json.gz", "es.json"],
  ["fr", "fr-FR.json.gz", "fr.json"],
  ["it", "it-IT.json.gz", "it.json"],
  ["ja", "ja-JP.json.gz", "ja.json"],
  ["ko", "ko-KR.json.gz", "ko.json"],
  ["pl", "pl-PL.json.gz", "pl.json"],
  ["pt-br", "pt-BR.json.gz", "pt-br.json"],
  ["ru", "ru-RU.json.gz", "ru.json"],
  ["tr", "tr-TR.json.gz", "tr.json"],
  ["zh-cn", "zh-CN.json.gz", "zh-cn.json"],
  ["zh-tw", "zh-TW.json.gz", "zh-tw.json"],
];

function usage(message) {
  if (message) {
    console.error(message);
  }
  console.error(
    "Usage: node scripts/setup/sync-typescript-diagnostics.mjs --check|--write"
  );
  process.exit(2);
}

function invariant(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function readJson(path, label) {
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    throw new Error("Cannot read " + label + " " + path + ": " + error.message);
  }

  try {
    return { bytes, value: JSON.parse(bytes.toString("utf8")) };
  } catch (error) {
    throw new Error("Cannot parse " + label + " " + path + ": " + error.message);
  }
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function verifyBytes(bytes, expectedSize, expectedSha256, label) {
  invariant(
    Number.isInteger(expectedSize) && expectedSize >= 0,
    label + " has an invalid expected size in " + manifestPath
  );
  invariant(
    typeof expectedSha256 === "string" && /^[0-9a-f]{64}$/.test(expectedSha256),
    label + " has an invalid expected SHA-256 in " + manifestPath
  );
  invariant(
    bytes.length === expectedSize,
    label + " size mismatch: expected " + expectedSize + ", got " + bytes.length
  );

  const actualSha256 = sha256(bytes);
  invariant(
    actualSha256 === expectedSha256,
    label + " SHA-256 mismatch: expected " + expectedSha256 + ", got " + actualSha256
  );
}

function resolveInside(base, path, label) {
  invariant(typeof path === "string" && path.length > 0, label + " path is missing");
  const absolute = resolve(base, path);
  const rel = relative(base, absolute);
  invariant(
    rel.length > 0 &&
      rel !== ".." &&
      !rel.startsWith(".." + sep) &&
      !isAbsolute(rel),
    label + " escapes its allowed root: " + path
  );
  return absolute;
}

function verifyCatalog(bytes, expectedEntries, label) {
  let catalog;
  try {
    catalog = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    throw new Error("Cannot parse " + label + ": " + error.message);
  }
  invariant(
    catalog !== null && typeof catalog === "object" && !Array.isArray(catalog),
    label + " must contain a JSON object"
  );

  const entries = Object.entries(catalog);
  invariant(
    entries.length === expectedEntries,
    label + " entry count mismatch: expected " + expectedEntries + ", got " + entries.length
  );
  return entries;
}

function verifyDiagnosticCatalog(bytes, expectedEntries, label) {
  const entries = verifyCatalog(bytes, expectedEntries, label);
  const seenCodes = new Set();
  const categories = new Set(["Error", "Warning", "Message", "Suggestion"]);
  for (const [message, info] of entries) {
    invariant(
      info !== null && typeof info === "object" && !Array.isArray(info),
      label + " entry " + JSON.stringify(message) + " must be an object"
    );
    invariant(
      Number.isInteger(info.code) && info.code > 0,
      label + " entry " + JSON.stringify(message) + " has an invalid code"
    );
    invariant(
      categories.has(info.category),
      label + " TS" + info.code + " has an unsupported category"
    );
    invariant(!seenCodes.has(info.code), label + " repeats TS" + info.code);
    seenCodes.add(info.code);
  }
}

function sameSortedValues(actual, expected) {
  return (
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  );
}

function main() {
  const args = process.argv.slice(2);
  if (args.length !== 1 || !["--check", "--write"].includes(args[0])) {
    usage();
  }
  const mode = args[0].slice(2);

  const { value: manifest } = readJson(manifestPath, "artifact manifest");
  invariant(manifest.schema === 1, "Unsupported artifact manifest schema");
  invariant(Array.isArray(manifest.locales), "Artifact manifest locales must be an array");

  const { value: versions } = readJson(versionsPath, "TypeScript version map");
  const pin = versions.mappings?.[versions.current];
  invariant(pin !== undefined, "Current TypeScript corpus pin is not mapped");
  invariant(
    versions.current === manifest.legacyDiagnosticMessages.corpusCommit,
    "Legacy diagnostic corpus commit does not match the current TypeScript pin"
  );
  invariant(
    pin.native_sha === manifest.commit,
    "Vendored typescript-go commit does not match the current native pin"
  );
  invariant(
    pin.native_tag === manifest.tag,
    "Vendored typescript-go tag does not match the current native pin"
  );
  invariant(
    pin.native_repository === manifest.repository,
    "Vendored typescript-go repository does not match the current native pin"
  );
  invariant(
    pin.npm === manifest.typescriptVersion,
    "Vendored diagnostics version does not match the current npm compiler pin"
  );

  const legacyArtifact = manifest.legacyDiagnosticMessages;
  const legacyPath = resolveInside(root, legacyArtifact.source, "Legacy diagnostics");
  const legacyBytes = readFileSync(legacyPath);
  verifyBytes(
    legacyBytes,
    legacyArtifact.size,
    legacyArtifact.sha256,
    "Legacy diagnostics"
  );
  verifyDiagnosticCatalog(
    legacyBytes,
    legacyArtifact.entries,
    "Legacy diagnostics"
  );

  const extraArtifact = manifest.extraDiagnosticMessages;
  const extraPath = resolveInside(vendorRoot, extraArtifact.source, "Native diagnostics");
  const extraBytes = readFileSync(extraPath);
  verifyBytes(
    extraBytes,
    extraArtifact.size,
    extraArtifact.sha256,
    "Native diagnostics"
  );
  verifyDiagnosticCatalog(
    extraBytes,
    extraArtifact.entries,
    "Native diagnostics"
  );

  invariant(
    manifest.locales.length === EXPECTED_LOCALES.length,
    "Artifact manifest must describe all " + EXPECTED_LOCALES.length + " locales"
  );
  const localeByName = new Map();
  for (const locale of manifest.locales) {
    invariant(
      typeof locale.locale === "string" && !localeByName.has(locale.locale),
      "Artifact manifest contains a missing or duplicate locale"
    );
    localeByName.set(locale.locale, locale);
  }

  const desiredLocales = [];
  for (const [localeName, sourceName, outputName] of EXPECTED_LOCALES) {
    const locale = localeByName.get(localeName);
    invariant(locale !== undefined, "Artifact manifest is missing locale " + localeName);
    invariant(
      locale.source === "internal/diagnostics/loc/" + sourceName,
      "Unexpected source path for locale " + localeName
    );
    invariant(
      locale.output === outputLocaleRoot + "/" + outputName,
      "Unexpected output path for locale " + localeName
    );

    const sourcePath = resolveInside(vendorRoot, locale.source, localeName + " locale source");
    const compressed = readFileSync(sourcePath);
    verifyBytes(
      compressed,
      locale.compressedSize,
      locale.compressedSha256,
      localeName + " compressed locale"
    );

    let expanded;
    try {
      expanded = gunzipSync(compressed);
    } catch (error) {
      throw new Error("Cannot expand " + localeName + " locale: " + error.message);
    }
    verifyBytes(
      expanded,
      locale.uncompressedSize,
      locale.uncompressedSha256,
      localeName + " expanded locale"
    );
    verifyCatalog(expanded, locale.entries, localeName + " expanded locale");

    desiredLocales.push({
      locale: localeName,
      outputPath: resolveInside(root, locale.output, localeName + " locale output"),
      expanded,
    });
  }

  const vendorLocaleDir = join(vendorRoot, "internal/diagnostics/loc");
  const actualVendorFiles = readdirSync(vendorLocaleDir)
    .filter((name) => name.endsWith(".json.gz"))
    .sort();
  const expectedVendorFiles = EXPECTED_LOCALES.map((entry) => entry[1]).sort();
  invariant(
    sameSortedValues(actualVendorFiles, expectedVendorFiles),
    "Vendored locale file set differs from the artifact manifest"
  );

  const outputLocaleDir = join(root, outputLocaleRoot);
  const actualOutputFiles = existsSync(outputLocaleDir)
    ? readdirSync(outputLocaleDir)
        .filter((name) => name.endsWith(".json"))
        .sort()
    : [];
  const expectedOutputFiles = EXPECTED_LOCALES.map((entry) => entry[2]).sort();
  const unexpectedOutputFiles = actualOutputFiles.filter(
    (name) => !expectedOutputFiles.includes(name)
  );
  invariant(
    unexpectedOutputFiles.length === 0,
    "Runtime locale directory contains unexpected files: " +
      unexpectedOutputFiles.join(", ")
  );

  const staleLocales = [];
  const updatedLocales = [];
  for (const desired of desiredLocales) {
    const current = existsSync(desired.outputPath)
      ? readFileSync(desired.outputPath)
      : undefined;
    if (current?.equals(desired.expanded)) {
      continue;
    }

    if (mode === "check") {
      staleLocales.push(relative(root, desired.outputPath));
    } else {
      mkdirSync(dirname(desired.outputPath), { recursive: true });
      writeFileSync(desired.outputPath, desired.expanded);
      updatedLocales.push(relative(root, desired.outputPath));
    }
  }

  if (staleLocales.length > 0) {
    console.error("Generated locales are stale:");
    for (const path of staleLocales) {
      console.error("  " + path);
    }
    console.error(
      "Run node scripts/setup/sync-typescript-diagnostics.mjs --write."
    );
  }

  const generator = spawnSync(
    process.execPath,
    [generatorPath, "--" + mode, legacyPath, extraPath],
    { cwd: root, stdio: "inherit" }
  );
  if (generator.error) {
    throw new Error("Cannot run diagnostic generator: " + generator.error.message);
  }
  const generatorFailed = generator.status !== 0;
  if (mode === "write" && generatorFailed) {
    throw new Error(
      "Diagnostic generator failed with status " +
        (generator.status ?? "unknown") +
        (generator.signal ? " (" + generator.signal + ")" : "")
    );
  }

  if (staleLocales.length > 0 || generatorFailed) {
    process.exitCode = 1;
    return;
  }

  const action = mode === "check" ? "Checked" : "Synchronized";
  const detail =
    mode === "write" ? " (" + updatedLocales.length + " locales updated)" : "";
  console.log(
    action +
      " TypeScript " +
      manifest.typescriptVersion +
      " diagnostics and " +
      desiredLocales.length +
      " locales" +
      detail
  );
}

try {
  main();
} catch (error) {
  console.error("sync-typescript-diagnostics: " + error.message);
  process.exit(1);
}
