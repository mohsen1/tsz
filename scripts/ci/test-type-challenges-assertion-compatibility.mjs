#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { REQUIRED_COMPATIBILITY_FIELDS } from "../bench/project-rows.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(SCRIPT_DIR, "..", "..");
const SCRIPT = path.join(
  ROOT,
  "scripts",
  "ci",
  "type-challenges-assertion-compatibility.mjs",
);

function withTempDir(fn) {
  const dir = fs.mkdtempSync(
    path.join(os.tmpdir(), "tsz-type-challenges-assertion-compat-"),
  );
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function readRows(file) {
  return fs.readFileSync(file, "utf8")
    .trim()
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function withComparisonCompilerStatuses(report) {
  if (!report?.comparison || typeof report.comparison !== "object") {
    return report;
  }

  const candidateUniverse = candidateFileUniverse(report);
  const compilers = Object.fromEntries(
    Object.entries(report.compilers ?? {}).map(([compiler, result]) => [
      compiler,
      withDiagnosticFreeFileList(result, candidateUniverse),
    ]),
  );

  return {
    ...report,
    compilers,
    comparison: withCandidateFileComparison(compilers, {
      tscStatus: compilers.tsc?.status,
      tszStatus: compilers.tsz?.status,
      ...report.comparison,
    }),
  };
}

function candidateFileUniverse(report) {
  const files = [];
  for (const result of Object.values(report.compilers ?? {})) {
    const diagnostics = result?.candidateDiagnostics;
    for (const field of ["filesWithDiagnostics", "filesWithoutDiagnostics"]) {
      for (const file of diagnostics?.[field] ?? []) {
        if (!files.includes(file)) {
          files.push(file);
        }
      }
    }
  }

  const totalCandidates = Math.max(
    0,
    ...Object.values(report.compilers ?? {})
      .map((result) => result?.candidateDiagnostics?.totalCandidates)
      .filter(Number.isInteger),
  );
  while (files.length < totalCandidates) {
    files.push(`assertions/diagnostic-free-${files.length + 1}.ts`);
  }
  return files;
}

function withDiagnosticFreeFileList(result, candidateUniverse) {
  const diagnostics = result?.candidateDiagnostics;
  if (
    !diagnostics ||
    Object.prototype.hasOwnProperty.call(diagnostics, "filesWithoutDiagnostics") ||
    !Number.isInteger(diagnostics.candidatesWithoutDiagnostics)
  ) {
    return result;
  }

  return {
    ...result,
    candidateDiagnostics: {
      ...diagnostics,
      filesWithoutDiagnostics: candidateUniverse
        .filter((file) => !(diagnostics.filesWithDiagnostics || []).includes(file))
        .slice(0, diagnostics.candidatesWithoutDiagnostics),
    },
  };
}

function withCandidateFileComparison(compilers, comparison) {
  if (
    Object.prototype.hasOwnProperty.call(comparison, "candidateFileComparison") ||
    !hasConcreteCandidateFileLists(compilers.tsc) ||
    !hasConcreteCandidateFileLists(compilers.tsz)
  ) {
    return comparison;
  }

  const tscDiagnostics = compilers.tsc.candidateDiagnostics;
  const tszDiagnostics = compilers.tsz.candidateDiagnostics;
  const bothAccepted = intersectSorted(
    tscDiagnostics.filesWithoutDiagnostics,
    tszDiagnostics.filesWithoutDiagnostics,
  );
  const bothRejected = intersectSorted(
    tscDiagnostics.filesWithDiagnostics || [],
    tszDiagnostics.filesWithDiagnostics || [],
  );
  const tscAcceptedTszRejected = intersectSorted(
    tscDiagnostics.filesWithoutDiagnostics,
    tszDiagnostics.filesWithDiagnostics || [],
  );
  const tscRejectedTszAccepted = intersectSorted(
    tscDiagnostics.filesWithDiagnostics || [],
    tszDiagnostics.filesWithoutDiagnostics,
  );

  return {
    ...comparison,
    candidateFileComparison: {
      totalCandidates: tscDiagnostics.totalCandidates,
      counts: {
        bothAccepted: bothAccepted.length,
        bothRejected: bothRejected.length,
        tscAcceptedTszRejected: tscAcceptedTszRejected.length,
        tscRejectedTszAccepted: tscRejectedTszAccepted.length,
      },
      bothAccepted,
      bothRejected,
      tscAcceptedTszRejected,
      tscRejectedTszAccepted,
    },
  };
}

function hasConcreteCandidateFileLists(result) {
  const diagnostics = result?.candidateDiagnostics;
  return (
    diagnostics &&
    Number.isInteger(diagnostics.totalCandidates) &&
    Number.isInteger(diagnostics.candidatesWithDiagnostics) &&
    Number.isInteger(diagnostics.candidatesWithoutDiagnostics) &&
    (diagnostics.candidatesWithDiagnostics === 0 ||
      Array.isArray(diagnostics.filesWithDiagnostics)) &&
    (diagnostics.candidatesWithoutDiagnostics === 0 ||
      Array.isArray(diagnostics.filesWithoutDiagnostics))
  );
}

function intersectSorted(left, right) {
  const rightSet = new Set(right);
  return left.filter((file) => rightSet.has(file)).sort();
}

function assertRequiredCompatibilityFields(row) {
  for (const field of REQUIRED_COMPATIBILITY_FIELDS) {
    assert.ok(
      Object.prototype.hasOwnProperty.call(row, field),
      `assertion compatibility row is missing ${field}`,
    );
  }
}

function candidateCounts(generatedAssertions = 1) {
  return {
    pairedSolutions: generatedAssertions,
    generatedAssertions,
    assertionsReferencingSolutionDeclaration: generatedAssertions,
    assertionsMissingSolutionDeclarationReference: 0,
  };
}

function candidateSources() {
  return {
    templates: { repository: "type", ref: "type-ref" },
    testCases: { repository: "type", ref: "type-ref" },
    solutions: { repository: "solutions", ref: "solutions-ref" },
  };
}

function candidateManifest(generatedAssertions = 1) {
  return {
    sources: candidateSources(),
    counts: candidateCounts(generatedAssertions),
  };
}

function cleanCandidateManifest({
  tscAcceptedAssertions = 1,
  referencing = tscAcceptedAssertions,
  missing = 0,
  totalCandidates = tscAcceptedAssertions,
  rejected = totalCandidates - tscAcceptedAssertions,
} = {}) {
  return {
    fixture: "type-challenges-assertions-tsc-clean",
    sources: candidateSources(),
    counts: {
      totalCandidates,
      tscAcceptedAssertions,
      tscAcceptedAssertionsReferencingSolutionDeclaration: referencing,
      tscAcceptedAssertionsMissingSolutionDeclarationReference: missing,
      tscRejectedAssertions: rejected,
    },
  };
}

function runCompatibility({ dir, classification, cleanSubsetManifest = null, cleanSubsetClassification = null }) {
  const candidateDir = path.join(dir, "type-challenges-assertions");
  const classificationPath = path.join(candidateDir, "classification.json");
  const cleanSubsetDir = path.join(dir, "type-challenges-assertions-tsc-clean");
  const cleanSubsetManifestPath = path.join(cleanSubsetDir, "manifest.json");
  const cleanSubsetClassificationPath = path.join(cleanSubsetDir, "classification.json");
  const outFile = path.join(dir, "project-compatibility.jsonl");
  writeJson(classificationPath, withComparisonCompilerStatuses(classification));
  if (cleanSubsetManifest) {
    writeJson(cleanSubsetManifestPath, cleanSubsetManifest);
  }
  if (cleanSubsetClassification) {
    writeJson(
      cleanSubsetClassificationPath,
      withComparisonCompilerStatuses(cleanSubsetClassification),
    );
  }

  const result = spawnSync(
    process.execPath,
    [
      SCRIPT,
      classificationPath,
      candidateDir,
      outFile,
      dir,
      cleanSubsetManifest ? cleanSubsetManifestPath : "",
      cleanSubsetClassification ? cleanSubsetClassificationPath : "",
      cleanSubsetDir,
    ],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return readRows(outFile)[0];
}

function runCompatibilityRaw({
  dir,
  classification,
  cleanSubsetManifest = null,
  cleanSubsetClassification = null,
}) {
  const candidateDir = path.join(dir, "type-challenges-assertions");
  const classificationPath = path.join(candidateDir, "classification.json");
  const cleanSubsetDir = path.join(dir, "type-challenges-assertions-tsc-clean");
  const cleanSubsetManifestPath = path.join(cleanSubsetDir, "manifest.json");
  const cleanSubsetClassificationPath = path.join(cleanSubsetDir, "classification.json");
  const outFile = path.join(dir, "project-compatibility.jsonl");
  writeJson(classificationPath, withComparisonCompilerStatuses(classification));
  if (cleanSubsetManifest) {
    writeJson(cleanSubsetManifestPath, cleanSubsetManifest);
  }
  if (cleanSubsetClassification) {
    writeJson(
      cleanSubsetClassificationPath,
      withComparisonCompilerStatuses(cleanSubsetClassification),
    );
  }

  return {
    outFile,
    result: spawnSync(
      process.execPath,
      [
        SCRIPT,
        classificationPath,
        candidateDir,
        outFile,
        dir,
        cleanSubsetManifest ? cleanSubsetManifestPath : "",
        cleanSubsetClassification ? cleanSubsetClassificationPath : "",
        cleanSubsetDir,
      ],
      {
        cwd: ROOT,
        encoding: "utf8",
      },
    ),
  };
}

withTempDir((dir) => {
  const row = runCompatibility({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: {
        sources: {
          templates: { repository: "type", ref: "type-ref" },
          testCases: { repository: "type", ref: "type-ref" },
          solutions: { repository: "solutions", ref: "solutions-ref" },
        },
        counts: {
          pairedSolutions: 2,
          generatedAssertions: 2,
          assertionsReferencingSolutionDeclaration: 1,
          assertionsMissingSolutionDeclarationReference: 1,
        },
      },
      compilers: {
        tsc: {
          status: "fail",
          exitCode: 1,
          diagnostics: {
            firstErrors: ["assertions/one.ts(1,1): error TS2344: mismatch"],
            byCode: [{ key: "TS2344", count: 1 }],
          },
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [
              {
                file: ".\\assertions\\one.ts",
                errorCount: 1,
                codes: [{ key: "TS2344", count: 1 }],
                semanticFamilies: ["mapped/key-remapped types"],
                firstErrors: [
                  {
                    line: 1,
                    column: 1,
                    code: "TS2344",
                    message: "mismatch",
                  },
                ],
                candidate: {
                  id: "00001-easy-pick",
                  solution: {
                    output: "solutions/one.ts",
                    source: "questions/00001-easy-pick/README.md",
                    declarations: ["MyPick"],
                  },
                  testCase: {
                    output: "test-cases/one.ts",
                    source: "questions/00001-easy-pick/test-cases.ts",
                  },
                  assertion: {
                    hasReferencedSolutionDeclaration: true,
                    referencedSolutionDeclarations: ["MyPick"],
                  },
                },
              },
            ],
          },
        },
        tsz: {
          status: "pass",
          exitCode: 0,
          candidateDiagnostics: {
            candidatesWithoutDiagnostics: 2,
          },
        },
      },
      comparison: {
        status: "tsz-accepts-tsc-rejected",
        diagnosticFreeCandidateDelta: 1,
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 1,
            bothRejected: 0,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 1,
          },
          bothAccepted: ["assertions/two.ts"],
          bothRejected: [],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: ["assertions/one.ts"],
        },
        bySemanticFamilyDelta: [{ key: "mapped/key-remapped types", delta: -1 }],
      },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ id: "two", output: "assertions/two.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        referencing: 1,
        missing: 0,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(row.name, "type-challenges-assertion-candidates");
  assertRequiredCompatibilityFields(row);
  assert.equal(row.state, "gray");
  assert.equal(row.exit_class, "fixture invalid");
  assert.equal(row.first_failure_class, "assertion corpus not tsc-clean");
  assert.deepEqual(row.known_blockers, ["assertion corpus not tsc-clean"]);
  assert.deepEqual(row.diagnostic_codes, ["TS2344"]);
  assert.deepEqual(row.diagnostic_subsystems, [
    {
      subsystem: "type-challenges mapped/key-remapped types",
      codes: [],
      count: 1,
      examples: [],
    },
  ]);
  assert.equal(
    row.repro.tsconfig_path,
    "type-challenges-assertions/tsconfig.tsz-guard.json",
  );
  assert.equal(
    row.repro.first_failure_path,
    "type-challenges-assertions/assertions/one.ts",
  );
  assert.deepEqual(row.exit_codes, { tsc: [1], tsz: [0], tsgo: [] });
  assert.deepEqual(row.assertion_candidates, {
    sources: {
      templates: { repository: "type", ref: "type-ref" },
      testCases: { repository: "type", ref: "type-ref" },
      solutions: { repository: "solutions", ref: "solutions-ref" },
    },
    paired_solutions: 2,
    generated_assertions: 2,
    assertions_referencing_solution_declaration: 1,
    assertions_missing_solution_declaration_reference: 1,
    tsc_diagnostic_free: 1,
    tsc_with_diagnostics: 1,
    tsz_diagnostic_free: 2,
    diagnostic_free_candidate_delta: 1,
    both_accepted: 1,
    both_rejected: 0,
    tsc_accepted_tsz_rejected: 0,
    tsc_rejected_tsz_accepted: 1,
    tsc_clean_subset: {
      manifest_path: "type-challenges-assertions-tsc-clean/manifest.json",
      classification_path: "type-challenges-assertions-tsc-clean/classification.json",
      tsconfig_path: "type-challenges-assertions-tsc-clean/tsconfig.tsz-guard.json",
      total_candidates: 2,
      generated_assertions: 1,
      assertions_referencing_solution_declaration: 1,
      assertions_missing_solution_declaration_reference: 0,
      rejected_from_full_corpus: 1,
      tsc_status: "pass",
      tsz_status: "pass",
      comparison_status: "both-pass",
      tsc_diagnostic_free: 1,
      tsz_diagnostic_free: 1,
    },
    file_comparison: {
      total_candidates: 2,
      counts: {
        bothAccepted: 1,
        bothRejected: 0,
        tscAcceptedTszRejected: 0,
        tscRejectedTszAccepted: 1,
      },
      both_accepted: ["type-challenges-assertions/assertions/two.ts"],
      both_rejected: [],
      tsc_accepted_tsz_rejected: [],
      tsc_rejected_tsz_accepted: ["type-challenges-assertions/assertions/one.ts"],
    },
    diagnostic_candidate_examples: [
      {
        compiler: "tsc",
        file: "type-challenges-assertions/assertions/one.ts",
        candidate_id: "00001-easy-pick",
        error_count: 1,
        codes: ["TS2344"],
        semantic_families: ["mapped/key-remapped types"],
        first_error: {
          line: 1,
          column: 1,
          code: "TS2344",
          message: "mismatch",
        },
      },
    ],
  });
});

withTempDir((dir) => {
  const row = runCompatibility({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          exitCode: 0,
          diagnostics: { firstErrors: [], byCode: [] },
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithDiagnostics: 0,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: [],
            filesWithoutDiagnostics: ["assertions/two.ts"],
          },
        },
        tsz: {
          status: "fail",
          exitCode: 1,
          diagnostics: {
            firstErrors: ["assertions/two.ts(2,3): error TS2589: deep"],
            byCode: [{ key: "TS2589", count: 1 }],
          },
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 0,
            filesWithDiagnostics: ["assertions/two.ts"],
          },
        },
      },
      comparison: {
        status: "tsz-rejects-tsc-accepted",
        diagnosticFreeCandidateDelta: -1,
        bySemanticFamilyDelta: [{ key: "recursive conditionals", delta: 1 }],
      },
    },
  });

  assertRequiredCompatibilityFields(row);
  assert.equal(row.state, "red");
  assert.equal(row.exit_class, "nonzero exit");
  assert.equal(row.diagnostic_status, "tsz rejects tsc-accepted assertion candidates");
  assert.equal(row.first_failure_class, "tsz rejects tsc-accepted assertion candidates");
  assert.deepEqual(row.diagnostic_codes, ["TS2589"]);
  assert.equal(
    row.repro.first_failure_path,
    "type-challenges-assertions/assertions/two.ts",
  );
  assert.deepEqual(row.exit_codes, { tsc: [0], tsz: [1], tsgo: [] });
});

