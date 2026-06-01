#!/usr/bin/env node
// Diagnostic-subsystem classification table.
//
// The rules table is the single source of truth for mapping TypeScript
// diagnostic codes to owning subsystems, owner tracks, owning crates, and
// labels. It lives in `diagnostic-subsystems.json` so JS modules (this file
// + the `node <<'NODE'` heredoc in `scripts/bench/bench-vs-tsgo.sh`) read
// the same table and cannot drift.

import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TABLE = JSON.parse(fs.readFileSync(path.join(HERE, "diagnostic-subsystems.json"), "utf8"));

export const DIAGNOSTIC_SUBSYSTEM_RULES = TABLE.rules.map(
  (rule) => [rule.subsystem, new Set(rule.codes)],
);

const CODE_TO_SUBSYSTEM = new Map();
for (const rule of TABLE.rules) {
  for (const code of rule.codes) CODE_TO_SUBSYSTEM.set(code, rule.subsystem);
}

export const OWNER_TRACK_BY_SUBSYSTEM = new Map(
  TABLE.rules.map((rule) => [rule.subsystem, rule.owner_track]),
);

export const CRATE_BY_SUBSYSTEM = new Map([
  ...TABLE.rules.map((rule) => [rule.subsystem, rule.crate]),
  ...Object.entries(TABLE.exit_class_crates ?? {}),
]);

export const LABELS_BY_SUBSYSTEM = new Map(
  TABLE.rules.map((rule) => [rule.subsystem, rule.labels]),
);

export function subsystemForCode(code) {
  return CODE_TO_SUBSYSTEM.get(code) ?? "unclassified diagnostic";
}

export function ownerTrackForSubsystem(subsystem) {
  if (subsystem === "uncoded diagnostic" || subsystem === "unclassified diagnostic") {
    return "Track 1 triage";
  }
  if (subsystem && subsystem.startsWith("type-challenges ")) {
    return subsystem.includes("indexed access")
      ? "Track 5 Type Challenges keyspace/indexed access"
      : "Track 2/3 Type Challenges type-level semantics";
  }
  return OWNER_TRACK_BY_SUBSYSTEM.get(subsystem) ?? "Track 1 triage";
}
