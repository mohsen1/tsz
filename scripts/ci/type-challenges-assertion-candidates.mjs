#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const [
  pairingReportPath,
  typeChallengesCompileDir,
  solutionsCompileDir,
  outputDir,
  manifestPath,
] = process.argv.slice(2);

if (
  !pairingReportPath ||
  !typeChallengesCompileDir ||
  !solutionsCompileDir ||
  !outputDir ||
  !manifestPath
) {
  console.error(
    "usage: type-challenges-assertion-candidates.mjs <pairing-report> <type-challenges-compile-dir> <solutions-compile-dir> <output-dir> <manifest.json>",
  );
  process.exit(2);
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function readRequiredFile(file, label) {
  if (!fs.existsSync(file)) {
    console.error(`error: ${label} does not exist: ${file}`);
    process.exit(1);
  }
  return fs.readFileSync(file, "utf8");
}

function identifierPattern(name) {
  return new RegExp(`(^|[^A-Za-z0-9_$])${escapeRegExp(name)}([^A-Za-z0-9_$]|$)`);
}

function escapeRegExp(text) {
  return text.replace(/[\\^$.*+?()[\]{}|]/g, "\\$&");
}

function safeSegment(text) {
  return String(text)
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 120);
}

function ensurePairingReportShape(report) {
  if (report?.fixture !== "type-challenges-readiness-pairing") {
    console.error(
      `error: unexpected Type Challenges pairing report fixture: ${report?.fixture || "<missing>"}`,
    );
    process.exit(1);
  }

  const pairs = report.pairedSolutions;
  if (!Array.isArray(pairs)) {
    console.error("error: Type Challenges pairing report has no pairedSolutions array");
    process.exit(1);
  }

  const expectedPairs = Number(report?.counts?.pairedSolutions);
  if (!Number.isInteger(expectedPairs) || expectedPairs !== pairs.length) {
    console.error(
      [
        "error: Type Challenges pairing report count metadata is inconsistent",
        `pairedSolutions: ${pairs.length}`,
        `counts.pairedSolutions: ${Number.isInteger(expectedPairs) ? expectedPairs : "<missing>"}`,
      ].join("\n"),
    );
    process.exit(1);
  }
  if (pairs.length === 0) {
    console.error("error: Type Challenges pairing report has no paired solutions");
    process.exit(1);
  }

  for (const countName of ["solutionsMissingTemplates", "solutionsMissingTestCases"]) {
    const value = report?.counts?.[countName];
    if (value !== 0) {
      console.error(
        `error: Type Challenges pairing report count ${countName} must be 0; got ${Number.isInteger(value) ? value : "<missing>"}`,
      );
      process.exit(1);
    }
  }

  for (const missingName of ["solutionsMissingTemplates", "solutionsMissingTestCases"]) {
    const entries = report?.missing?.[missingName];
    if (!Array.isArray(entries) || entries.length !== 0) {
      console.error(
        `error: Type Challenges pairing report missing.${missingName} must be an empty array; got ${Array.isArray(entries) ? entries.length : "<missing>"}`,
      );
      process.exit(1);
    }
  }

  for (const label of ["templates", "testCases", "solutions"]) {
    const source = report?.sources?.[label];
    if (source?.repository && source?.ref) continue;

    console.error(
      [
        `error: Type Challenges pairing report is missing ${label} source metadata`,
        `${source?.repository || "<missing repository>"} @ ${source?.ref || "<missing ref>"}`,
      ].join("\n"),
    );
    process.exit(1);
  }

  const templateSource = report.sources.templates;
  const testCaseSource = report.sources.testCases;
  if (
    templateSource.repository !== testCaseSource.repository ||
    templateSource.ref !== testCaseSource.ref
  ) {
    console.error(
      [
        "error: Type Challenges pairing report template and test-case sources come from different snapshots",
        `templates: ${templateSource.repository} @ ${templateSource.ref}`,
        `testCases: ${testCaseSource.repository} @ ${testCaseSource.ref}`,
      ].join("\n"),
    );
    process.exit(1);
  }
}

function requiredString(value, label) {
  if (typeof value === "string" && value.trim().length > 0) {
    return value;
  }

  console.error(`error: Type Challenges pairing report has missing ${label}`);
  process.exit(1);
}