withTempDir((dir) => {
  const row = runCompatibility({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          exitCode: 0,
          diagnostics: { firstErrors: [], byCode: [] },
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithDiagnostics: 0,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: [],
          },
        },
        tsz: {
          status: "pass",
          exitCode: 0,
          candidateDiagnostics: {
            candidatesWithoutDiagnostics: 1,
          },
        },
      },
      comparison: {
        status: "both-pass",
        diagnosticFreeCandidateDelta: 0,
        bySemanticFamilyDelta: [],
      },
    },
  });

  assertRequiredCompatibilityFields(row);
  assert.equal(row.state, "green");
  assert.equal(row.exit_class, "exit success");
  assert.equal(row.diagnostic_status, "none");
  assert.equal(row.first_failure_class, null);
  assert.deepEqual(row.known_blockers, []);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "stale-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /unexpected assertion classification fixture/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {},
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification comparison\.status must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            filesWithoutDiagnostics: ["assertions/two.ts"],
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 0,
            candidatesWithoutDiagnostics: 2,
            filesWithDiagnostics: [],
            filesWithoutDiagnostics: ["assertions/one.ts", "assertions/two.ts"],
          },
        },
      },
      comparison: {
        status: "both-pass",
        diagnosticFreeCandidateDelta: 1,
        candidateFileComparison: null,
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /comparison\.candidateFileComparison is required when both compiler candidate file lists are concrete/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/two.ts"],
            filesWithoutDiagnostics: ["assertions/one.ts"],
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            filesWithoutDiagnostics: ["assertions/two.ts"],
          },
        },
      },
      comparison: {
        status: "both-pass",
        diagnosticFreeCandidateDelta: 0,
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 1,
            bothRejected: 1,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["assertions/one.ts"],
          bothRejected: ["assertions/two.ts"],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison\.bothAccepted does not match compiler candidate diagnostic file lists: expected <none>, actual assertions\/one\.ts/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "fail",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 0,
          },
        },
        tsz: {
          status: "fail",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 0,
          },
        },
      },
      comparison: { status: "both-nonpassing" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsc status \(fail\) must be pass/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 0,
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsc candidatesWithoutDiagnostics \(0\) must match manifest tscAcceptedAssertions \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            filesWithoutDiagnostics: ["assertions/two.ts"],
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 0,
            candidatesWithoutDiagnostics: 2,
            filesWithDiagnostics: [],
            filesWithoutDiagnostics: ["assertions/one.ts", "assertions/two.ts"],
          },
        },
      },
      comparison: {
        status: "both-pass",
        diagnosticFreeCandidateDelta: 1,
        candidateFileComparison: null,
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /comparison\.candidateFileComparison is required when both compiler candidate file lists are concrete/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/two.ts"],
            filesWithoutDiagnostics: ["assertions/one.ts"],
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            filesWithoutDiagnostics: ["assertions/two.ts"],
          },
        },
      },
      comparison: {
        status: "both-pass",
        diagnosticFreeCandidateDelta: 0,
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 1,
            bothRejected: 1,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["assertions/one.ts"],
          bothRejected: ["assertions/two.ts"],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison\.bothAccepted does not match compiler candidate diagnostic file lists: expected <none>, actual assertions\/one\.ts/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 0,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: -1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion manifest counts\.tscRejectedAssertions must be non-negative/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: {
        sources: candidateSources(),
        counts: {
          pairedSolutions: 1,
          generatedAssertions: 1,
          assertionsReferencingSolutionDeclaration: -1,
          assertionsMissingSolutionDeclarationReference: 2,
        },
      },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /counts\.assertionsReferencingSolutionDeclaration must be non-negative/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "fail",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 0,
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification comparison\.status \(both-pass\) does not match compiler statuses \(tsz-accepts-tsc-rejected\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "fail" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification comparison\.status \(both-pass\) does not match compiler statuses \(tsz-rejects-tsc-accepted\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
        tsz: {
          status: "fail",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 0,
          },
        },
      },
      comparison: {
        status: "tsz-rejects-tsc-accepted",
        tszStatus: "pass",
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification comparison\.tszStatus \(pass\) does not match compilers\.tsz\.status \(fail\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest(),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
      comparison: { status: "match" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification comparison\.status must be one of .*: match/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification comparison\.status must be one of .*: match/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        tscStatus: "fail",
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification comparison\.tscStatus \(fail\) does not match compilers\.tsc\.status \(pass\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const sources = candidateSources();
  sources.solutions = { repository: "solutions", ref: 123 };
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: {
        sources,
        counts: candidateCounts(1),
      },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateManifest\.sources\.solutions is missing source metadata/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          exitCode: "0",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
        tsz: {
          status: "pass",
          exitCode: 0,
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsc exitCode must be an integer when present/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [
              { file: "assertions/one.ts" },
              { file: "./assertions/one.ts" },
            ],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate contains duplicate files: assertions\/one\.ts/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: { status: "pass", exitCode: "0" },
        tsz: { status: "pass", exitCode: 0 },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification tsc exitCode must be an integer when present/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "passing" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification tsc status must be one of pass, fail, timeout, error, unavailable: passing/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 1,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest(),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 },
        },
        tsz: {
          status: "passing",
          candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsz status must be one of pass, fail, timeout, error, unavailable: passing/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          diagnostics: {
            byCode: [
              { key: "TS2344", count: 1 },
              { key: "TS2344", count: 2 },
            ],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc\.diagnostics\.byCode contains duplicate codes: TS2344/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [
              {
                file: "assertions/one.ts",
                candidate: { id: "" },
              },
            ],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.candidate\.id must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [{ file: "assertions/one.ts", errorCount: -1 }],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.errorCount must be a non-negative integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [{ file: "assertions/one.ts", codes: [{ key: "" }] }],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.codes\[0\]\.key must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [{ file: "assertions/one.ts", semanticFamilies: [""] }],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.semanticFamilies\[0\] must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [
              {
                file: "assertions/one.ts",
                firstErrors: [{ line: "1", column: 1, code: "TS2344", message: "mismatch" }],
              },
            ],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.firstErrors\[0\]\.line must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            byCandidate: [
              {
                file: "assertions/one.ts",
                codes: [{ key: "", count: 1 }],
              },
            ],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.codes\[0\]\.key must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            byCandidate: [
              {
                file: "assertions/one.ts",
                codes: [{ key: "TS2344", count: -1 }],
              },
            ],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.codes\[0\]\.count must be a non-negative integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest(),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
      comparison: {},
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification comparison\.status must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          diagnostics: { firstErrors: [""] },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc\.diagnostics\.firstErrors\[0\] must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          diagnostics: { byCode: [{ key: "", count: 1 }] },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc\.diagnostics\.byCode\[0\]\.key must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          diagnostics: { byCode: [{ key: "TS2344", count: -1 }] },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc\.diagnostics\.byCode\[0\]\.count must be a non-negative integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        bySemanticFamilyDelta: [{ key: "", delta: 1 }],
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /comparison\.bySemanticFamilyDelta\[0\]\.key must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        bySemanticFamilyDelta: [{ key: "mapped/key-remapped types", delta: "1" }],
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /comparison\.bySemanticFamilyDelta\[0\]\.delta must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 2,
            candidatesWithoutDiagnostics: 0,
            filesWithDiagnostics: ["assertions/one.ts", "./assertions/one.ts"],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.filesWithDiagnostics contains duplicate files: assertions\/one\.ts/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            filesWithoutDiagnostics: ["./assertions/one.ts"],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics files overlap between diagnostic and diagnostic-free lists: assertions\/one\.ts/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            filesWithoutDiagnostics: null,
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.filesWithoutDiagnostics must be an array when candidatesWithoutDiagnostics is nonzero/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const row = runCompatibility({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "fail",
          exitCode: 1,
          diagnostics: {
            firstErrors: ["assertions/one.ts(1,1): error TS2344: mismatch"],
            byCode: [{ key: "TS2344", count: 1 }],
          },
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 0,
            filesWithDiagnostics: [".\\assertions\\one.ts"],
          },
        },
        tsz: {
          status: "pass",
          exitCode: 0,
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithDiagnostics: 0,
            candidatesWithoutDiagnostics: 1,
          },
        },
      },
      comparison: {
        status: "tsz-accepts-tsc-rejected",
        diagnosticFreeCandidateDelta: 1,
      },
    },
  });

  assert.equal(
    row.repro.first_failure_path,
    "type-challenges-assertions/assertions/one.ts",
  );
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
            byCandidate: [{ file: "../outside.ts" }],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.byCandidate\[0\]\.file must stay inside the assertion candidate directory/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 1,
            bothRejected: 1,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["assertions/one.ts"],
          bothRejected: ["./assertions/one.ts"],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison buckets overlap: assertions\/one\.ts \(bothAccepted, bothRejected\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const row = runCompatibility({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 1,
          counts: {
            bothAccepted: 1,
            bothRejected: 0,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: [".\\assertions\\one.ts"],
          bothRejected: [],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.deepEqual(row.assertion_candidates.file_comparison.both_accepted, [
    "type-challenges-assertions/assertions/one.ts",
  ]);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 2,
            bothRejected: 0,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["assertions/one.ts", "assertions/one.ts"],
          bothRejected: [],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison\.bothAccepted contains duplicate files: assertions\/one\.ts/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 1,
            bothRejected: 1,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["assertions/one.ts"],
          bothRejected: ["assertions/one.ts"],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison buckets overlap: assertions\/one\.ts \(bothAccepted, bothRejected\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: [],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.filesWithDiagnostics length \(0\) does not match candidatesWithDiagnostics \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["../outside.ts"],
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.filesWithDiagnostics\[0\] must stay inside the assertion candidate directory/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 1,
            filesWithDiagnostics: ["assertions/one.ts"],
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 0,
            candidatesWithoutDiagnostics: 2,
            filesWithDiagnostics: [],
          },
        },
      },
      comparison: {
        status: "both-pass",
        diagnosticFreeCandidateDelta: 2,
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /diagnosticFreeCandidateDelta \(2\) does not match tsz\/tsc diagnostic-free delta \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 1,
          counts: {
            bothAccepted: 1,
            bothRejected: 0,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["../outside.ts"],
          bothRejected: [],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison\.bothAccepted\[0\] must stay inside the assertion candidate directory/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 0,
            candidatesWithoutDiagnostics: 3,
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidateDiagnostics\.candidatesWithoutDiagnostics \(3\) must be between 0 and totalCandidates \(2\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 2,
            candidatesWithDiagnostics: 1,
            candidatesWithoutDiagnostics: 0,
          },
        },
        tsz: { status: "pass" },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc candidate diagnostic counts \(1 \+ 0\) do not match totalCandidates \(2\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 1,
          counts: {
            bothAccepted: 1,
            bothRejected: 0,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison\.totalCandidates \(1\) does not match generatedAssertions \(2\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 1,
            bothRejected: 1,
            tscAcceptedTszRejected: 1,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["assertions/one.ts"],
          bothRejected: ["assertions/two.ts"],
          tscAcceptedTszRejected: ["assertions/three.ts"],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison bucket counts \(3\) do not match totalCandidates \(2\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(2),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: {
        status: "both-pass",
        candidateFileComparison: {
          totalCandidates: 2,
          counts: {
            bothAccepted: 1,
            bothRejected: 1,
            tscAcceptedTszRejected: 0,
            tscRejectedTszAccepted: 0,
          },
          bothAccepted: ["assertions/one.ts", "assertions/two.ts"],
          bothRejected: ["assertions/three.ts"],
          tscAcceptedTszRejected: [],
          tscRejectedTszAccepted: [],
        },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateFileComparison\.bothAccepted length \(2\) does not match counts\.bothAccepted \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: {
        sources: candidateSources(),
        counts: {
          pairedSolutions: 1,
          generatedAssertions: 1,
        },
      },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /counts\.assertionsReferencingSolutionDeclaration must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: {
        sources: candidateSources(),
        counts: {
          pairedSolutions: 2,
          generatedAssertions: 2,
          assertionsReferencingSolutionDeclaration: 2,
          assertionsMissingSolutionDeclarationReference: 1,
        },
      },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /declaration-reference counts \(2 \+ 1\) do not match generatedAssertions \(2\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion manifest counts\.totalCandidates must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 1,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion manifest counts\.tscRejectedAssertions must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 3,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion manifest accepted\/rejected counts \(1 \+ 1\) do not match totalCandidates \(3\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: { totalCandidates: 1 },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsc candidateDiagnostics\.candidatesWithoutDiagnostics must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 2,
          },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsz candidatesWithoutDiagnostics \(2\) must be between 0 and totalCandidates \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
        tsz: { candidateDiagnostics: { totalCandidates: 1 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsz status must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /must include both tsc and tsz compiler results/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: {}, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification tsc status must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /assertion classification tsz status must be a non-empty string/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 1,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /counts\.tscAcceptedAssertionsReferencingSolutionDeclaration must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /candidateManifest is missing sources/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: {
        sources: candidateSources(),
        counts: { generatedAssertions: 1 },
      },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /counts\.pairedSolutions must be an integer/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: {
        sources: candidateSources(),
        counts: {
          pairedSolutions: 2,
          generatedAssertions: 1,
        },
      },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /pairedSolutions \(2\) does not match generatedAssertions \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "stale-clean-subset",
      counts: { tscAcceptedAssertions: 1 },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /unexpected tsc-clean assertion manifest fixture/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 1,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "stale-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /unexpected assertion classification fixture/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 1,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /tsc-clean assertion manifest is missing sources/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const manifestSources = candidateSources();
  manifestSources.solutions = {
    repository: "solutions",
    ref: "stale-solutions-ref",
  };
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: manifestSources,
      counts: {
        totalCandidates: 1,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /candidateManifest\.sources\.solutions .* does not match manifest sources\.solutions/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 3,
        rejected: 2,
      }),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classification counts\.totalCandidates \(3\) does not match manifest counts\.totalCandidates \(2\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 0,
      }),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classification counts\.tscRejectedAssertions \(0\) does not match manifest counts\.tscRejectedAssertions \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 2,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 1,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /counts\.tscAcceptedAssertions \(2\) does not match entries length \(1\)/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 2,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 2,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 1,
        tscRejectedAssertions: 0,
      },
      entries: [
        { output: "assertions/00001-easy-pick.ts" },
        { output: "assertions/00002-medium-return-type.ts" },
      ],
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /declaration-reference counts \(2 \+ 1\) do not match tscAcceptedAssertions \(2\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 3,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /accepted\/rejected counts \(1 \+ 1\) do not match totalCandidates \(3\)/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /manifest has 1 accepted assertions but classification is missing/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /tsc-clean assertion classification is missing candidateManifest/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest(),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification report is missing comparison/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        rejected: 1,
      }),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: {} },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 1, candidatesWithoutDiagnostics: 1 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /tsc-clean assertion classification tsc candidateDiagnostics\.totalCandidates must be an integer/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        tscAcceptedAssertions: 2,
        referencing: 2,
        missing: 0,
        rejected: 0,
      }),
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 2, candidatesWithoutDiagnostics: 2 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 2, candidatesWithoutDiagnostics: 2 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classification tscAcceptedAssertions \(2\) does not match manifest tscAcceptedAssertions \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "both-pass" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      sources: candidateSources(),
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 1,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: cleanCandidateManifest({
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        referencing: 1,
        missing: 0,
        rejected: 1,
      }),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: {
            totalCandidates: 1,
            candidatesWithoutDiagnostics: 1,
          },
        },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 2, candidatesWithoutDiagnostics: 2 } },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classification tsz totalCandidates \(2\) does not match manifest tscAcceptedAssertions \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(0),
      compilers: {
        tsc: {
          status: "unavailable",
          exitCode: null,
          diagnostics: { firstErrors: [], byCode: [] },
          candidateDiagnostics: {
            totalCandidates: 0,
            candidatesWithDiagnostics: null,
            candidatesWithoutDiagnostics: null,
            filesWithDiagnostics: [],
          },
        },
        tsz: {
          status: "unavailable",
          exitCode: null,
          candidateDiagnostics: {
            candidatesWithoutDiagnostics: null,
          },
        },
      },
      comparison: {
        status: "unavailable",
        diagnosticFreeCandidateDelta: null,
        bySemanticFamilyDelta: [],
      },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 0,
        tscAcceptedAssertions: 0,
        tscAcceptedAssertionsReferencingSolutionDeclaration: 0,
        tscAcceptedAssertionsMissingSolutionDeclarationReference: 0,
        tscRejectedAssertions: 0,
      },
      entries: [],
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /generatedAssertions must be greater than zero/);
  assert.equal(fs.existsSync(outFile), false);
});

withTempDir((dir) => {
  const { result, outFile } = runCompatibilityRaw({
    dir,
    classification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: candidateManifest(1),
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: { totalCandidates: 1 },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: { totalCandidates: 2 },
        },
      },
      comparison: { status: "both-pass" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classification tsz totalCandidates \(2\) does not match generatedAssertions \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});
