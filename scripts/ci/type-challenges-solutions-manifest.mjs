#!/usr/bin/env node
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { semanticFamiliesForText } from "./type-challenges-semantic-families.mjs";

const [tsvPath, manifestPath] = process.argv.slice(2);

if (!tsvPath || !manifestPath) {
  console.error(
    "usage: type-challenges-solutions-manifest.mjs <entries.tsv> <manifest.json>",
  );
  process.exit(2);
}

/**
 * Load content-addressed cache from an existing manifest file.
 *
 * When the same source content (identified by `sourceSha256`) was processed in
 * a prior run against the same repository ref, the derived output values
 * (`outputSha256`, `declarations`, `semanticFamilies`) are deterministic and do
 * not need to be recomputed.  This avoids re-reading and re-hashing every
 * generated `.ts` file when most sources are unchanged between runs.
 *
 * The cache is keyed by `sourceSha256` alone (not by file path), because the
 * content hash fully identifies the source content.  Two different source paths
 * with the same content would share a cache entry, but that is harmless — they
 * would produce identical output anyway.
 *
 * Cache validity conditions (both must hold):
 *   1. The existing manifest's `source.ref` equals the current ref env var.
 *   2. The entry's `challenge.sourceSha256` matches the TSV row's value.
 *
 * @param {string} existingManifestPath - Path that will be overwritten; read
 *   before processing starts so the old content is still available.
 * @param {string} currentRef - Current `TYPE_CHALLENGES_SOLUTIONS_REF` value.
 * @returns {Map<string, {outputSha256: string, declarations: string[], semanticFamilies: string[]}>}
 */
function loadManifestCache(existingManifestPath, currentRef) {
  let oldManifest;
  try {
    oldManifest = JSON.parse(fs.readFileSync(existingManifestPath, "utf8"));
  } catch {
    return new Map();
  }
  if (oldManifest.source?.ref !== currentRef) return new Map();
  if (!Array.isArray(oldManifest.entries)) return new Map();

  const cache = new Map();
  for (const entry of oldManifest.entries) {
    const sourceSha256 = entry.challenge?.sourceSha256;
    if (
      !/^[0-9a-f]{64}$/.test(sourceSha256) ||
      cache.has(sourceSha256)
    ) {
      continue;
    }
    if (
      typeof entry.outputSha256 !== "string" ||
      !Array.isArray(entry.declarations) ||
      !Array.isArray(entry.semanticFamilies)
    ) {
      continue;
    }
    cache.set(sourceSha256, {
      outputSha256: entry.outputSha256,
      declarations: entry.declarations,
      semanticFamilies: entry.semanticFamilies,
    });
  }
  return cache;
}

const repository = process.env.TYPE_CHALLENGES_SOLUTIONS_REPO;
const ref = process.env.TYPE_CHALLENGES_SOLUTIONS_REF;
const expectedGenerated = Number(
  process.env.TYPE_CHALLENGES_SOLUTIONS_EXPECTED_GENERATED,
);
const CHALLENGE_LEVELS = new Set(["warm", "easy", "medium", "hard", "extreme"]);

if (
  typeof repository !== "string" ||
  repository.trim() === "" ||
  typeof ref !== "string" ||
  ref.trim() === "" ||
  !Number.isInteger(expectedGenerated)
) {
  console.error(
    "error: missing Type Challenges solutions repository, ref, or expected count",
  );
  process.exit(1);
}

function isInsideOrSame(root, candidate) {
  return candidate === root || candidate.startsWith(`${root}${path.sep}`);
}

function validateManifestOutputPath(tsvPath, manifestPath) {
  const manifestRoot = path.dirname(path.resolve(tsvPath));
  const resolvedTsvPath = path.resolve(tsvPath);
  const resolvedManifestPath = path.resolve(manifestPath);

  if (
    resolvedManifestPath === manifestRoot ||
    !isInsideOrSame(manifestRoot, resolvedManifestPath)
  ) {
    console.error(
      `error: Type Challenges solution manifest path must stay inside the compile directory: ${manifestPath}`,
    );
    process.exit(1);
  }

  if (resolvedManifestPath === resolvedTsvPath) {
    console.error(
      `error: Type Challenges solution manifest path must not overwrite the TSV input: ${manifestPath}`,
    );
    process.exit(1);
  }

  if (isInsideOrSame(path.join(manifestRoot, "solutions"), resolvedManifestPath)) {
    console.error(
      `error: Type Challenges solution manifest path must not clobber generated solution outputs: ${manifestPath}`,
    );
    process.exit(1);
  }

  const parent = path.dirname(resolvedManifestPath);
  if (!fs.existsSync(parent) || !fs.statSync(parent).isDirectory()) {
    console.error(
      `error: Type Challenges solution manifest parent directory does not exist: ${parent}`,
    );
    process.exit(1);
  }

  if (
    fs.existsSync(resolvedManifestPath) &&
    !fs.statSync(resolvedManifestPath).isFile()
  ) {
    console.error(
      `error: Type Challenges solution manifest path is not a file: ${manifestPath}`,
    );
    process.exit(1);
  }

  return {
    manifestRoot,
    resolvedManifestPath,
  };
}

const {
  manifestRoot,
  resolvedManifestPath,
} = validateManifestOutputPath(tsvPath, manifestPath);

// Load the old manifest before we start writing so we can reuse cached
// outputSha256/declarations/semanticFamilies for unchanged source entries.
const manifestCache = loadManifestCache(resolvedManifestPath, ref);

const lines = fs.readFileSync(tsvPath, "utf8").trimEnd().split(/\r?\n/);
const header = lines.shift();

