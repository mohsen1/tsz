#!/usr/bin/env node

export const DIAGNOSTIC_SUBSYSTEM_RULES = [
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

const CODE_TO_SUBSYSTEM = new Map();
for (const [subsystem, codes] of DIAGNOSTIC_SUBSYSTEM_RULES) {
  for (const code of codes) CODE_TO_SUBSYSTEM.set(code, subsystem);
}

export const OWNER_TRACK_BY_SUBSYSTEM = new Map([
  ["project-config", "Track 1 project-corpus harness/config"],
  ["syntax-parser-jsdoc", "Track 8 syntax/parser/jsdoc parity"],
  ["module-symbol-resolution", "Track 7 lib/module identity"],
  ["relations-assignability", "Track 4 relation diagnostics/compatibility"],
  ["evaluation-inference-instantiation", "Track 2/3 conditional, mapped, inference, instantiation"],
  ["keyspace-property-indexed", "Track 5 keyspace/property/indexed access"],
  ["flow-narrowing", "Track 6 flow/narrowing"],
  ["class-this-accessor", "Track 4 class/this/accessor compatibility"],
  ["emit-dts-nameability", "emit/dts nameability"],
]);

export const CRATE_BY_SUBSYSTEM = new Map([
  ["project-config", "bench"],
  ["syntax-parser-jsdoc", "parser"],
  ["module-symbol-resolution", "checker"],
  ["relations-assignability", "solver"],
  ["evaluation-inference-instantiation", "solver"],
  ["keyspace-property-indexed", "solver"],
  ["flow-narrowing", "solver"],
  ["class-this-accessor", "solver"],
  ["emit-dts-nameability", "emitter"],
  ["runtime-timeout", "bench"],
  ["runtime-oom", "bench"],
  ["runtime-crash", "bench"],
  ["runner-error", "bench"],
]);

export const LABELS_BY_SUBSYSTEM = new Map([
  ["project-config", ["bench"]],
  ["syntax-parser-jsdoc", ["bench", "parser"]],
  ["module-symbol-resolution", ["checker"]],
  ["relations-assignability", ["solver", "checker"]],
  ["evaluation-inference-instantiation", ["solver"]],
  ["keyspace-property-indexed", ["solver"]],
  ["flow-narrowing", ["solver"]],
  ["class-this-accessor", ["solver", "checker"]],
  ["emit-dts-nameability", ["emitter"]],
]);

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
