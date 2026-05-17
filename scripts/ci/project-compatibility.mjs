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
  "type-challenges-project",
  "type-challenges-solutions-project",
  "type-challenges-assertions-tsc-clean",
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

function toNumber(value) {
  if (value === undefined || value === null || value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
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
  const codes = [];
  const seen = new Set();
  for (const line of deltas) {
    for (const match of String(line || "").matchAll(/\bTS\d{4,5}\b/g)) {
      const code = match[0];
      if (seen.has(code)) continue;
      seen.add(code);
      codes.push(code);
      if (codes.length >= 8) return codes;
    }
  }
  return codes;
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
  if (exitClass === "nonzero exit") return "red";
  return "yellow";
}

function ownerTrackFrom({ exitClass, diagnosticSubsystems }) {
  if (exitClass === "timeout") return "Track 1 runtime/timeout triage";
  if (exitClass === "oom") return "Track 1 residency triage";
  if (exitClass === "crash") return "Track 1 crash triage";
  if (exitClass === "fixture invalid") return "Track 1 project-corpus harness/config";
  if (exitClass === "runner error") return "Track 1 benchmark runner";
  if (exitClass === "tsz unavailable") return "Track 1 benchmark runner";

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

function record() {
  const delta = process.env.COMPAT_DIAGNOSTIC_DELTA || "";
  const diagnosticDeltas = delta
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 20);

  const projectName = process.env.COMPAT_NAME || "";
  const diagnosticSubsystems = diagnosticSubsystemsForProject(projectName, diagnosticDeltas);
  const diagnosticCodes = diagnosticCodesFrom(diagnosticDeltas);
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
  const row = {
    name: projectName,
    state,
    exit_class: exitClass,
    first_failure_class: state === "green" ? null : knownBlockers[0] || exitClass,
    owner_track: state === "green" ? null : ownerTrackFrom({ exitClass, diagnosticSubsystems }),
    phase: process.env.COMPAT_PHASE || "unknown",
    last_successful_phase: lastSuccessfulPhaseFrom({ exitClass, diagnosticStatus }),
    diagnostic_status: diagnosticStatus,
    diagnostic_deltas: diagnosticDeltas,
    diagnostic_subsystems: diagnosticSubsystems,
    primary_subsystem: diagnosticSubsystems[0]?.subsystem || null,
    diagnostic_codes: diagnosticCodes,
    emit_status: "not in scope (noEmit project check)",
    dts_status: "not in scope (noEmit project check)",
    known_blockers: knownBlockers,
    reduced_repro_path: repro.reduced_repro_path,
    repro,
    exit_codes: {
      tsc: toExitCodes(process.env.COMPAT_TSC_EXIT_CODES),
      tsz: toExitCodes(process.env.COMPAT_TSZ_EXIT_CODES),
      tsgo: toExitCodes(process.env.COMPAT_TSGO_EXIT_CODES),
    },
    files_reached: toNumber(process.env.COMPAT_FILES_REACHED),
    peak_memory_bytes: toNumber(process.env.COMPAT_PEAK_MEMORY_BYTES),
  };

  fs.appendFileSync(process.env.COMPAT_JSONL_FILE, `${JSON.stringify(row)}\n`, "utf8");
}

function summarize() {
  const { rows, malformedLineCount, malformedExamples } = readRows(process.env.SUMMARY_JSONL_FILE || "");
  const byState = rows.reduce((counts, row) => {
    const key = row.state || rowStateFrom({
      exitClass: row.exit_class,
      diagnosticStatus: row.diagnostic_status,
    });
    counts[key] = (counts[key] || 0) + 1;
    return counts;
  }, {});

  const summary = {
    generated_at: new Date().toISOString(),
    project_set: process.env.SUMMARY_PROJECT_SET || "required",
    project_filter: process.env.SUMMARY_PROJECT_FILTER || "",
    allow_failures: process.env.SUMMARY_ALLOW_FAILURES === "1",
    failures: Number(process.env.SUMMARY_FAILURES || 0),
    row_count: rows.length,
    malformed_jsonl_lines: malformedLineCount,
    malformed_jsonl_examples: malformedExamples,
    by_state: byState,
    rows,
  };

  fs.writeFileSync(process.env.SUMMARY_OUTPUT_FILE, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
}

const command = process.argv[2];
if (command === "record") {
  record();
} else if (command === "summary") {
  summarize();
} else {
  console.error("usage: project-compatibility.mjs <record|summary>");
  process.exit(2);
}
