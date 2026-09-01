#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import {
  normalizePath,
  semanticFamiliesForFile,
} from "./type-challenges-semantic-families.mjs";
import { ownerTrackForSubsystem } from "./diagnostic-subsystems.mjs";
import {
  aggregateRowDeltas,
  aggregateRowsForSummary,
  parseDiagnosticLine,
} from "./diagnostic-aggregator.mjs";
import { fixtureStubEvidenceFingerprint } from "../bench/lib/fixture-stub-inventory.mjs";

const TYPE_CHALLENGES_PROJECT_ROWS = new Set([
  "type-challenges-solutions-project",
]);

const ORACLE_CLASSIFICATION_ORDER = [
  "both-pass",
  "tsc-fails-only",
  "tsz-fails-only",
  "both-fail-same",
  "both-fail-different",
  "unknown",
];
const ORACLE_CLASSIFICATIONS = new Set(ORACLE_CLASSIFICATION_ORDER);

const ROW_STATE_DISPLAY_ORDER = ["green", "yellow", "red", "gray"];

// Closed vocabulary for the structured reason a residency field is absent.
// Null is reserved for "measurement present"; every other value must come
// from these sets so dashboards can group residency gaps deterministically.
const FILES_REACHED_REASONS = new Set([
  "runner did not count",
  "not in scope",
  "process exited before counting",
  "compiler stats missing",
  "compiler stats malformed",
  "zero source files processed",
  "fixture dependency stubs present",
  "fixture stub inventory unavailable",
]);
const PEAK_MEMORY_BYTES_REASONS = new Set([
  "not measured on platform",
  "measurement disabled",
  "process exited before sampling",
  "not in scope",
]);
const DEFAULT_FILES_REACHED_REASON = "runner did not count";
const DEFAULT_PEAK_MEMORY_BYTES_REASON = "not measured on platform";
if (!FILES_REACHED_REASONS.has(DEFAULT_FILES_REACHED_REASON)) {
  throw new Error("DEFAULT_FILES_REACHED_REASON must be in FILES_REACHED_REASONS");
}
if (!PEAK_MEMORY_BYTES_REASONS.has(DEFAULT_PEAK_MEMORY_BYTES_REASON)) {
  throw new Error("DEFAULT_PEAK_MEMORY_BYTES_REASON must be in PEAK_MEMORY_BYTES_REASONS");
}