function ensureRelativePath(value, label) {
  const text = requiredString(value, label);
  const normalized = text.replace(/\\/g, "/").replace(/^(?:\.\/)+/, "");
  const segments = normalized.split("/");
  if (
    !path.isAbsolute(text) &&
    !/^[A-Za-z]:\//.test(normalized) &&
    normalized !== "" &&
    normalized !== "." &&
    segments.every((segment) => segment.length > 0 && segment !== "." && segment !== "..")
  ) {
    return normalized;
  }

  console.error(
    `error: Type Challenges pairing report ${label} must be a relative path inside the pairing root: ${text}`,
  );
  process.exit(1);
}

function solutionDeclarations(pair, index) {
  const declarations = pair?.solution?.declarations;
  if (!Array.isArray(declarations)) {
    console.error(
      `error: Type Challenges pairing report pair ${index} has no solution declarations array`,
    );
    process.exit(1);
  }

  const names = declarations.filter(
    (name) => typeof name === "string" && name.length > 0,
  );
  if (names.length === declarations.length && names.length > 0) {
    return names;
  }

  console.error(
    `error: Type Challenges pairing report pair ${index} has no solution declarations`,
  );
  process.exit(1);
}

function ensureChallengeId(entry, expectedId, label) {
  const actualId = entry?.challenge?.id;
  if (String(actualId) === expectedId) {
    return entry.challenge;
  }

  console.error(
    [
      `error: Type Challenges pairing report ${label} challenge id mismatch`,
      `expected: ${expectedId}`,
      `actual: ${actualId == null ? "<missing>" : String(actualId)}`,
    ].join("\n"),
  );
  process.exit(1);
}

function challengeField(entry, field) {
  const value = entry?.challenge?.[field];
  return value == null ? "" : String(value);
}

function ensurePairedChallengeMetadataMatches(pair, index) {
  const templateLevel = challengeField(pair.template, "level");
  const testCaseLevel = challengeField(pair.testCase, "level");
  const templateSlug = challengeField(pair.template, "slug");
  const testCaseSlug = challengeField(pair.testCase, "slug");
  const solutionLevel = challengeField(pair.solution, "level");

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
      `error: Type Challenges pairing report paired challenge metadata mismatch for pair ${index}`,
      ...mismatches,
    ].join("\n"),
  );
  process.exit(1);
}

function candidateFileName(pair) {
  const sourceBase = path
    .basename(pair.solution.source ?? pair.solution.output, path.extname(pair.solution.source ?? ""))
    .replace(/^0+/, "");
  const id = safeSegment(pair.id);
  const base = safeSegment(sourceBase) || "challenge";
  return `${id}-${base}.ts`;
}

function validatePairs(pairs) {
  const ids = new Set();
  const outputs = new Set();

  return pairs.map((pair, index) => {
    const id = requiredString(pair?.id, `pairedSolutions[${index}].id`);
    if (ids.has(id)) {
      console.error(`error: duplicate Type Challenges paired solution id ${id}`);
      process.exit(1);
    }
    ids.add(id);

    const normalized = {
      ...pair,
      id,
      solution: {
        ...pair?.solution,
        challenge: ensureChallengeId(
          pair?.solution,
          id,
          `pairedSolutions[${index}].solution`,
        ),
        output: ensureRelativePath(
          pair?.solution?.output,
          `pairedSolutions[${index}].solution.output`,
        ),
        source: ensureRelativePath(
          pair?.solution?.source,
          `pairedSolutions[${index}].solution.source`,
        ),
        declarations: solutionDeclarations(pair, index),
      },
      template: {
        ...pair?.template,
        challenge: ensureChallengeId(
          pair?.template,
          id,
          `pairedSolutions[${index}].template`,
        ),
        output: ensureRelativePath(
          pair?.template?.output,
          `pairedSolutions[${index}].template.output`,
        ),
        source: ensureRelativePath(
          pair?.template?.source,
          `pairedSolutions[${index}].template.source`,
        ),
      },
      testCase: {
        ...pair?.testCase,
        challenge: ensureChallengeId(
          pair?.testCase,
          id,
          `pairedSolutions[${index}].testCase`,
        ),
        output: ensureRelativePath(
          pair?.testCase?.output,
          `pairedSolutions[${index}].testCase.output`,
        ),
        source: ensureRelativePath(
          pair?.testCase?.source,
          `pairedSolutions[${index}].testCase.source`,
        ),
      },
    };

    ensurePairedChallengeMetadataMatches(normalized, index);

    const output = path.join("assertions", candidateFileName(normalized));
    if (outputs.has(output)) {
      console.error(
        `error: duplicate Type Challenges assertion candidate output ${output}`,
      );
      process.exit(1);
    }
    outputs.add(output);

    return normalized;
  });
}

