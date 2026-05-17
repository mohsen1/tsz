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

function runCompatibility({ dir, classification }) {
  const candidateDir = path.join(dir, "type-challenges-assertions");
  const classificationPath = path.join(candidateDir, "classification.json");
  const outFile = path.join(dir, "project-compatibility.jsonl");
  writeJson(classificationPath, classification);

  const result = spawnSync(
    process.execPath,
    [SCRIPT, classificationPath, candidateDir, outFile, dir],
    {
      cwd: ROOT,
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return readRows(outFile)[0];
}

withTempDir((dir) => {
  const row = runCompatibility({
    dir,
    classification: {
      candidateManifest: {
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
        bySemanticFamilyDelta: [{ key: "mapped/key-remapped types", delta: -1 }],
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
    paired_solutions: 2,
    generated_assertions: 2,
    tsc_diagnostic_free: 1,
    tsc_with_diagnostics: 1,
    tsz_diagnostic_free: 2,
    diagnostic_free_candidate_delta: 1,
  });
});

withTempDir((dir) => {
  const row = runCompatibility({
    dir,
    classification: {
      candidateManifest: { counts: { generatedAssertions: 1 } },
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
      candidateManifest: { counts: { generatedAssertions: 1 } },
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