function toNumber(value) {
  if (value === undefined || value === null || value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function toNonnegativeInteger(value) {
  const parsed = toNumber(value);
  return Number.isInteger(parsed) && parsed >= 0 ? parsed : null;
}

function toSha256(value) {
  const fingerprint = String(value || "").trim().toLowerCase();
  return /^[0-9a-f]{64}$/.test(fingerprint) ? fingerprint : null;
}

function toGitCommit(value) {
  const commit = String(value || "").trim().toLowerCase();
  return /^[0-9a-f]{40}$/.test(commit) ? commit : null;
}

function toBoolean(value) {
  if (value === true || value === "true") return true;
  if (value === false || value === "false") return false;
  return null;
}

function toStringArray(value) {
  if (Array.isArray(value) && value.every((item) => typeof item === "string")) {
    return [...value].sort();
  }
  try {
    const parsed = JSON.parse(String(value || ""));
    return Array.isArray(parsed) && parsed.every((item) => typeof item === "string")
      ? [...parsed].sort()
      : null;
  } catch {
    return null;
  }
}

function residencyReason(value, rawReason, vocabulary, fallback, fieldName) {
  if (value !== null) return null;
  const reason = String(rawReason || "").trim();
  if (!reason) return fallback;
  if (vocabulary.has(reason)) return reason;
  console.error(
    `warning: ${fieldName} reason ${JSON.stringify(reason)} is not in the accepted vocabulary; ` +
    `falling back to ${JSON.stringify(fallback)}. Accepted: ${[...vocabulary].sort().join(", ")}`,
  );
  return fallback;
}

function toExitCodes(value) {
  const matches = String(value || "").match(/\b\d+\b/g) || [];
  return matches.map(Number).filter(Number.isInteger);
}

function splitDeltaLines(value) {
  return String(value || "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

// When per-source COMPAT_*_DIAGNOSTIC_DELTA env overrides are set they win
// over the unified-delta partition; otherwise the per-source bodies come
// directly from the unified-delta aggregator so the unified list is walked
// exactly once for both subsystem grouping AND per-source code extraction.
// `explicit` is taken from the caller so the env triplet is read in exactly
// one place per row.
function perSourceDeltasFrom(explicit, aggregate) {
  const bodiesBySource = aggregate.bodiesBySource;
  return {
    tsc: explicit.tsc !== undefined ? splitDeltaLines(explicit.tsc) : bodiesBySource.tsc,
    tsz: explicit.tsz !== undefined
      ? splitDeltaLines(explicit.tsz)
      : [...bodiesBySource.tsz, ...bodiesBySource.unattributed],
    tsgo: explicit.tsgo !== undefined ? splitDeltaLines(explicit.tsgo) : bodiesBySource.tsgo,
  };
}

function codesFromLines(lines, limit) {
  const codes = [];
  const seen = new Set();
  for (const line of lines) {
    for (const match of String(line || "").matchAll(/\bTS\d{4,5}\b/g)) {
      const code = match[0];
      if (seen.has(code)) continue;
      seen.add(code);
      codes.push(code);
      if (codes.length >= limit) return codes;
    }
  }
  return codes;
}

// Single-sided failures classify as *-fails-only so dashboards can route
// oracle-side failures away from tsz-divergence triage.
function oracleClassificationFrom({ tscExitCodes, tszExitCodes, tscDiagnosticCodes, tszDiagnosticCodes }) {
  // A side counts as "failed" when it emitted any diagnostic codes OR exited
  // nonzero. Diagnostic codes are the authoritative per-compiler signal: the
  // parity path in scripts/ci/project-compile-guard.sh normalizes tsz's exit
  // code to 0 once the tsc oracle cancels the tsz-only delta, yet tsz DID
  // report the same diagnostics tsc did. Keying only on the exit code there
  // would misread that side as "passed" and mislabel a tsz==tsc parity row as
  // tsc-fails-only instead of both-fail-same. Empty codes + nonzero exit
  // (crash/timeout/oom) still classifies as failed via the exit-code branch.
  const failed = (exitCodes, diagnosticCodes) => (
    diagnosticCodes.length > 0 || exitCodes.some((code) => code !== 0)
  );
  const tscSignaled = tscExitCodes.length > 0 || tscDiagnosticCodes.length > 0;
  const tszSignaled = tszExitCodes.length > 0 || tszDiagnosticCodes.length > 0;
  const tscFailed = failed(tscExitCodes, tscDiagnosticCodes);
  const tszFailed = failed(tszExitCodes, tszDiagnosticCodes);

  if (!tscSignaled && !tszSignaled) return "unknown";
  if (!tszSignaled) return tscFailed ? "tsc-fails-only" : "unknown";
  if (!tscSignaled) return "unknown";

  if (!tscFailed && !tszFailed) return "both-pass";
  if (tscFailed && !tszFailed) return "tsc-fails-only";
  if (!tscFailed && tszFailed) return "tsz-fails-only";

  const tscSet = new Set(tscDiagnosticCodes);
  const tszSet = new Set(tszDiagnosticCodes);
  // Empty=empty counts as "same" so two failures with only exit-code signals
  // classify together only when their ordinary exit status also agrees.
  if (
    tscSet.size === tszSet.size &&
    [...tscSet].every((code) => tszSet.has(code)) &&
    tscExitCodes.length === tszExitCodes.length &&
    tscExitCodes.every((code, index) => code === tszExitCodes[index])
  ) {
    return "both-fail-same";
  }
  return "both-fail-different";
}

function firstNonEmpty(...values) {
  for (const value of values) {
    const normalized = String(value ?? "").trim();
    if (normalized) return normalized;
  }
  return null;
}

function githubRunUrl(env, runId) {
  if (!runId || runId === "local") return null;
  const serverUrl = firstNonEmpty(env.GITHUB_SERVER_URL, "https://github.com");
  const repository = firstNonEmpty(env.GITHUB_REPOSITORY);
  if (!repository) return null;
  return `${serverUrl}/${repository}/actions/runs/${runId}`;
}

function artifactMetadata(env, prefix, generatedAt) {
  const runId = firstNonEmpty(env[`${prefix}_WORKFLOW_RUN_ID`], env.GITHUB_RUN_ID, "local");
  const runStatus = firstNonEmpty(
    env[`${prefix}_RUN_STATUS`],
    env.GITHUB_ACTIONS === "true" ? "completed" : "local",
  );
  return {
    generated_at: firstNonEmpty(env[`${prefix}_GENERATED_AT`], generatedAt),
    source_commit: firstNonEmpty(env[`${prefix}_SOURCE_COMMIT`], env.BENCH_TARGET_SHA, env.GITHUB_SHA, "local"),
    workflow_name: firstNonEmpty(env[`${prefix}_WORKFLOW_NAME`], env.GITHUB_WORKFLOW, "local"),
    workflow_run_id: runId,
    workflow_run_url: firstNonEmpty(
      env[`${prefix}_WORKFLOW_RUN_URL`],
      githubRunUrl(env, runId),
    ),
    workflow_run_attempt: firstNonEmpty(env[`${prefix}_WORKFLOW_RUN_ATTEMPT`], env.GITHUB_RUN_ATTEMPT),
    run_status: runStatus,
  };
}

function isProjectRowName(value) {
  return typeof value === "string" && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}

function fixtureSourcesFrom(value) {
  const sources = [];
  const seen = new Set();
  const lines = String(value || "").split(/\r?\n/);
  for (const [index, rawLine] of lines.entries()) {
    const line = rawLine.trim();
    if (!line) continue;
    const parts = line.split("|").map((part) => part.trim());
    if (parts.length !== 3 || parts.some((part) => part === "")) {
      throw new Error(
        `COMPAT_FIXTURE_SOURCES line ${index + 1} must be name|repository|ref: ${line}`,
      );
    }
    const [name, repository, ref] = parts;
    const key = `${name}\0${repository}\0${ref}`;
    if (seen.has(key)) continue;
    seen.add(key);
    sources.push({
      name,
      repository,
      ref,
    });
  }
  return sources;
}

function sourceRootsForTypeChallenges() {
  const roots = [];
  const add = (value) => {
    if (!value) return;
    const resolved = path.resolve(value);
    if (!roots.includes(resolved)) roots.push(resolved);
  };
  add(process.env.COMPAT_SOURCE_ROOT);
  add(process.env.COMPAT_FIXTURE_ROOT);
  return roots;
}

function typeChallengesFamiliesForFile(file, sourceRoots, sourceCache) {
  if (!file) return ["unknown"];
  const normalized = normalizePath(file).replace(/^\.\//, "");

  for (const root of sourceRoots) {
    const families = semanticFamiliesForFile(normalized, root, sourceCache);
    if (!(families.length === 1 && families[0] === "unknown")) {
      return families;
    }
  }

  if (path.isAbsolute(file)) {
    for (const root of sourceRoots) {
      const families = semanticFamiliesForFile(file, root, sourceCache);
      if (!(families.length === 1 && families[0] === "unknown")) {
        return families;
      }
    }
  }

  return ["unknown"];
}

// Plug-in subsystem classifier for the type-challenges project row. Returns
// the family labels for the line if any match, or null to fall back to the
// default code -> subsystem table. Returning `[]` would suppress the line
// for grouping; we instead fall back so the unified aggregator's
// subsystem-or-uncoded bucket is still populated for non-classifiable lines.
function typeChallengesSubsystemHook() {
  const sourceRoots = sourceRootsForTypeChallenges();
  const sourceCache = new Map();
  return (_line, parsedLocation) => {
    const filePath = parsedLocation?.path ?? null;
    const families = typeChallengesFamiliesForFile(filePath, sourceRoots, sourceCache);
    if (families.length === 1 && families[0] === "unknown") return null;
    return families.map((family) => `type-challenges ${family}`);
  };
}

function knownBlockersFrom({ exitClass, phase, diagnosticSubsystems, diagnosticCodes }) {
  const blockers = [];
  const add = (blocker) => {
    if (blocker && !blockers.includes(blocker) && blockers.length < 8) blockers.push(blocker);
  };

  if (exitClass === "timeout") add("timeout during project check");
  if (exitClass === "oom") add("OOM or killed during project check");
  if (exitClass === "crash") add("compiler crash during project check");
  if (exitClass === "slowdown") add("runtime slowdown during project timing");
  if (exitClass === "fixture invalid") add("reference fixture invalid");
  if (exitClass === "runner error") add("benchmark runner error");
  if (exitClass === "tsz unavailable") add("tsz unavailable in benchmark runner");
  if (exitClass === "oracle unavailable") add("tsc oracle unavailable");
  if (phase && phase !== "check") add(`${phase} phase blocker`);

  for (const group of diagnosticSubsystems) {
    add(group.subsystem);
  }

  if (!blockers.length && diagnosticCodes.length) {
    add("unclassified diagnostic mismatch");
  }

  return blockers;
}

function lastSuccessfulPhaseFrom({ exitClass, diagnosticStatus }) {
  if (exitClass === "exit success" && diagnosticStatus === "none") return "check";
  return null;
}

function rowStateFrom({ exitClass, diagnosticStatus }) {
  if (exitClass === "exit success" && diagnosticStatus === "none") return "green";
  if (
    exitClass === "fixture invalid" ||
    exitClass === "runner error" ||
    exitClass === "tsz unavailable" ||
    exitClass === "oracle unavailable"
  ) return "gray";
  if (String(diagnosticStatus || "").toLowerCase().includes("diagnostic mismatch")) {
    return "yellow";
  }
  if (
    exitClass === "nonzero exit" ||
    exitClass === "timeout" ||
    exitClass === "oom" ||
    exitClass === "crash" ||
    exitClass === "slowdown"
  ) {
    return "red";
  }
  return "yellow";
}

function exactOrdinaryExit(codes) {
  if (!Array.isArray(codes) || codes.length !== 1) return null;
  const value = Number(codes[0]);
  return Number.isInteger(value) && value >= 0 && value <= 4 ? value : null;
}

function evidenceV3Failures(row) {
  const failures = [];
  const requireSha = (field) => {
    if (!toSha256(row[field])) failures.push(field);
  };
  if (!toGitCommit(row.source_commit)) failures.push("source_commit");
  if (typeof row.source_dirty !== "boolean") failures.push("source_dirty");
  if (row.source_stable !== true) failures.push("source_stable");
  if (row.compile_input_stable !== true) failures.push("compile_input_stable");
  for (const field of [
    "source_tree_fingerprint",
    "evidence_protocol_fingerprint",
    "tsz_binary_sha256",
    "build_manifest_sha256",
    "build_inputs_sha256",
    "build_manifest_binary_sha256",
    "compile_input_fingerprint",
    "oracle_fingerprint",
    "root_file_fingerprint",
    "source_file_fingerprint",
    "oracle_root_file_fingerprint",
    "oracle_source_file_fingerprint",
    "diagnostic_fingerprint",
    "oracle_diagnostic_fingerprint",
    "stub_inventory_fingerprint",
  ]) requireSha(field);
  if (row.build_manifest_binary_sha256 !== row.tsz_binary_sha256) {
    failures.push("build_manifest_binary");
  }
  if (row.semantic_completion !== "complete") failures.push("semantic_completion");
  if (!Number.isInteger(row.root_files) || row.root_files <= 0
    || !Number.isInteger(row.source_files) || row.source_files <= 0
    || row.root_files !== row.oracle_root_files
    || row.source_files !== row.oracle_source_files
    || row.files_reached !== row.source_files
    || row.root_file_fingerprint !== row.oracle_root_file_fingerprint
    || row.source_file_fingerprint !== row.oracle_source_file_fingerprint) {
    failures.push("project_graph");
  }
  const tszExit = exactOrdinaryExit(row.exit_codes?.tsz);
  const tscExit = exactOrdinaryExit(row.exit_codes?.tsc);
  if (tszExit === null || tscExit === null || tszExit !== tscExit) failures.push("compiler_exits");
  if (row.oracle_classification !== "both-pass" && row.oracle_classification !== "both-fail-same") {
    failures.push("oracle_classification");
  }
  if (!Number.isInteger(row.diagnostic_records) || row.diagnostic_records < 0
    || row.diagnostic_records !== row.oracle_diagnostic_records
    || row.diagnostic_fingerprint !== row.oracle_diagnostic_fingerprint) {
    failures.push("diagnostic_records");
  }
  const owners = toStringArray(row.stub_inventory_owners);
  if (row.stub_inventory_schema !== 2
    || !Number.isInteger(row.stubbed_modules) || row.stubbed_modules !== 0
    || !Number.isInteger(row.stubbed_any_members) || row.stubbed_any_members !== 0
    || !owners
    || owners.length !== 0
    || row.stub_inventory_fingerprint !== fixtureStubEvidenceFingerprint(0, 0, owners || [])) {
    failures.push("fixture_stub_inventory");
  }
  if (row.exit_class !== "exit success" || row.diagnostic_status !== "none") {
    failures.push("semantic_verdict");
  }
  return [...new Set(failures)];
}

function applyEvidenceState(row, requestedSchema) {
  const failures = evidenceV3Failures(row);
  const exact = requestedSchema === 3 && failures.length === 0;
  row.evidence_schema = exact ? 3 : null;
  row.evidence_status = exact ? "exact" : "incomplete";
  row.evidence_failures = exact
    ? []
    : requestedSchema === 3 ? failures : ["evidence_schema"];
  if (row.state === "green" && !exact) {
    row.state = "gray";
    row.first_failure_class = "compatibility evidence incomplete";
    row.owner_track = "Track 1 project-corpus harness/config";
    row.known_blockers = [
      `compatibility evidence incomplete: ${row.evidence_failures.join(", ")}`,
      ...(Array.isArray(row.known_blockers) ? row.known_blockers : []),
    ].slice(0, 8);
  }
}

function ownerTrackFrom({ exitClass, diagnosticSubsystems }) {
  if (exitClass === "timeout") return "Track 1 runtime/timeout triage";
  if (exitClass === "oom") return "Track 1 residency triage";
  if (exitClass === "crash") return "Track 1 crash triage";
  if (exitClass === "slowdown") return "Track 10 runtime slowdown triage";
  if (exitClass === "fixture invalid") return "Track 1 project-corpus harness/config";
  if (exitClass === "runner error") return "Track 1 benchmark runner";
  if (exitClass === "tsz unavailable") return "Track 1 benchmark runner";
  if (exitClass === "oracle unavailable") return "Track 1 tsc oracle evidence";

  const primary = diagnosticSubsystems[0]?.subsystem;
  return ownerTrackForSubsystem(primary) || "Track 1 triage";
}

function relativeToFixture(value) {
  if (!value) return null;
  const fixtureRoot = process.env.COMPAT_FIXTURE_ROOT || "";
  if (!fixtureRoot || !path.isAbsolute(value)) return value;

  const relative = path.relative(fixtureRoot, value);
  if (relative && !relative.startsWith("..") && !path.isAbsolute(relative)) {
    return relative;
  }
  return value;
}

// Builds the repro payload from the aggregator's first-failure location.
// The aggregator captures the location during its single walk over the delta
// list, so the delta list is not re-scanned here. `relativeToFixture` is
// applied to the path at the boundary (raw paths flow into the aggregator).
function reproFrom(rawLocation) {
  const relativePath = rawLocation ? relativeToFixture(rawLocation.path) : null;
  const tsconfigPath = relativeToFixture(process.env.COMPAT_TSCONFIG_PATH || "");
  const sourceRoot = relativeToFixture(process.env.COMPAT_SOURCE_ROOT || "");
  const reducedReproPath = relativePath || sourceRoot || tsconfigPath || null;
  const tszCommandEnvPrefix = String(process.env.COMPAT_TSZ_COMMAND_ENV_PREFIX || "").trim();
  const commandPrefix = tszCommandEnvPrefix ? `${tszCommandEnvPrefix} ` : "";

  return {
    tsconfig_path: tsconfigPath,
    source_root: sourceRoot,
    first_failure_path: relativePath || null,
    first_failure_line: rawLocation?.line ?? null,
    first_failure_column: rawLocation?.column ?? null,
    first_failure_code: rawLocation?.code || null,
    reduced_repro_path: reducedReproPath,
    command: tsconfigPath ? `${commandPrefix}$TSZ_BIN --noEmit -p ${tsconfigPath}` : null,
  };
}

function readRows(input) {
  const result = { rows: [], malformedLineCount: 0, malformedExamples: [] };
  try {
    const lines = fs.readFileSync(input, "utf8")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    for (const [index, line] of lines.entries()) {
      try {
        result.rows.push(JSON.parse(line));
      } catch (error) {
        result.malformedLineCount += 1;
        if (result.malformedExamples.length < 3) {
          result.malformedExamples.push({
            line: index + 1,
            error: error instanceof Error ? error.message : String(error),
            text: line.slice(0, 240),
          });
        }
      }
    }
  } catch {
    return result;
  }
  return result;
}

function readOptionalJson(file) {
  if (!file || !fs.existsSync(file)) return null;
  if (!fs.statSync(file).isFile()) return null;
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function isInside(root, file) {
  const relative = path.relative(root, file);
  return relative === "" || (!!relative && !relative.startsWith("..") && !path.isAbsolute(relative));
}

function resolveWritableFile({ value, label, root, forbidden = [] }) {
  if (!value) {
    throw new Error(`${label} is required`);
  }

  const resolved = path.resolve(value);
  if (root) {
    const resolvedRoot = path.resolve(root);
    if (!isInside(resolvedRoot, resolved)) {
      throw new Error(`${label} must stay inside output root`);
    }
  }

  for (const blocked of forbidden) {
    if (blocked && path.resolve(blocked) === resolved) {
      throw new Error(`${label} must not overwrite an input artifact`);
    }
  }

  const parent = path.dirname(resolved);
  if (!fs.existsSync(parent) || !fs.statSync(parent).isDirectory()) {
    throw new Error(`${label} parent directory does not exist`);
  }
  if (fs.existsSync(resolved) && !fs.statSync(resolved).isFile()) {
    throw new Error(`${label} path is not a file`);
  }

  return resolved;
}

function record() {
  const generatedAt = new Date().toISOString();
  const delta = process.env.COMPAT_DIAGNOSTIC_DELTA || "";
  const diagnosticDeltas = splitDeltaLines(delta).slice(0, 20);

  const projectName = process.env.COMPAT_NAME || "";
  if (!isProjectRowName(projectName)) {
    console.error("error: COMPAT_NAME must be a lowercase hyphenated project row slug");
    process.exit(1);
  }

  // One linear walk over the canonical (capped) delta list populates every
  // bucket downstream: subsystem groups, deduped codes, per-source bodies,
  // and the first-failure location. Type-challenges family classification
  // piggybacks on the same walk via the subsystem hook so the delta list
  // is never re-iterated for that project family either.
  const aggregate = aggregateRowDeltas(diagnosticDeltas, {
    subsystemFor: TYPE_CHALLENGES_PROJECT_ROWS.has(projectName)
      ? typeChallengesSubsystemHook()
      : undefined,
  });
  const diagnosticSubsystems = aggregate.subsystems;
  const diagnosticCodes = aggregate.codes;

  const tscExitCodes = toExitCodes(process.env.COMPAT_TSC_EXIT_CODES);
  const tszExitCodes = toExitCodes(process.env.COMPAT_TSZ_EXIT_CODES);
  const tsgoExitCodes = toExitCodes(process.env.COMPAT_TSGO_EXIT_CODES);

  // env reads, per-source body partition, and per-source code lists are all
  // derived from a single shared `explicit` object so each env var is read
  // exactly once even though it gates multiple downstream decisions.
  const explicit = {
    tsc: process.env.COMPAT_TSC_DIAGNOSTIC_DELTA,
    tsz: process.env.COMPAT_TSZ_DIAGNOSTIC_DELTA,
    tsgo: process.env.COMPAT_TSGO_DIAGNOSTIC_DELTA,
  };
  const perSourceDeltas = perSourceDeltasFrom(explicit, aggregate);
  const tscDiagnosticCodes = explicit.tsc !== undefined
    ? codesFromLines(perSourceDeltas.tsc, 8)
    : aggregate.codesBySource.tsc;
  // Unattributed bodies fold into the tsz bucket (the active failing side
  // in the common project-row failure path). Both source lists are already
  // deduped and capped at CODE_LIMIT by the aggregator, so the merge is
  // bounded and small enough to express inline.
  const tszDiagnosticCodes = explicit.tsz !== undefined
    ? codesFromLines(perSourceDeltas.tsz, 8)
    : [...new Set([...aggregate.codesBySource.tsz, ...aggregate.codesBySource.unattributed])].slice(0, 8);
  const tsgoDiagnosticCodes = explicit.tsgo !== undefined
    ? codesFromLines(perSourceDeltas.tsgo, 8)
    : aggregate.codesBySource.tsgo;

  const oracleClassification = oracleClassificationFrom({
    tscExitCodes,
    tszExitCodes,
    tscDiagnosticCodes,
    tszDiagnosticCodes,
  });
  if (!ORACLE_CLASSIFICATIONS.has(oracleClassification)) {
    console.error(`error: computed oracle_classification "${oracleClassification}" is not in the accepted set`);
    process.exit(1);
  }

  const exitClass = process.env.COMPAT_EXIT_CLASS || "unknown";
  const diagnosticStatus = process.env.COMPAT_DIAGNOSTIC_STATUS || "unknown";
  const state = rowStateFrom({ exitClass, diagnosticStatus });
  const repro = reproFrom(aggregate.firstLocation);
  const knownBlockers = knownBlockersFrom({
    exitClass,
    phase: process.env.COMPAT_PHASE || "unknown",
    diagnosticSubsystems,
    diagnosticCodes,
  });
  let outputFile;
  let fixtureSources;
  try {
    outputFile = resolveWritableFile({
      value: process.env.COMPAT_JSONL_FILE,
      label: "project compatibility JSONL",
      root: process.env.COMPAT_OUTPUT_ROOT,
    });
    fixtureSources = fixtureSourcesFrom(process.env.COMPAT_FIXTURE_SOURCES);
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }

  const filesReached = toNumber(process.env.COMPAT_FILES_REACHED);
  const peakMemoryBytes = toNumber(process.env.COMPAT_PEAK_MEMORY_BYTES);
  const evidenceSchema = toNonnegativeInteger(process.env.COMPAT_EVIDENCE_SCHEMA);
  const stubInventorySchema = toNonnegativeInteger(process.env.COMPAT_STUB_INVENTORY_SCHEMA);
  const filesReachedReason = residencyReason(
    filesReached,
    process.env.COMPAT_FILES_REACHED_REASON,
    FILES_REACHED_REASONS,
    DEFAULT_FILES_REACHED_REASON,
    "files_reached",
  );
  const peakMemoryBytesReason = residencyReason(
    peakMemoryBytes,
    process.env.COMPAT_PEAK_MEMORY_BYTES_REASON,
    PEAK_MEMORY_BYTES_REASONS,
    DEFAULT_PEAK_MEMORY_BYTES_REASON,
    "peak_memory_bytes",
  );

  const row = {
    ...artifactMetadata(process.env, "COMPAT", generatedAt),
    name: projectName,
    state,
    exit_class: exitClass,
    first_failure_class: state === "green" ? null : knownBlockers[0] || exitClass,
    owner_track: state === "green" ? null : ownerTrackFrom({ exitClass, diagnosticSubsystems }),
    phase: process.env.COMPAT_PHASE || "unknown",
    last_successful_phase: lastSuccessfulPhaseFrom({ exitClass, diagnosticStatus }),
    diagnostic_status: diagnosticStatus,
    evidence_schema: null,
    evidence_status: null,
    evidence_failures: [],
    source_dirty: toBoolean(process.env.COMPAT_SOURCE_DIRTY),
    source_stable: toBoolean(process.env.COMPAT_SOURCE_STABLE),
    source_tree_fingerprint: toSha256(process.env.COMPAT_SOURCE_TREE_FINGERPRINT),
    evidence_protocol_fingerprint: toSha256(process.env.COMPAT_EVIDENCE_PROTOCOL_FINGERPRINT),
    tsz_binary_sha256: toSha256(process.env.COMPAT_TSZ_BINARY_SHA256),
    build_manifest_sha256: toSha256(process.env.COMPAT_BUILD_MANIFEST_SHA256),
    build_inputs_sha256: toSha256(process.env.COMPAT_BUILD_INPUTS_SHA256),
    build_manifest_binary_sha256: toSha256(process.env.COMPAT_BUILD_MANIFEST_BINARY_SHA256),
    compile_input_fingerprint: toSha256(process.env.COMPAT_COMPILE_INPUT_FINGERPRINT),
    compile_input_stable: toBoolean(process.env.COMPAT_COMPILE_INPUT_STABLE),
    oracle_fingerprint: toSha256(process.env.COMPAT_ORACLE_FINGERPRINT),
    semantic_completion: String(process.env.COMPAT_SEMANTIC_COMPLETION || "").trim() || null,
    root_files: toNonnegativeInteger(process.env.COMPAT_ROOT_FILES),
    source_files: toNonnegativeInteger(process.env.COMPAT_SOURCE_FILES),
    root_file_fingerprint: toSha256(process.env.COMPAT_ROOT_FILE_FINGERPRINT),
    source_file_fingerprint: toSha256(process.env.COMPAT_SOURCE_FILE_FINGERPRINT),
    oracle_root_files: toNonnegativeInteger(process.env.COMPAT_ORACLE_ROOT_FILES),
    oracle_source_files: toNonnegativeInteger(process.env.COMPAT_ORACLE_SOURCE_FILES),
    oracle_root_file_fingerprint: toSha256(process.env.COMPAT_ORACLE_ROOT_FILE_FINGERPRINT),
    oracle_source_file_fingerprint: toSha256(process.env.COMPAT_ORACLE_SOURCE_FILE_FINGERPRINT),
    diagnostic_records: toNonnegativeInteger(process.env.COMPAT_DIAGNOSTIC_RECORDS),
    diagnostic_fingerprint: toSha256(process.env.COMPAT_DIAGNOSTIC_FINGERPRINT),
    oracle_diagnostic_records: toNonnegativeInteger(process.env.COMPAT_ORACLE_DIAGNOSTIC_RECORDS),
    oracle_diagnostic_fingerprint: toSha256(process.env.COMPAT_ORACLE_DIAGNOSTIC_FINGERPRINT),
    stub_inventory_schema: stubInventorySchema === 2 ? 2 : null,
    stubbed_modules: toNonnegativeInteger(process.env.COMPAT_STUBBED_MODULES),
    stubbed_any_members: toNonnegativeInteger(process.env.COMPAT_STUBBED_ANY_MEMBERS),
    stub_inventory_fingerprint: toSha256(process.env.COMPAT_STUB_INVENTORY_FINGERPRINT),
    stub_inventory_owners: toStringArray(process.env.COMPAT_STUB_INVENTORY_OWNERS),
    oracle_classification: oracleClassification,
    diagnostic_deltas: diagnosticDeltas,
    diagnostic_subsystems: diagnosticSubsystems,
    primary_subsystem: diagnosticSubsystems[0]?.subsystem || null,
    diagnostic_codes: diagnosticCodes,
    tsc_diagnostic_codes: tscDiagnosticCodes,
    tsz_diagnostic_codes: tszDiagnosticCodes,
    tsgo_diagnostic_codes: tsgoDiagnosticCodes,
    emit_status: "not in scope (noEmit project check)",
    dts_status: "not in scope (noEmit project check)",
    known_blockers: knownBlockers,
    reduced_repro_path: repro.reduced_repro_path,
    repro,
    exit_codes: {
      tsc: tscExitCodes,
      tsz: tszExitCodes,
      tsgo: tsgoExitCodes,
    },
    diagnostic_counts: {
      tsc: perSourceDeltas.tsc.length,
      tsz: perSourceDeltas.tsz.length,
      tsgo: perSourceDeltas.tsgo.length,
    },
    files_reached: filesReached,
    files_reached_reason: filesReachedReason,
    peak_memory_bytes: peakMemoryBytes,
    peak_memory_bytes_reason: peakMemoryBytesReason,
    fixture_sources: fixtureSources,
  };
  applyEvidenceState(row, evidenceSchema);
  fs.appendFileSync(outputFile, `${JSON.stringify(row)}\n`, "utf8");
}

function summarize() {
  const generatedAt = new Date().toISOString();
  const { rows, malformedLineCount, malformedExamples } = readRows(process.env.SUMMARY_JSONL_FILE || "");
  let outputFile;
  try {
    outputFile = resolveWritableFile({
      value: process.env.SUMMARY_OUTPUT_FILE,
      label: "project compatibility summary",
      root: process.env.SUMMARY_OUTPUT_ROOT,
      forbidden: [process.env.SUMMARY_JSONL_FILE],
    });
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exit(1);
  }

  // Ensure every row has a `state` populated before the single-pass
  // aggregator runs. Recorded rows already carry `state`; the few that
  // don't (legacy fixtures, malformed manual inputs) get derived once
  // here so the aggregator stays a pure data-flow function.
  for (const row of rows) {
    if (!row?.state) {
      row.state = rowStateFrom({
        exitClass: row?.exit_class,
        diagnosticStatus: row?.diagnostic_status,
      });
    }
    if (row?.state === "green") applyEvidenceState(row, row.evidence_schema);
  }

  // Single-pass row aggregation: by_state, by_oracle_classification, top
  // diagnostic deltas, and the residency table are all built in one walk
  // over `rows`. The old code path had four separate scans (two reduces,
  // a filter+sort+iter+map for top deltas, and another filter+sort+map for
  // residency) which scaled superlinearly when every row carried up to 20
  // diagnostic delta lines.
  const {
    byState,
    byOracleClassification,
    topDiagnosticDeltas: firstDiagnosticDeltas,
    residencyByRow,
  } = aggregateRowsForSummary(rows, {
    topDeltasLimit: 3,
    oracleClassifications: ORACLE_CLASSIFICATIONS,
  });

  const summary = {
    ...artifactMetadata(process.env, "SUMMARY", generatedAt),
    project_set: process.env.SUMMARY_PROJECT_SET || "required",
    project_filter: process.env.SUMMARY_PROJECT_FILTER || "",
    allow_failures: process.env.SUMMARY_ALLOW_FAILURES === "1",
    failures: Number(process.env.SUMMARY_FAILURES || 0),
    row_count: rows.length,
    malformed_jsonl_lines: malformedLineCount,
    malformed_jsonl_examples: malformedExamples,
    by_state: byState,
    by_oracle_classification: byOracleClassification,
    first_diagnostic_deltas: firstDiagnosticDeltas,
    residency_by_row: residencyByRow,
    rows,
  };

  fs.writeFileSync(outputFile, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
}

function readSummary(file) {
  if (!file) return null;
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    throw error;
  }
}

function renderStepSummaryMarkdown(summary, options) {
  const { title, artifactName, artifactUrl, jsonlPath, summaryPath } = options;
  const lines = [];
  lines.push(`### ${title || "Project compatibility artifact"}`);
  if (artifactUrl) {
    lines.push(`- Artifact: [${artifactName || "project-compatibility"}](${artifactUrl})`);
  } else if (artifactName) {
    lines.push(`- Artifact: ${artifactName}`);
  }
  if (jsonlPath) lines.push(`- JSONL: \`${jsonlPath}\``);
  if (summaryPath) {
    const suffix = summary?.missing ? " (not produced)" : "";
    lines.push(`- Summary: \`${summaryPath}\`${suffix}`);
  }
  if (summary?.missing) {
    return `${lines.join("\n")}\n`;
  }

  const byState = summary.by_state || {};
  const byOracle = summary.by_oracle_classification || {};
  const stateCounts = ROW_STATE_DISPLAY_ORDER
    .filter((key) => byState[key])
    .map((key) => `${key}=${byState[key]}`);
  if (stateCounts.length) {
    lines.push(`- Rows by state: ${stateCounts.join(", ")}`);
  }
  const oracleCounts = ORACLE_CLASSIFICATION_ORDER
    .filter((key) => byOracle[key])
    .map((key) => `${key}=${byOracle[key]}`);
  if (oracleCounts.length) {
    lines.push(`- Oracle classification: ${oracleCounts.join(", ")}`);
  }
  if (Number(summary.malformed_jsonl_lines || 0) > 0) {
    lines.push(`- Malformed JSONL lines: ${summary.malformed_jsonl_lines}`);
    const examples = Array.isArray(summary.malformed_jsonl_examples)
      ? summary.malformed_jsonl_examples
      : [];
    for (const example of examples.slice(0, 3)) {
      const line = example?.line ?? "unknown";
      const error = truncateForCell(example?.error || "unknown parse error", 120);
      lines.push(`  - line ${line}: ${error}`);
    }
  }

  // Residency for red/yellow rows is surfaced before any speedup/timing
  // section so reviewers can distinguish scale/runtime failure (OOM,
  // timeout, crash, unmeasured) from semantic divergence without scrolling.
  const residency = Array.isArray(summary.residency_by_row)
    ? summary.residency_by_row
    : [];
  if (residency.length) {
    lines.push("");
    lines.push("#### Residency (red/yellow rows)");
    lines.push("");
    lines.push("| Project | State | Files reached | Peak RSS |");
    lines.push("| --- | --- | --- | --- |");
    for (const item of residency) {
      const project = escapeMarkdownCell(item.project || "—");
      const state = escapeMarkdownCell(item.state || "—");
      const files = escapeMarkdownCell(renderResidencyCell(
        item.files_reached,
        item.files_reached_reason,
        formatFilesReached,
      ));
      const memory = escapeMarkdownCell(renderResidencyCell(
        item.peak_memory_bytes,
        item.peak_memory_bytes_reason,
        formatPeakMemoryBytes,
      ));
      lines.push(`| ${project} | ${state} | ${files} | ${memory} |`);
    }
  }

  const deltas = Array.isArray(summary.first_diagnostic_deltas)
    ? summary.first_diagnostic_deltas
    : [];
  if (deltas.length) {
    lines.push("");
    lines.push("#### First diagnostic deltas");
    lines.push("");
    lines.push("| Project | Oracle | Subsystem | Code | Delta |");
    lines.push("| --- | --- | --- | --- | --- |");
    for (const item of deltas) {
      const project = escapeMarkdownCell(item.project || "—");
      const oracle = escapeMarkdownCell(item.oracle_classification || "unknown");
      const subsystem = escapeMarkdownCell(item.subsystem || "—");
      const code = escapeMarkdownCell(item.code || "—");
      const delta = escapeMarkdownCell(truncateForCell(item.delta || "", 160));
      lines.push(`| ${project} | ${oracle} | ${subsystem} | ${code} | ${delta} |`);
    }
    if (jsonlPath || summaryPath) {
      lines.push("");
      lines.push("See artifact for the remaining diagnostic deltas.");
    }
  }

  return `${lines.join("\n")}\n`;
}

function renderResidencyCell(value, reason, formatter) {
  if (value !== null && value !== undefined && Number.isFinite(Number(value))) {
    return formatter(Number(value));
  }
  return reason ? `n/a (${reason})` : "n/a";
}

function formatFilesReached(value) {
  return Number.isInteger(value) ? value.toLocaleString("en-US") : String(value);
}

function formatPeakMemoryBytes(value) {
  if (!Number.isFinite(value) || value <= 0) return String(value);
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let scaled = value;
  let unit = 0;
  while (scaled >= 1024 && unit < units.length - 1) {
    scaled /= 1024;
    unit += 1;
  }
  const digits = scaled >= 100 ? 0 : scaled >= 10 ? 1 : 2;
  return `${scaled.toFixed(digits)} ${units[unit]}`;
}

function escapeMarkdownCell(value) {
  return String(value || "").replace(/\|/g, "\\|").replace(/\n/g, " ").trim();
}

function truncateForCell(value, max) {
  const text = String(value || "");
  if (text.length <= max) return text;
  return `${text.slice(0, max - 1)}…`;
}

function formatStepSummary() {
  const inputFile = process.env.SUMMARY_INPUT_FILE;
  const summary = readSummary(inputFile) || { missing: true };
  const markdown = renderStepSummaryMarkdown(summary, {
    title: process.env.SUMMARY_TITLE || "Project compatibility artifact",
    artifactName: process.env.SUMMARY_ARTIFACT_NAME || "",
    artifactUrl: process.env.SUMMARY_ARTIFACT_URL || "",
    jsonlPath: process.env.SUMMARY_JSONL_PATH || "",
    summaryPath: process.env.SUMMARY_SUMMARY_PATH || inputFile || "",
  });
  process.stdout.write(markdown);
}

const command = process.argv[2];
if (command === "record") {
  record();
} else if (command === "summary") {
  summarize();
} else if (command === "format-step-summary") {
  formatStepSummary();
} else {
  console.error("usage: project-compatibility.mjs <record|summary|format-step-summary>");
  process.exit(2);
}