const report = readJson(pairingReportPath);
ensurePairingReportShape(report);
const pairs = validatePairs(report.pairedSolutions ?? []);
fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(path.join(outputDir, "assertions"), { recursive: true });
fs.mkdirSync(path.join(outputDir, "utils"), { recursive: true });

const typeChallengesUtils = path.join(typeChallengesCompileDir, "utils", "index.d.ts");
const utilsText = readRequiredFile(typeChallengesUtils, "Type Challenges utils");
fs.writeFileSync(path.join(outputDir, "utils", "index.d.ts"), utilsText);

const entries = [];

for (const pair of pairs) {
  const declarations = pair.solution.declarations ?? [];
  const solutionPath = path.join(solutionsCompileDir, pair.solution.output);
  const templatePath = path.join(typeChallengesCompileDir, pair.template.output);
  const testCasePath = path.join(
    typeChallengesCompileDir,
    "test-cases",
    pair.testCase.output,
  );
  const solutionText = readRequiredFile(solutionPath, "solution source");
  readRequiredFile(templatePath, "template source");
  const testCaseText = readRequiredFile(testCasePath, "test-case source");
  const referencedSolutionDeclarations = declarations.filter((name) =>
    identifierPattern(name).test(testCaseText),
  );

  const output = path.join("assertions", candidateFileName(pair));
  const outputPath = path.join(outputDir, output);
  fs.writeFileSync(
    outputPath,
    [
      `// Generated Type Challenges assertion candidate for challenge ${pair.id}.`,
      `// Solution source: ${pair.solution.source}`,
      `// Test-case source: ${pair.testCase.source}`,
      "",
      solutionText.trimEnd(),
      "",
      testCaseText.trimEnd(),
      "",
    ].join("\n"),
  );

  entries.push({
    id: pair.id,
    output,
    solution: pair.solution,
    template: pair.template,
    testCase: pair.testCase,
    assertion: {
      referencedSolutionDeclarations,
      hasReferencedSolutionDeclaration: referencedSolutionDeclarations.length > 0,
    },
  });
}

const manifest = {
  fixture: "type-challenges-assertion-candidates",
  sources: report.sources,
  counts: {
    pairedSolutions: pairs.length,
    generatedAssertions: entries.length,
    assertionsReferencingSolutionDeclaration: entries.filter(
      (entry) => entry.assertion.hasReferencedSolutionDeclaration,
    ).length,
    assertionsMissingSolutionDeclarationReference: entries.filter(
      (entry) => !entry.assertion.hasReferencedSolutionDeclaration,
    ).length,
  },
  entries,
};

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

const tsconfigPath = path.join(outputDir, "tsconfig.tsz-guard.json");
fs.writeFileSync(
  tsconfigPath,
  `${JSON.stringify(
    {
      compilerOptions: {
        target: "es2017",
        lib: ["ESNext"],
        module: "commonjs",
        moduleResolution: "node",
        strict: true,
        noEmit: true,
        types: [],
        noImplicitReturns: true,
        noUnusedLocals: false,
        noUnusedParameters: false,
        esModuleInterop: true,
        skipLibCheck: true,
        ignoreDeprecations: "6.0",
        baseUrl: ".",
        paths: {
          "@type-challenges/utils": ["utils/index.d.ts"],
        },
      },
      include: ["assertions/**/*.ts", "utils/index.d.ts"],
    },
    null,
    2,
  )}\n`,
);

console.log(
  [
    `generated ${manifest.counts.generatedAssertions} Type Challenges assertion candidates`,
    `with declaration references: ${manifest.counts.assertionsReferencingSolutionDeclaration}`,
    `missing declaration references: ${manifest.counts.assertionsMissingSolutionDeclarationReference}`,
    `manifest: ${path.relative(process.cwd(), manifestPath).split(path.sep).join("/")}`,
  ].join("\n"),
);
