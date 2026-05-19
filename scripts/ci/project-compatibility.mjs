#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import {
  normalizePath,
  semanticFamiliesForFile,
} from "./type-challenges-semantic-families.mjs";

const DIAGNOSTIC_SUBSYSTEM_RULES = [
  ["project-config", new Set(["TS18003", "TS5052", "TS5069", "TS5070", "TS5083", "TS5110", "TS6053", "TS2688"])],
  ["syntax-parser-jsdoc", new Set(["TS1005", "TS1109", "TS1128", "TS17004", "TS8010", "TS8023", "TS8032"])],
  ["module-symbol-resolution", new Set(["TS2304", "TS2305", "TS2306", "TS2307", "TS2451", "TS2503", "TS2580", "TS2583", "TS2664", "TS2665", "TS2666", "TS2694"])],
  ["relations-assignability", new Set(["TS2322", "TS2345", "TS2352", "TS2394", "TS2416", "TS2420", "TS2430", "TS2559", "TS2740", "TS2741", "TS2769"])],
  ["evaluation-inference-instantiation", new Set(["TS2313", "TS2314", "TS2315", "TS2344", "TS2558", "TS2589", "TS2590", "TS2615", "TS7022"])],
  ["keyspace-property-indexed", new Set(["TS2339", "TS2353", "TS2536", "TS2537", "TS2538", "TS2540", "TS4111", "TS7053"])],
  ["flow-narrowing", new Set(["TS2367", "TS2677", "TS2774", "TS18047", "TS18048"])],
  ["class-this-accessor", new Set(["TS2415", "TS2511", "TS2515", "TS2526", "TS2683", "TS4113", "TS4114"])],
  ["emit-dts-nameability", new Set(["TS4023", "TS4058", "TS4082", "TS4094", "TS9005", "TS9039"])],
];

const OWNER_TRACK_BY_SUBSYSTEM = new Map([
  ["project-config", "Track 1 project-corpus harness/config"],
  ["syntax-parser-jsdoc", "Track 8 syntax/parser/jsdoc parity"],
  ["module-symbol-resolution", "Track 7 lib/module identity"],
  ["relations-assignability", "Track 4 relation diagnostics/compatibility"],
  ["evaluation-inference-instantiation", "Track 2/3 conditional, mapped, inference, instantiation"],
  ["keyspace-property-indexed", "Track 5 keyspace/property/indexed access"],
  ["flow-narrowing", "Track 6 flow/narrowing"],
  ["class-this-accessor", "Track 4 class/this/accessor compatibility"],
  ["emit-dts-nameability", "emit/dts nameability"],
  ["uncoded diagnostic", "Track 1 triage"],
  ["unclassified diagnostic", "Track 1 triage"],
]);

const TYPE_CHALLENGES_PROJECT_ROWS = new Set([
  "type-challenges-solutions-project",
]);

function ownerTrackForSubsystem(subsystem) {
  if (subsystem?.startsWith("type-challenges ")) {
    if (subsystem.includes("indexed access")) {
      return "Track 5 Type Challenges keyspace/indexed access";
    }
    return "Track 2/3 Type Challenges type-level semantics";
  }
  return OWNER_TRACK_BY_SUBSYSTEM.get(subsystem);
}

const DELTA_SOURCES = ["tsc", "tsz", "tsgo"];
const DELTA_SOURCE_SET = new Set(DELTA_SOURCES);
const SOURCE_LABEL_PATTERN = /^([a-z][\w-]*):\s+(.*)$/;

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
const ROW_STATE_PRIORITY = { red: 0, yellow: 1, gray: 2, green: 3 };

