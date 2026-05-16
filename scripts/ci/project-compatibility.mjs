#!/usr/bin/env node
import fs from "node:fs";

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

function readRows(input) {
  try {
    return fs.readFileSync(input, "utf8")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => JSON.parse(line));
  } catch {
    return [];
  }
}

function record() {
  const delta = process.env.COMPAT_DIAGNOSTIC_DELTA || "";
  const diagnosticDeltas = delta
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 20);

  const diagnosticSubsystems = diagnosticSubsystemsFrom(diagnosticDeltas);
  const diagnosticCodes = diagnosticCodesFrom(diagnosticDeltas);
  const exitClass = process.env.COMPAT_EXIT_CLASS || "unknown";
  const diagnosticStatus = process.env.COMPAT_DIAGNOSTIC_STATUS || "unknown";
  const row = {
    name: process.env.COMPAT_NAME || "",
    exit_class: exitClass,
    phase: process.env.COMPAT_PHASE || "unknown",
    last_successful_phase: lastSuccessfulPhaseFrom({ exitClass, diagnosticStatus }),
    diagnostic_status: diagnosticStatus,
    diagnostic_deltas: diagnosticDeltas,
    diagnostic_subsystems: diagnosticSubsystems,
    primary_subsystem: diagnosticSubsystems[0]?.subsystem || null,
    diagnostic_codes: diagnosticCodes,
    emit_status: "not in scope (noEmit project check)",
    dts_status: "not in scope (noEmit project check)",
    known_blockers: knownBlockersFrom({
      exitClass,
      phase: process.env.COMPAT_PHASE || "unknown",
      diagnosticSubsystems,
      diagnosticCodes,
    }),
    exit_codes: {
      tsc: [],
      tsz: toExitCodes(process.env.COMPAT_TSZ_EXIT_CODES),
      tsgo: [],
    },
    files_reached: toNumber(process.env.COMPAT_FILES_REACHED),
    peak_memory_bytes: toNumber(process.env.COMPAT_PEAK_MEMORY_BYTES),
  };

  fs.appendFileSync(process.env.COMPAT_JSONL_FILE, `${JSON.stringify(row)}\n`, "utf8");
}

function summarize() {
  const rows = readRows(process.env.SUMMARY_JSONL_FILE || "");
  const byState = rows.reduce((counts, row) => {
    const key = row.exit_class === "exit success" && row.diagnostic_status === "none"
      ? "green"
      : row.exit_class === "nonzero exit"
        ? "red"
        : "yellow";
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
