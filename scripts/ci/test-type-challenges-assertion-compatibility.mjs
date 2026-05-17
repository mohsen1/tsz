#!/usr/bin/env node
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

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

function candidateCounts(generatedAssertions = 1) {
  return {
    pairedSolutions: generatedAssertions,
    generatedAssertions,
  };
}

function runCompatibility({ dir, classification, cleanSubsetManifest = null, cleanSubsetClassification = null }) {
  const candidateDir = path.join(dir, "type-challenges-assertions");
  const classificationPath = path.join(candidateDir, "classification.json");
  const cleanSubsetDir = path.join(dir, "type-challenges-assertions-tsc-clean");
  const cleanSubsetManifestPath = path.join(cleanSubsetDir, "manifest.json");
  const cleanSubsetClassificationPath = path.join(cleanSubsetDir, "classification.json");
  const outFile = path.join(dir, "project-compatibility.jsonl");
  writeJson(classificationPath, classification);
  if (cleanSubsetManifest) {
    writeJson(cleanSubsetManifestPath, cleanSubsetManifest);
  }
  if (cleanSubsetClassification) {
    writeJson(cleanSubsetClassificationPath, cleanSubsetClassification);
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
  writeJson(classificationPath, classification);
  if (cleanSubsetManifest) {
    writeJson(cleanSubsetManifestPath, cleanSubsetManifest);
  }
  if (cleanSubsetClassification) {
    writeJson(cleanSubsetClassificationPath, cleanSubsetClassification);
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
                file: "assertions/one.ts",
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
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscRejectedAssertions: 1,
      },
      entries: [{ id: "two", output: "assertions/two.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      compilers: {
        tsc: {
          status: "pass",
          candidateDiagnostics: { candidatesWithoutDiagnostics: 1 },
        },
        tsz: {
          status: "pass",
          candidateDiagnostics: { candidatesWithoutDiagnostics: 1 },
        },
      },
    },
  });

  assert.equal(row.name, "type-challenges-assertion-candidates");
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
      generated_assertions: 1,
      rejected_from_full_corpus: 1,
      tsc_status: "pass",
      tsz_status: "pass",
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
      candidateManifest: { counts: candidateCounts(1) },
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
      candidateManifest: { counts: candidateCounts(1) },
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
        status: "match",
        diagnosticFreeCandidateDelta: 0,
        bySemanticFamilyDelta: [],
      },
    },
  });

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
      candidateManifest: { counts: candidateCounts(1) },
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
      candidateManifest: { counts: candidateCounts(1) },
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
      candidateManifest: { counts: { generatedAssertions: 1 } },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
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
        counts: {
          pairedSolutions: 2,
          generatedAssertions: 1,
        },
      },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
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
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
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
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 1,
        tscAcceptedAssertions: 1,
        tscRejectedAssertions: 0,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "stale-classification",
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
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
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 2,
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
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 3,
        tscAcceptedAssertions: 1,
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
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
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
      candidateManifest: { counts: candidateCounts(1) },
      compilers: { tsc: { status: "pass" }, tsz: { status: "pass" } },
      comparison: { status: "match" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: { counts: { generatedAssertions: 2 } },
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 2 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 2 } },
      },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classification generatedAssertions \(2\) does not match manifest tscAcceptedAssertions \(1\)/,
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
      comparison: { status: "match" },
    },
    cleanSubsetManifest: {
      fixture: "type-challenges-assertions-tsc-clean",
      counts: {
        totalCandidates: 2,
        tscAcceptedAssertions: 1,
        tscRejectedAssertions: 1,
      },
      entries: [{ output: "assertions/00001-easy-pick.ts" }],
    },
    cleanSubsetClassification: {
      fixture: "type-challenges-assertion-classification",
      candidateManifest: { counts: candidateCounts(1) },
      compilers: {
        tsc: { status: "pass", candidateDiagnostics: { totalCandidates: 1 } },
        tsz: { status: "pass", candidateDiagnostics: { totalCandidates: 2 } },
      },
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
      candidateManifest: { counts: candidateCounts(0) },
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
        tscRejectedAssertions: null,
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
      candidateManifest: { counts: candidateCounts(1) },
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
      comparison: { status: "match" },
    },
  });

  assert.equal(result.status, 1);
  assert.match(
    result.stderr,
    /classification tsz totalCandidates \(2\) does not match generatedAssertions \(1\)/,
  );
  assert.equal(fs.existsSync(outFile), false);
});
