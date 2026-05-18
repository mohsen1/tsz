#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [sourceDir, compileDir, manifestPath] = process.argv.slice(2);

if (!sourceDir || !compileDir || !manifestPath) {
  console.error(
    "usage: type-challenges-test-cases-manifest.mjs <source-dir> <compile-dir> <manifest.json>",
  );
  process.exit(2);
}

const repository = process.env.TYPE_CHALLENGES_REPO;
const ref = process.env.TYPE_CHALLENGES_REF;
const expectedGenerated = Number(process.env.TYPE_CHALLENGES_EXPECTED_TEST_CASES);
const CHALLENGE_LEVELS = new Set(["warm", "easy", "medium", "hard", "extreme"]);

if (!repository || !ref || !Number.isInteger(expectedGenerated)) {
  console.error(
    "error: missing Type Challenges repository, ref, or expected test-case count",
  );
  process.exit(1);
}

function walkTestCases(dir, results = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkTestCases(fullPath, results);
    } else if (entry.isFile() && entry.name === "test-cases.ts") {
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
      `error: Type Challenges test-case source has an unparseable challenge directory: ${source}`,
    );
    process.exit(1);
  }
  if (!CHALLENGE_LEVELS.has(challenge.level)) {
    console.error(
      `error: Type Challenges test-case source has an unknown challenge level ${challenge.level}: ${source}`,
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
        `error: duplicate Type Challenges test-case challenge id ${entry.challenge.id}: ${previous} and ${entry.source}`,
      );
      process.exit(1);
    }
    seen.set(entry.challenge.id, entry.source);
  }
}

const questionsDir = path.join(sourceDir, "questions");
if (!fs.existsSync(questionsDir)) {
  console.error(`error: Type Challenges questions directory not found: ${questionsDir}`);
  process.exit(1);
}

const entries = walkTestCases(questionsDir)
  .map((testCasesPath) => {
    const source = path.relative(sourceDir, testCasesPath).split(path.sep).join("/");
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
        path.basename(path.dirname(testCasesPath)),
        source,
      ),
    };
  })
  .sort((left, right) => left.source.localeCompare(right.source));

if (entries.length === 0) {
  console.error(`error: no Type Challenges test-case sources found under ${questionsDir}`);
  process.exit(1);
}

validateUniqueChallengeIds(entries);

if (entries.length !== expectedGenerated) {
  console.error(
    `error: manifest has ${entries.length} entries; expected ${expectedGenerated} for ${ref}`,
  );
  process.exit(1);
}

const manifest = {
  fixture: "type-challenges-project",
  source: {
    repository,
    ref,
    path: "questions/**/test-cases.ts",
  },
  expectedGenerated,
  generated: entries.length,
  entries,
};

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
