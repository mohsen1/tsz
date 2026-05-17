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
}

function candidateFileName(pair) {
  const sourceBase = path
    .basename(pair.solution.source ?? pair.solution.output, path.extname(pair.solution.source ?? ""))
    .replace(/^0+/, "");
  const id = safeSegment(pair.id);
  const base = safeSegment(sourceBase) || "challenge";
  return `${id}-${base}.ts`;
}

const report = readJson(pairingReportPath);
ensurePairingReportShape(report);
const pairs = report.pairedSolutions ?? [];
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
  const testCasePath = path.join(
    typeChallengesCompileDir,
    "test-cases",
    pair.testCase.output,
  );
  const solutionText = readRequiredFile(solutionPath, "solution source");
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
