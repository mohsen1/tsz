#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  COMPILE_CANARY_PROJECT_ROWS,
  COMPATIBILITY_CORPUS_ROWS,
  REQUIRED_COMPATIBILITY_FIELDS,
  REQUIRED_PROJECT_ROWS,
} from "./project-rows.mjs";

function assertUnique(values, label) {
  const seen = new Set();
  for (const value of values) {
    assert.equal(typeof value, "string", `${label} entries must be strings`);
    assert.ok(value.length > 0, `${label} entries must not be empty`);
    assert.ok(!seen.has(value), `${label} has duplicate entry ${value}`);
    seen.add(value);
  }
}

assertUnique(REQUIRED_PROJECT_ROWS, "REQUIRED_PROJECT_ROWS");
assertUnique(COMPILE_CANARY_PROJECT_ROWS, "COMPILE_CANARY_PROJECT_ROWS");
assertUnique(REQUIRED_COMPATIBILITY_FIELDS, "REQUIRED_COMPATIBILITY_FIELDS");

const required = new Set(REQUIRED_PROJECT_ROWS);
const canary = new Set(COMPILE_CANARY_PROJECT_ROWS);
const tracked = new Set([...required, ...canary]);

for (const name of REQUIRED_PROJECT_ROWS) {
  assert.ok(
    !canary.has(name),
    `${name} must be required or compile-canary, not both`,
  );
}

const metadataNames = COMPATIBILITY_CORPUS_ROWS.map((row) => row?.name);
assertUnique(metadataNames, "COMPATIBILITY_CORPUS_ROWS names");

for (const row of COMPATIBILITY_CORPUS_ROWS) {
  assert.ok(tracked.has(row.name), `${row.name} is not a tracked project row`);
  for (const field of ["label", "owner", "family"]) {
    assert.equal(
      typeof row[field],
      "string",
      `${row.name}.${field} must be a string`,
    );
    assert.ok(row[field].length > 0, `${row.name}.${field} must not be empty`);
  }
}

for (const name of tracked) {
  assert.ok(
    metadataNames.includes(name),
    `${name} is missing COMPATIBILITY_CORPUS_ROWS metadata`,
  );
}

for (const field of [
  "state",
  "exit_class",
  "first_failure_class",
  "owner_track",
  "phase",
  "last_successful_phase",
  "diagnostic_status",
  "diagnostic_deltas",
  "diagnostic_subsystems",
  "known_blockers",
  "reduced_repro_path",
  "repro",
  "exit_codes",
  "files_reached",
  "peak_memory_bytes",
  "emit_status",
  "dts_status",
]) {
  assert.ok(
    REQUIRED_COMPATIBILITY_FIELDS.includes(field),
    `${field} must remain a required compatibility field`,
  );
}
