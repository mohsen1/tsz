#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

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

if (!repository || !ref || !Number.isInteger(expectedGenerated)) {
  console.error(
    "error: missing Type Challenges solutions repository, ref, or expected count",
  );
  process.exit(1);
}

const manifestDir = path.dirname(manifestPath);
const lines = fs.readFileSync(tsvPath, "utf8").trimEnd().split(/\r?\n/);
const header = lines.shift();

if (header !== "output\tsource\tid\tlevel\ttitle") {
  console.error(`error: unexpected manifest TSV header: ${header ?? "<empty>"}`);
  process.exit(1);
}

function readDeclarationNames(outputPath) {
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

  return names;
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

    const outputPath = path.join(manifestDir, output);
    if (!fs.existsSync(outputPath)) {
      console.error(`error: manifest output does not exist: ${output}`);
      process.exit(1);
    }

    const declarations = readDeclarationNames(outputPath);
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

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