if (header !== "output\tsource\tsourceSha256\tid\tlevel\ttitle") {
  console.error(`error: unexpected manifest TSV header: ${header ?? "<empty>"}`);
  process.exit(1);
}

function readOutputMetadata(outputPath, sourceSha256) {
  // Reuse cached metadata when the source content is unchanged.  The output
  // `.ts` content is deterministically derived from the source markdown, so
  // unchanged source => unchanged output => all derived values are stable.
  const cached = manifestCache.get(sourceSha256);
  if (cached !== undefined) {
    return cached;
  }

  const text = fs.readFileSync(outputPath, "utf8");
  const names = [];
  const seen = new Set();
  const declarationPattern =
    /^\s*(?:export\s+)?(?:declare\s+)?(?:type|interface|namespace|class|enum)\s+([A-Za-z_$][A-Za-z0-9_$]*)|^\s*(?:export\s+)?declare\s+(?:function|const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)/gm;

  for (const match of text.matchAll(declarationPattern)) {
    const name = match[1] ?? match[2];
    if (!seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  }

  return {
    declarations: names,
    semanticFamilies: semanticFamiliesForText(text),
    outputSha256: crypto.createHash("sha256").update(text).digest("hex"),
  };
}

function parseRequiredChallengeId(id, source) {
  if (!/^\d+$/.test(id)) {
    console.error(
      `error: Type Challenges solution source has an unparseable challenge id: ${source}`,
    );
    process.exit(1);
  }
  return id.replace(/^0+/, "") || "0";
}

function validateUniqueChallengeIds(entries) {
  const seen = new Map();
  for (const entry of entries) {
    const previous = seen.get(entry.challenge.id);
    if (previous) {
      console.error(
        `error: duplicate Type Challenges solution challenge id ${entry.challenge.id}: ${previous} and ${entry.source}`,
      );
      process.exit(1);
    }
    seen.set(entry.challenge.id, entry.source);
  }
}

function validateUniqueEntryField(entries, field, label) {
  const seen = new Map();
  for (const entry of entries) {
    const value = entry[field];
    const previous = seen.get(value);
    if (previous) {
      console.error(
        `error: duplicate Type Challenges solution ${label} ${value}: ${previous.source} and ${entry.source}`,
      );
      process.exit(1);
    }
    seen.set(value, entry);
  }
}

function validateChallengeLevel(level, source) {
  if (!CHALLENGE_LEVELS.has(level)) {
    console.error(
      `error: Type Challenges solution source has an unknown challenge level ${level}: ${source}`,
    );
    process.exit(1);
  }
}

function validateSha256Hex(value, label, source) {
  if (!/^[0-9a-f]{64}$/.test(value)) {
    console.error(
      `error: Type Challenges solution ${label} must be a lowercase sha256 hex digest: ${source}`,
    );
    process.exit(1);
  }
}

function validateManifestPath(value, label, requiredPrefix) {
  if (
    path.isAbsolute(value) ||
    value.includes("\\") ||
    !value.startsWith(requiredPrefix) ||
    value
      .split("/")
      .some((segment) => segment.length === 0 || segment === "." || segment === "..")
  ) {
    console.error(`error: unsafe manifest ${label} path: ${value}`);
    process.exit(1);
  }
}

function parseSourceStem(source) {
  if (!source.endsWith(".md")) {
    console.error(
      `error: Type Challenges solution source must be a Markdown file: ${source}`,
    );
    process.exit(1);
  }

  return path.posix.basename(source, ".md");
}

const entries = lines
  .filter((line) => line.length > 0)
  .map((line, index) => {
    const [output, source, sourceSha256, id, level, ...titleParts] = line.split("\t");
    const title = titleParts.join("\t");

    if (!output || !source || !sourceSha256 || !id || !level || !title) {
      console.error(`error: incomplete manifest row ${index + 2}: ${line}`);
      process.exit(1);
    }

    validateManifestPath(output, "output", "solutions/");
    validateManifestPath(source, "source", "en/");
    validateSha256Hex(sourceSha256, "sourceSha256", source);
    validateChallengeLevel(level, source);

    const outputPath = path.join(manifestRoot, output);
    if (!fs.existsSync(outputPath)) {
      console.error(`error: manifest output does not exist: ${output}`);
      process.exit(1);
    }
    if (!fs.statSync(outputPath).isFile()) {
      console.error(`error: manifest output is not a file: ${output}`);
      process.exit(1);
    }

    const {
      declarations,
      semanticFamilies,
      outputSha256,
    } = readOutputMetadata(outputPath, sourceSha256);
    if (declarations.length === 0) {
      console.error(`error: manifest output has no declarations: ${output}`);
      process.exit(1);
    }

    return {
      output,
      source,
      challenge: {
        id: parseRequiredChallengeId(id, source),
        level,
        title,
        sourceStem: parseSourceStem(source),
        sourceSha256,
      },
      declarations,
      semanticFamilies,
      outputSha256,
    };
  });

validateUniqueChallengeIds(entries);
validateUniqueEntryField(entries, "output", "output");
validateUniqueEntryField(entries, "source", "source");

if (entries.length !== expectedGenerated) {
  console.error(
    `error: manifest has ${entries.length} entries; expected ${expectedGenerated} for ${ref}`,
  );
  process.exit(1);
}

const manifest = {
  fixture: "type-challenges-solutions-project",
  source: {
    repository,
    ref,
    path: "en/*.md",
  },
  expectedGenerated,
  generated: entries.length,
  entries,
};

fs.writeFileSync(resolvedManifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