// Closed vocabulary for the structured reason a residency field is absent.
// Null is reserved for "measurement present"; every other value must come
// from these sets so dashboards can group residency gaps deterministically.
const FILES_REACHED_REASONS = new Set([
  "runner did not count",
  "not in scope",
  "process exited before counting",
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
  const seen = new Set();
  const codes = [];
  for (const match of matches) {
    const parsed = Number(match);
    if (!Number.isInteger(parsed) || seen.has(parsed)) continue;
    seen.add(parsed);
    codes.push(parsed);
  }
  return codes;
}

function splitDeltaLines(value) {
  return String(value || "")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function stripSourceLabel(line) {
  const text = String(line || "").trim();
  const match = text.match(SOURCE_LABEL_PATTERN);
  if (!match) return { source: null, body: text };
  const source = match[1].toLowerCase();
  if (!DELTA_SOURCE_SET.has(source)) return { source: null, body: text };
  return { source, body: match[2].trim() };
}

// Unattributed lines are folded into the tsz bucket downstream because tsz
// is the active failing side in the common project-row failure path.
function partitionDeltaBySource(lines) {
  const buckets = { unattributed: [] };
  for (const source of DELTA_SOURCES) buckets[source] = [];
  for (const line of lines) {
    const { source, body } = stripSourceLabel(line);
    (source ? buckets[source] : buckets.unattributed).push(body);
  }
  return buckets;
}

function perSourceDeltasFrom(env, unifiedLines) {
  const explicit = {
    tsc: env.COMPAT_TSC_DIAGNOSTIC_DELTA,
    tsz: env.COMPAT_TSZ_DIAGNOSTIC_DELTA,
    tsgo: env.COMPAT_TSGO_DIAGNOSTIC_DELTA,
  };
  const needsPartition = DELTA_SOURCES.some((source) => explicit[source] === undefined);
  const partitioned = needsPartition
    ? partitionDeltaBySource(unifiedLines)
    : { tsc: [], tsz: [], tsgo: [], unattributed: [] };
  return {
    tsc: explicit.tsc !== undefined ? splitDeltaLines(explicit.tsc) : partitioned.tsc,
    tsz: explicit.tsz !== undefined
      ? splitDeltaLines(explicit.tsz)
      : [...partitioned.tsz, ...partitioned.unattributed],
    tsgo: explicit.tsgo !== undefined ? splitDeltaLines(explicit.tsgo) : partitioned.tsgo,
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
  const failed = (exitCodes, diagnosticCodes) => (
    exitCodes.length > 0 ? exitCodes.some((code) => code !== 0) : diagnosticCodes.length > 0
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
  // (crashes, timeouts, etc.) classify together rather than as a divergence.
  if (tscSet.size === tszSet.size && [...tscSet].every((code) => tszSet.has(code))) {
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

function subsystemForCode(code) {
  for (const [subsystem, codes] of DIAGNOSTIC_SUBSYSTEM_RULES) {
    if (codes.has(code)) return subsystem;
  }
  return "unclassified diagnostic";
}

function diagnosticSubsystemsFrom(deltas) {
  const groups = new Map();
  for (const line of deltas) {
    const codes = [...line.matchAll(/\bTS\d{4,5}\b/g)].map((match) => match[0]);
    const lineCodes = codes.length ? codes : ["uncoded"];
    for (const code of lineCodes) {
      const subsystem = code === "uncoded" ? "uncoded diagnostic" : subsystemForCode(code);
      if (!groups.has(subsystem)) {
        groups.set(subsystem, { subsystem, codes: [], count: 0, examples: [] });
      }
      const group = groups.get(subsystem);
      group.count += 1;
      if (code !== "uncoded" && !group.codes.includes(code) && group.codes.length < 8) {
        group.codes.push(code);
      }
      if (group.examples.length < 3) {
        group.examples.push(line);
      }
    }
  }
  return [...groups.values()];
}

function parseDiagnosticDelta(line) {
  const withoutLabel = String(line || "").replace(/^[a-z][\w-]*:\s+/, "");
  const parenMatch = withoutLabel.match(
    /^(.+?)\((\d+),(\d+)\):\s+(?:error\s+)?(TS\d{4,5})/,
  );
  if (parenMatch) {
    return {
      path: parenMatch[1],
      code: parenMatch[4],
    };
  }

  const colonMatch = withoutLabel.match(
    /^(.+?):(\d+):(\d+)(?:\s+-)?\s+(?:error\s+)?(TS\d{4,5})/,
  );
  if (colonMatch) {
    return {
      path: colonMatch[1],
      code: colonMatch[4],
    };
  }
  return {
    path: null,
    code: null,
  };
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

function typeChallengesDiagnosticSubsystemsFrom(projectName, deltas) {
  if (!TYPE_CHALLENGES_PROJECT_ROWS.has(projectName)) {
    return [];
  }

  const groups = new Map();
  const sourceRoots = sourceRootsForTypeChallenges();
  const sourceCache = new Map();
  for (const line of deltas) {
    const diagnostic = parseDiagnosticDelta(line);
    const codes = diagnostic.code
      ? [diagnostic.code]
      : [...line.matchAll(/\bTS\d{4,5}\b/g)].map((match) => match[0]);
    const lineCodes = codes.length ? codes : ["uncoded"];
    const families = typeChallengesFamiliesForFile(diagnostic.path, sourceRoots, sourceCache);
    if (families.length === 1 && families[0] === "unknown") {
      continue;
    }

    for (const family of families) {
      const subsystem = `type-challenges ${family}`;
      if (!groups.has(subsystem)) {
        groups.set(subsystem, { subsystem, codes: [], count: 0, examples: [] });
      }
      const group = groups.get(subsystem);
      group.count += 1;
      for (const code of lineCodes) {
        if (code !== "uncoded" && !group.codes.includes(code) && group.codes.length < 8) {
          group.codes.push(code);
        }
      }
      if (group.examples.length < 3) {
        group.examples.push(line);
      }
    }
  }
  return [...groups.values()];
}

function diagnosticSubsystemsForProject(projectName, deltas) {
  const typeChallengesSubsystems = typeChallengesDiagnosticSubsystemsFrom(projectName, deltas);
  return typeChallengesSubsystems.length ? typeChallengesSubsystems : diagnosticSubsystemsFrom(deltas);
}

function diagnosticCodesFrom(deltas) {
  return codesFromLines(deltas, 8);
}

function knownBlockersFrom({ exitClass, phase, diagnosticSubsystems, diagnosticCodes }) {
  const blockers = [];
  const add = (blocker) => {
    if (blocker && !blockers.includes(blocker) && blockers.length < 8) blockers.push(blocker);
  };

  if (exitClass === "timeout") add("timeout during project check");
  if (exitClass === "oom") add("OOM or killed during project check");
  if (exitClass === "crash") add("compiler crash during project check");
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
    exitClass === "runner error"
  ) {
    return "red";
  }
  return "yellow";
}

function ownerTrackFrom({ exitClass, diagnosticSubsystems }) {
  if (exitClass === "timeout") return "Track 1 runtime/timeout triage";
  if (exitClass === "oom") return "Track 1 residency triage";
  if (exitClass === "crash") return "Track 1 crash triage";
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

function firstDiagnosticLocation(diagnosticDeltas) {
  for (const line of diagnosticDeltas) {
    const withoutLabel = String(line || "").replace(/^[a-z][\w-]*:\s+/, "");
    const parenMatch = withoutLabel.match(/^(.+?)\((\d+),(\d+)\):\s+(?:error\s+)?(TS\d{4,5})/);
    if (parenMatch) {
      return {
        path: relativeToFixture(parenMatch[1]),
        line: Number(parenMatch[2]),
        column: Number(parenMatch[3]),
        code: parenMatch[4],
      };
    }

    const colonMatch = withoutLabel.match(/^(.+?):(\d+):(\d+)(?:\s+-)?\s+(?:error\s+)?(TS\d{4,5})/);
    if (colonMatch) {
      return {
        path: relativeToFixture(colonMatch[1]),
        line: Number(colonMatch[2]),
        column: Number(colonMatch[3]),
        code: colonMatch[4],
      };
    }
  }
  return null;
}

function reproFrom(diagnosticDeltas) {
  const location = firstDiagnosticLocation(diagnosticDeltas);
  const tsconfigPath = relativeToFixture(process.env.COMPAT_TSCONFIG_PATH || "");
  const sourceRoot = relativeToFixture(process.env.COMPAT_SOURCE_ROOT || "");
  const reducedReproPath = location?.path || sourceRoot || tsconfigPath || null;

  return {
    tsconfig_path: tsconfigPath,
    source_root: sourceRoot,
    first_failure_path: location?.path || null,
    first_failure_line: location?.line ?? null,
    first_failure_column: location?.column ?? null,
    first_failure_code: location?.code || null,
    reduced_repro_path: reducedReproPath,
    command: tsconfigPath ? `$TSZ_BIN --noEmit -p ${tsconfigPath}` : null,
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

  const diagnosticSubsystems = diagnosticSubsystemsForProject(projectName, diagnosticDeltas);
  const diagnosticCodes = diagnosticCodesFrom(diagnosticDeltas);

  const tscExitCodes = toExitCodes(process.env.COMPAT_TSC_EXIT_CODES);
  const tszExitCodes = toExitCodes(process.env.COMPAT_TSZ_EXIT_CODES);
  const tsgoExitCodes = toExitCodes(process.env.COMPAT_TSGO_EXIT_CODES);

  const perSourceDeltas = perSourceDeltasFrom(process.env, diagnosticDeltas);
  const tscDiagnosticCodes = codesFromLines(perSourceDeltas.tsc, 8);
  const tszDiagnosticCodes = codesFromLines(perSourceDeltas.tsz, 8);
  const tsgoDiagnosticCodes = codesFromLines(perSourceDeltas.tsgo, 8);

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
  const repro = reproFrom(diagnosticDeltas);
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
  fs.appendFileSync(outputFile, `${JSON.stringify(row)}\n`, "utf8");
}

function topDiagnosticDeltasFrom(rows, limit) {
  const priority = (state) => ROW_STATE_PRIORITY[state] ?? 4;
  const sorted = rows
    .filter((row) => Array.isArray(row?.diagnostic_deltas) && row.diagnostic_deltas.length > 0)
    .sort((a, b) => priority(a.state) - priority(b.state));

  const items = [];
  for (const row of sorted) {
    const deltas = row.diagnostic_deltas;

    const subsystemByLine = new Map();
    const subsystems = Array.isArray(row.diagnostic_subsystems) ? row.diagnostic_subsystems : [];
    for (const group of subsystems) {
      if (!group?.subsystem) continue;
      for (const example of group.examples || []) {
        if (!subsystemByLine.has(example)) {
          subsystemByLine.set(example, group.subsystem);
        }
      }
    }

    for (const delta of deltas) {
      const parsed = parseDiagnosticDelta(delta);
      const subsystem = subsystemByLine.get(delta)
        || (parsed.code ? subsystemForCode(parsed.code) : "unattributed");
      items.push({
        project: row.name || null,
        oracle_classification: row.oracle_classification || "unknown",
        state: row.state || null,
        code: parsed.code || null,
        path: parsed.path || null,
        subsystem,
        delta,
      });
      if (items.length >= limit) return items;
    }
  }
  return items;
}

// Residency facts (files_reached, peak_memory_bytes) are surfaced for every
// red/yellow row so a reader can distinguish semantic failure from runner /
// timeout / OOM / scale failure without scrolling through full row JSON.
// Rows with neither a measurement nor a reason are still listed; the table
// would otherwise hide schema gaps.
function residencyByRowFrom(rows) {
  const priority = (state) => ROW_STATE_PRIORITY[state] ?? 4;
  return rows
    .filter((row) => row?.state === "red" || row?.state === "yellow")
    .sort((a, b) => priority(a.state) - priority(b.state))
    .map((row) => ({
      project: row.name || null,
      state: row.state || null,
      files_reached: row.files_reached ?? null,
      files_reached_reason: row.files_reached_reason ?? null,
      peak_memory_bytes: row.peak_memory_bytes ?? null,
      peak_memory_bytes_reason: row.peak_memory_bytes_reason ?? null,
    }));
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
  const byState = rows.reduce((counts, row) => {
    const key = row.state || rowStateFrom({
      exitClass: row.exit_class,
      diagnosticStatus: row.diagnostic_status,
    });
    counts[key] = (counts[key] || 0) + 1;
    return counts;
  }, {});

  const byOracleClassification = rows.reduce((counts, row) => {
    const key = ORACLE_CLASSIFICATIONS.has(row.oracle_classification)
      ? row.oracle_classification
      : "unknown";
    counts[key] = (counts[key] || 0) + 1;
    return counts;
  }, {});

  const firstDiagnosticDeltas = topDiagnosticDeltasFrom(rows, 3);
  const residencyByRow = residencyByRowFrom(rows);

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
