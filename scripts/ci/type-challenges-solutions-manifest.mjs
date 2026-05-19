#!/usr/bin/env node
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
const lines = fs.readFileSync(tsvPath, "utf8").trimEnd().split(/\r?\n/);
const header = lines.shift();

if (header !== "output\tsource\tid\tlevel\ttitle") {
  console.error(`error: unexpected manifest TSV header: ${header ?? "<empty>"}`);
  process.exit(1);
}

function readOutputMetadata(outputPath) {
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

const entries = lines
  .filter((line) => line.length > 0)
  .map((line, index) => {
    const [output, source, id, level, ...titleParts] = line.split("\t");
    const title = titleParts.join("\t");

    if (!output || !source || !id || !level || !title) {
      console.error(`error: incomplete manifest row ${index + 2}: ${line}`);
      process.exit(1);
    }

    validateManifestPath(output, "output", "solutions/");
    validateManifestPath(source, "source", "en/");
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

    const { declarations, semanticFamilies } = readOutputMetadata(outputPath);
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
      },
      declarations,
      semanticFamilies,
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
