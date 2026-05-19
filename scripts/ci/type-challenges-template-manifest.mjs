#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [sourceDir, compileDir, manifestPath] = process.argv.slice(2);

if (!sourceDir || !compileDir || !manifestPath) {
  console.error(
    "usage: type-challenges-template-manifest.mjs <source-dir> <compile-dir> <manifest.json>",
  );
  process.exit(2);
}

const repository = process.env.TYPE_CHALLENGES_REPO;
const ref = process.env.TYPE_CHALLENGES_REF;
const expectedGenerated = Number(process.env.TYPE_CHALLENGES_EXPECTED_GENERATED);
const CHALLENGE_LEVELS = new Set(["warm", "easy", "medium", "hard", "extreme"]);

if (
  typeof repository !== "string" ||
  repository.trim() === "" ||
  typeof ref !== "string" ||
  ref.trim() === "" ||
  !Number.isInteger(expectedGenerated)
) {
  console.error("error: missing Type Challenges repository, ref, or expected count");
  process.exit(1);
}

function walkTemplates(dir, results = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkTemplates(fullPath, results);
    } else if (entry.isFile() && entry.name === "template.ts") {
      results.push(fullPath);
    }
  }
  return results;
}

function parseChallenge(segment) {
  const match = segment.match(/^0*(\d+)-([a-z]+)-(.+)$/);
  if (!match) {
    return {
      id: null,
      level: "unknown",
      slug: segment,
    };
  }

  return {
    id: match[1],
    level: match[2],
    slug: match[3],
  };
}

function parseRequiredChallenge(segment, source) {
  const challenge = parseChallenge(segment);
  if (challenge.id == null) {
    console.error(
      `error: Type Challenges template source has an unparseable challenge directory: ${source}`,
    );
    process.exit(1);
  }
  if (!CHALLENGE_LEVELS.has(challenge.level)) {
    console.error(
      `error: Type Challenges template source has an unknown challenge level ${challenge.level}: ${source}`,
    );
    process.exit(1);
  }
  return challenge;
}

function validateUniqueChallengeIds(entries) {
  const seen = new Map();
  for (const entry of entries) {
    const previous = seen.get(entry.challenge.id);
    if (previous) {
      console.error(
        `error: duplicate Type Challenges template challenge id ${entry.challenge.id}: ${previous} and ${entry.source}`,
      );
      process.exit(1);
    }
    seen.set(entry.challenge.id, entry.source);
  }
}

function validateManifestPath(file, entries) {
  const resolvedCompileDir = path.resolve(compileDir);
  const resolvedManifestPath = path.resolve(file);
  if (
    resolvedManifestPath === resolvedCompileDir ||
    !resolvedManifestPath.startsWith(`${resolvedCompileDir}${path.sep}`)
  ) {
    console.error(`error: template manifest path must stay inside compile directory: ${file}`);
    process.exit(1);
  }

  const generatedOutputs = new Set(
    entries.map((entry) => path.resolve(resolvedCompileDir, entry.output)),
  );
  if (generatedOutputs.has(resolvedManifestPath)) {
    console.error(`error: template manifest path must not overwrite generated output: ${file}`);
    process.exit(1);
  }

  if (fs.existsSync(resolvedManifestPath) && !fs.statSync(resolvedManifestPath).isFile()) {
    console.error(`error: template manifest path is not a file: ${file}`);
    process.exit(1);
  }

  const manifestDir = path.dirname(resolvedManifestPath);
  if (!fs.existsSync(manifestDir) || !fs.statSync(manifestDir).isDirectory()) {
    console.error(`error: template manifest parent directory does not exist: ${file}`);
    process.exit(1);
  }

  return resolvedManifestPath;
}

const questionsDir = path.join(sourceDir, "questions");
if (!fs.existsSync(questionsDir)) {
  console.error(`error: Type Challenges questions directory not found: ${questionsDir}`);
  process.exit(1);
}

const entries = walkTemplates(questionsDir)
  .map((templatePath) => {
    const source = path.relative(sourceDir, templatePath).split(path.sep).join("/");
    const output = source;
    const outputPath = path.join(compileDir, output);

    if (!fs.existsSync(outputPath)) {
      console.error(`error: manifest output does not exist: ${output}`);
      process.exit(1);
    }
    if (!fs.statSync(outputPath).isFile()) {
      console.error(`error: manifest output is not a file: ${output}`);
      process.exit(1);
    }

    return {
      output,
      source,
      challenge: parseRequiredChallenge(
        path.basename(path.dirname(templatePath)),
        source,
      ),
    };
  })
  .sort((left, right) => left.source.localeCompare(right.source));

if (entries.length === 0) {
  console.error(`error: no Type Challenges template sources found under ${questionsDir}`);
  process.exit(1);
}

validateUniqueChallengeIds(entries);

if (entries.length !== expectedGenerated) {
  console.error(
    `error: manifest has ${entries.length} entries; expected ${expectedGenerated} for ${ref}`,
  );
  process.exit(1);
}

const resolvedManifestPath = validateManifestPath(manifestPath, entries);

const manifest = {
  fixture: "type-challenges-project",
  source: {
    repository,
    ref,
    path: "questions/**/template.ts",
  },
  expectedGenerated,
  generated: entries.length,
  entries,
};

fs.writeFileSync(resolvedManifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
