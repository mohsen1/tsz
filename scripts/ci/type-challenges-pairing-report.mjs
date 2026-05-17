#!/usr/bin/env node
import fs from "node:fs";

const [templateManifestPath, testCasesManifestPath, solutionsManifestPath, outputPath] =
  process.argv.slice(2);

if (
  !templateManifestPath ||
  !testCasesManifestPath ||
  !solutionsManifestPath ||
  !outputPath
) {
  console.error(
    "usage: type-challenges-pairing-report.mjs <template-manifest> <test-cases-manifest> <solutions-manifest> <output.json>",
  );
  process.exit(2);
}

function readManifest(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function validateManifest(manifest, label, expected) {
  if (manifest?.fixture !== expected.fixture) {
    fail(
      `unexpected ${label} manifest fixture: ${manifest?.fixture || "<missing>"}`,
    );
  }

  const source = manifest.source;
  if (
    !source ||
    typeof source.repository !== "string" ||
    source.repository.length === 0 ||
    typeof source.ref !== "string" ||
    source.ref.length === 0 ||
    typeof source.path !== "string" ||
    source.path.length === 0
  ) {
    fail(`${label} manifest is missing source repository/ref/path metadata`);
  }

  if (source.path !== expected.path) {
    fail(
      `${label} manifest source path mismatch: expected ${expected.path}, got ${source.path}`,
    );
  }

  if (!Array.isArray(manifest.entries)) {
    fail(`${label} manifest entries must be an array`);
  }

  if (!Number.isInteger(manifest.generated)) {
    fail(`${label} manifest is missing generated count metadata`);
  }

  if (manifest.generated !== manifest.entries.length) {
    fail(
      `${label} manifest generated count ${manifest.generated} does not match entries length ${manifest.entries.length}`,
    );
  }
}

function ensureSameOfficialSnapshot(templateManifest, testCasesManifest) {
  const templateSource = templateManifest.source;
  const testCasesSource = testCasesManifest.source;
  if (
    templateSource.repository === testCasesSource.repository &&
    templateSource.ref === testCasesSource.ref
  ) {
    return;
  }

  fail(
    [
      "Type Challenges template and test-case manifests must come from the same official snapshot",
      `templates: ${templateSource.repository} @ ${templateSource.ref}`,
      `test cases: ${testCasesSource.repository} @ ${testCasesSource.ref}`,
    ].join("\n"),
  );
}

function challengeId(entry) {
  const id = entry?.challenge?.id;
  return id == null ? null : String(id);
}

function indexByChallengeId(manifest, label) {
  const byId = new Map();
  for (const entry of manifest.entries ?? []) {
    const id = challengeId(entry);
    if (id == null) {
      console.error(
        `error: ${label} manifest entry has no challenge id: ${entry.source}`,
      );
      process.exit(1);
    }
    if (byId.has(id)) {
      console.error(`error: duplicate ${label} challenge id ${id}`);
      process.exit(1);
    }
    byId.set(id, entry);
  }
  return byId;
}

function summarizeEntry(entry) {
  const summary = {
    output: entry.output,
    source: entry.source,
    challenge: entry.challenge,
  };
  if (Array.isArray(entry.declarations)) {
    summary.declarations = entry.declarations;
  }
  return summary;
}

const templateManifest = readManifest(templateManifestPath);
const testCasesManifest = readManifest(testCasesManifestPath);
const solutionsManifest = readManifest(solutionsManifestPath);

validateManifest(templateManifest, "template", {
  fixture: "type-challenges-project",
  path: "questions/**/template.ts",
});
validateManifest(testCasesManifest, "test-case", {
  fixture: "type-challenges-project",
  path: "questions/**/test-cases.ts",
});
validateManifest(solutionsManifest, "solution", {
  fixture: "type-challenges-solutions-project",
  path: "en/*.md",
});
ensureSameOfficialSnapshot(templateManifest, testCasesManifest);

const templatesById = indexByChallengeId(templateManifest, "template");
const testCasesById = indexByChallengeId(testCasesManifest, "test-case");
const solutionsById = indexByChallengeId(solutionsManifest, "solution");

const pairedSolutions = [];
const solutionsMissingTemplates = [];
const solutionsMissingTestCases = [];

for (const [id, solution] of solutionsById) {
  const template = templatesById.get(id);
  const testCase = testCasesById.get(id);

  if (!template) {
    solutionsMissingTemplates.push(summarizeEntry(solution));
  }
  if (!testCase) {
    solutionsMissingTestCases.push(summarizeEntry(solution));
  }
  if (template && testCase) {
    pairedSolutions.push({
      id,
      solution: summarizeEntry(solution),
      template: summarizeEntry(template),
      testCase: summarizeEntry(testCase),
    });
  }
}

const testCasesMissingSolutions = [...testCasesById]
  .filter(([id]) => !solutionsById.has(id))
  .map(([, entry]) => summarizeEntry(entry));

if (solutionsMissingTemplates.length > 0 || solutionsMissingTestCases.length > 0) {
  console.error(
    [
      "error: Type Challenges solution entries are missing official assertion sources",
      `solutions without templates: ${solutionsMissingTemplates.length}`,
      `solutions without test cases: ${solutionsMissingTestCases.length}`,
    ].join("\n"),
  );
  process.exit(1);
}

const report = {
  fixture: "type-challenges-readiness-pairing",
  sources: {
    templates: templateManifest.source,
    testCases: testCasesManifest.source,
    solutions: solutionsManifest.source,
  },
  counts: {
    templates: templatesById.size,
    testCases: testCasesById.size,
    solutions: solutionsById.size,
    pairedSolutions: pairedSolutions.length,
    solutionsMissingTemplates: solutionsMissingTemplates.length,
    solutionsMissingTestCases: solutionsMissingTestCases.length,
    testCasesMissingSolutions: testCasesMissingSolutions.length,
  },
  pairedSolutions,
  missing: {
    solutionsMissingTemplates,
    solutionsMissingTestCases,
    testCasesMissingSolutions,
  },
};

fs.writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);
