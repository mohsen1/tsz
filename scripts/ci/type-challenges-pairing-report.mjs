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

function challengeField(entry, field) {
  const value = entry?.challenge?.[field];
  return value == null ? "" : String(value);
}

function ensureChallengeMetadataMatches(id, template, testCase, solution) {
  const templateLevel = challengeField(template, "level");
  const testCaseLevel = challengeField(testCase, "level");
  const templateSlug = challengeField(template, "slug");
  const testCaseSlug = challengeField(testCase, "slug");
  const solutionLevel = challengeField(solution, "level");

  const mismatches = [];
  if (templateLevel !== testCaseLevel) {
    mismatches.push(`template/test-case level: ${templateLevel || "<missing>"} vs ${testCaseLevel || "<missing>"}`);
  }
  if (templateSlug !== testCaseSlug) {
    mismatches.push(`template/test-case slug: ${templateSlug || "<missing>"} vs ${testCaseSlug || "<missing>"}`);
  }
  if (solutionLevel && templateLevel && solutionLevel !== templateLevel) {
    mismatches.push(`solution/template level: ${solutionLevel} vs ${templateLevel}`);
  }

  if (mismatches.length === 0) return;

  console.error(
    [
      `error: Type Challenges paired source metadata mismatch for challenge id ${id}`,
      ...mismatches,
    ].join("\n"),
  );
  process.exit(1);
}

function sourceField(manifest, field) {
  const value = manifest?.source?.[field];
  return value == null ? "" : String(value);
}

function describeManifestSource(label, manifest) {
  return `${label}: ${sourceField(manifest, "repository") || "<missing repository>"} @ ${sourceField(manifest, "ref") || "<missing ref>"}`;
}

function ensurePinnedSource(label, manifest) {
  if (sourceField(manifest, "repository") && sourceField(manifest, "ref")) return;

  console.error(
    [
      `error: Type Challenges ${label} manifest is missing pinned source metadata`,
      describeManifestSource(label, manifest),
    ].join("\n"),
  );
  process.exit(1);
}

function ensureManifestShape(label, manifest, expectedFixture, expectedPath) {
  const fixture = manifest?.fixture == null ? "" : String(manifest.fixture);
  const sourcePath = sourceField(manifest, "path");

  if (fixture === expectedFixture && sourcePath === expectedPath) return;

  console.error(
    [
      `error: Type Challenges ${label} manifest has unexpected fixture metadata`,
      `expected: ${expectedFixture} @ ${expectedPath}`,
      `actual: ${fixture || "<missing fixture>"} @ ${sourcePath || "<missing source path>"}`,
    ].join("\n"),
  );
  process.exit(1);
}

function ensureManifestEntries(label, manifest) {
  const entries = manifest?.entries;
  const generated = Number(manifest?.generated);
  const expectedGenerated = Number(manifest?.expectedGenerated);

  if (!Array.isArray(entries) || entries.length === 0) {
    console.error(`error: Type Challenges ${label} manifest has no entries`);
    process.exit(1);
  }

  if (
    !Number.isInteger(generated) ||
    !Number.isInteger(expectedGenerated) ||
    generated !== entries.length ||
    expectedGenerated !== entries.length
  ) {
    console.error(
      [
        `error: Type Challenges ${label} manifest count metadata is inconsistent`,
        `entries: ${entries.length}`,
        `generated: ${Number.isInteger(generated) ? generated : "<missing generated>"}`,
        `expectedGenerated: ${Number.isInteger(expectedGenerated) ? expectedGenerated : "<missing expectedGenerated>"}`,
      ].join("\n"),
    );
    process.exit(1);
  }
}

function ensureSolutionDeclarations(manifest) {
  for (const entry of manifest.entries ?? []) {
    const declarations = Array.isArray(entry?.declarations)
      ? entry.declarations.map(String).filter(Boolean)
      : [];
    if (declarations.length > 0) continue;

    console.error(
      [
        "error: Type Challenges solution manifest entry has no declarations",
        `source: ${entry?.source || "<missing source>"}`,
        `challenge id: ${challengeId(entry) || "<missing id>"}`,
      ].join("\n"),
    );
    process.exit(1);
  }
}

function ensureSourcesAreCompatible(templateManifest, testCasesManifest, solutionsManifest) {
  ensureManifestShape(
    "template",
    templateManifest,
    "type-challenges-project",
    "questions/**/template.ts",
  );
  ensureManifestShape(
    "test-case",
    testCasesManifest,
    "type-challenges-project",
    "questions/**/test-cases.ts",
  );
  ensureManifestShape(
    "solution",
    solutionsManifest,
    "type-challenges-solutions-project",
    "en/*.md",
  );

  ensureManifestEntries("template", templateManifest);
  ensureManifestEntries("test-case", testCasesManifest);
  ensureManifestEntries("solution", solutionsManifest);
  ensureSolutionDeclarations(solutionsManifest);

  ensurePinnedSource("template", templateManifest);
  ensurePinnedSource("test-case", testCasesManifest);
  ensurePinnedSource("solution", solutionsManifest);

  const templateRepo = sourceField(templateManifest, "repository");
  const testCasesRepo = sourceField(testCasesManifest, "repository");
  const templateRef = sourceField(templateManifest, "ref");
  const testCasesRef = sourceField(testCasesManifest, "ref");

  if (templateRepo === testCasesRepo && templateRef === testCasesRef) return;

  console.error(
    [
      "error: Type Challenges template and test-case manifests come from different source snapshots",
      describeManifestSource("template", templateManifest),
      describeManifestSource("test-case", testCasesManifest),
    ].join("\n"),
  );
  process.exit(1);
}

const templateManifest = readManifest(templateManifestPath);
const testCasesManifest = readManifest(testCasesManifestPath);
const solutionsManifest = readManifest(solutionsManifestPath);

ensureSourcesAreCompatible(templateManifest, testCasesManifest, solutionsManifest);

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
    ensureChallengeMetadataMatches(id, template, testCase, solution);
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
