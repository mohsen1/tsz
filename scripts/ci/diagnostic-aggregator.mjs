#!/usr/bin/env node
// Single-pass aggregators for project-row diagnostic data.
//
// The structural rule these helpers enforce: every diagnostic-delta list is
// walked exactly once, and every row list is walked exactly once, with all
// needed summary buckets (codes, per-source split, subsystem groups, first
// failure location, reduction candidates, state/oracle counts, top deltas,
// residency table) populated incrementally on that single pass. Repeated
// scans of the same diagnostic list per code/family/section are forbidden:
// they were the source of issue #11598's quadratic blow-up.

import { subsystemForCode } from "./diagnostic-subsystems.mjs";

const DELTA_SOURCE_SET = new Set(["tsc", "tsz", "tsgo"]);
const SOURCE_LABEL_PATTERN = /^([a-z][\w-]*):\s+(.*)$/;
const DIAGNOSTIC_CODE_PATTERN = /\bTS\d{4,5}\b/g;
const PAREN_LOCATION_PATTERN = /^(.+?)\((\d+),(\d+)\):\s+(?:error\s+)?(TS\d{4,5})/;
const COLON_LOCATION_PATTERN = /^(.+?):(\d+):(\d+)(?:\s+-)?\s+(?:error\s+)?(TS\d{4,5})/;
const GLOBAL_DIAGNOSTIC_PATTERN = /^(?:error|warning)\s+(TS\d{4,5})\b/;

const CODE_LIMIT = 8;
const SUBSYSTEM_EXAMPLE_LIMIT = 3;
const REDUCTION_LIMIT = 5;

// Parses a single diagnostic delta line into the structured location it
// describes, after stripping any `source: ` label prefix. Returns null when
// the line is not a parsable located diagnostic or pathless
// `error TSnnnn`/`warning TSnnnn` global diagnostic. Global diagnostics use
// null path/line/column fields. Exported so callers that need to
// classify an individual line (e.g. summary subsystem lookup) reuse this
// canonical parser rather than re-implementing the same two regexes.
export function parseDiagnosticLine(rawLine) {
  const text = String(rawLine || "").trim();
  const labelMatch = text.match(SOURCE_LABEL_PATTERN);
  const body = labelMatch && DELTA_SOURCE_SET.has(labelMatch[1].toLowerCase())
    ? labelMatch[2].trim()
    : text;
  const paren = body.match(PAREN_LOCATION_PATTERN);
  if (paren) {
    return { path: paren[1], line: Number(paren[2]), column: Number(paren[3]), code: paren[4] };
  }
  const colon = body.match(COLON_LOCATION_PATTERN);
  if (colon) {
    return { path: colon[1], line: Number(colon[2]), column: Number(colon[3]), code: colon[4] };
  }
  const global = body.match(GLOBAL_DIAGNOSTIC_PATTERN);
  if (global) {
    return { path: null, line: null, column: null, code: global[1] };
  }
  return null;
}

function pushUnique(target, value, limit) {
  if (target.length >= limit) return;
  if (target.includes(value)) return;
  target.push(value);
}

// Single-pass aggregation over a row's diagnostic delta list.
//
// `deltas` is the canonical (already-trimmed, already-line-capped) list. The
// returned shape is intentionally a superset so callers can pick the buckets
// they care about without re-walking the list:
//
//   {
//     codes:                deduped TS codes in encounter order (capped),
//     codesBySource:        { tsc, tsz, tsgo, unattributed } — deduped codes,
//     bodiesBySource:       { tsc, tsz, tsgo, unattributed } — body lines,
//     subsystems:           [{ subsystem, codes, count, examples }],
//     firstLocation:        first parsable {path,line,column,code} or null,
//     reductionCandidates:  up to N coded lines, with uncoded fallback,
//   }
//
// `subsystemFor(line, parsedLocation) -> string[] | null` lets callers
// (e.g. type-challenges row classification) plug a different subsystem
// classifier into the same single-pass walk instead of re-iterating the
// delta list. Returning `null` means "use the default code -> subsystem
// table"; returning `[]` means "skip this line for subsystem grouping".
//
// All bucket caps mirror the previous individual helpers exactly, so the
// fingerprint of the recorded row is unchanged.
export function aggregateRowDeltas(deltas, { subsystemFor } = {}) {
  const codesBySource = { tsc: [], tsz: [], tsgo: [], unattributed: [] };
  const codesSeenBySource = {
    tsc: new Set(), tsz: new Set(), tsgo: new Set(), unattributed: new Set(),
  };
  const bodiesBySource = { tsc: [], tsz: [], tsgo: [], unattributed: [] };
  const subsystemGroups = new Map();
  // Global `codes` list and seen-set track input-order encounter across the
  // unified delta walk. Recording first-seen codes inline (rather than
  // re-deriving from `codesBySource` per source) preserves the historical
  // encounter order when source labels interleave — e.g. a `tsz: TS2322`
  // line before a `tsc: TS2304` line must report `[TS2322, TS2304]`,
  // not the source-bucket order `[TS2304, TS2322]`. This invariant feeds
  // `known_blockers` and summary routing, so reshuffling would change the
  // recorded row fingerprint without any underlying diagnostic change.
  const codes = [];
  const codesSeen = new Set();
  const coded = [];
  const uncoded = [];
  let firstLocation = null;

  for (const rawLine of deltas) {
    const line = String(rawLine || "").trim();
    const labelMatch = line.match(SOURCE_LABEL_PATTERN);
    const labeledSource = labelMatch && DELTA_SOURCE_SET.has(labelMatch[1].toLowerCase())
      ? labelMatch[1].toLowerCase()
      : null;
    const body = labeledSource ? labelMatch[2].trim() : line;
    const sourceKey = labeledSource ?? "unattributed";
    bodiesBySource[sourceKey].push(body);

    const lineCodes = [];
    for (const m of body.matchAll(DIAGNOSTIC_CODE_PATTERN)) lineCodes.push(m[0]);
    const hasCodes = lineCodes.length > 0;

    if (hasCodes) coded.push(line);
    else uncoded.push(line);

    // Each line is parsed exactly once; the result feeds both
    // first-location capture and any custom subsystem hook.
    const parsedLocation = parseDiagnosticLine(line);
    if (firstLocation === null && parsedLocation) firstLocation = parsedLocation;

    if (hasCodes) {
      const sourceCodeList = codesBySource[sourceKey];
      const sourceCodeSeen = codesSeenBySource[sourceKey];
      for (const code of lineCodes) {
        if (!codesSeen.has(code) && codes.length < CODE_LIMIT) {
          codesSeen.add(code);
          codes.push(code);
        }
        if (!sourceCodeSeen.has(code) && sourceCodeList.length < CODE_LIMIT) {
          sourceCodeSeen.add(code);
          sourceCodeList.push(code);
        }
      }
    }

    const customSubsystems = subsystemFor ? subsystemFor(line, parsedLocation) : null;
    let subsystems;
    if (customSubsystems !== null && customSubsystems !== undefined) {
      if (customSubsystems.length === 0) continue;
      subsystems = customSubsystems;
    } else if (hasCodes) {
      subsystems = lineCodes.map(subsystemForCode);
    } else {
      subsystems = ["uncoded diagnostic"];
    }

    for (const subsystem of subsystems) {
      let group = subsystemGroups.get(subsystem);
      if (!group) {
        group = { subsystem, codes: [], count: 0, examples: [] };
        subsystemGroups.set(subsystem, group);
      }
      group.count += 1;
      // Each code on the line still flows into the matching group's code
      // list when the custom hook returned codes-per-line (default case);
      // for hooks that return categorical labels we still record codes
      // because that's what the existing recorded shape expects.
      if (hasCodes) {
        for (const code of lineCodes) pushUnique(group.codes, code, CODE_LIMIT);
      }
      if (group.examples.length < SUBSYSTEM_EXAMPLE_LIMIT) group.examples.push(line);
    }
  }

  const reductionSource = coded.length ? coded : uncoded;
  const reductionCandidates = reductionSource.slice(0, REDUCTION_LIMIT);

  return {
    codes,
    codesBySource,
    bodiesBySource,
    subsystems: [...subsystemGroups.values()],
    firstLocation,
    reductionCandidates,
  };
}

// Single-pass aggregation over a row list for summary generation.
//
// Returns:
//   {
//     byState:                  { green, yellow, red, gray, ... },
//     byOracleClassification:   { both-pass, tsz-fails-only, ... },
//     topDiagnosticDeltas:      up to N items, ordered by row state priority,
//     residencyByRow:           red+yellow rows, ordered by state priority,
//   }
//
// The function avoids the previous "4 passes over rows + extra sort of all
// rows" pattern: each row contributes to every bucket as we visit it.
// State-priority order is preserved by bucketing rows-with-deltas / red /
// yellow into per-state lists and then stitching them together at the end,
// which is O(rowsWithDeltas) for stitching versus O(R log R) for sorting.
export function aggregateRowsForSummary(rows, options = {}) {
  const {
    topDeltasLimit = 3,
    oracleClassifications,
    stateOrder = ["red", "yellow", "gray", "green"],
  } = options;

  const byState = {};
  const byOracleClassification = {};
  const rowsByStateWithDeltas = { red: [], yellow: [], gray: [], green: [] };
  const residencyByState = { red: [], yellow: [] };

  for (const row of rows) {
    const stateKey = row?.state || "unknown";
    byState[stateKey] = (byState[stateKey] || 0) + 1;

    const rawOracle = row?.oracle_classification;
    const oracleKey = oracleClassifications && !oracleClassifications.has(rawOracle)
      ? "unknown"
      : (rawOracle || "unknown");
    byOracleClassification[oracleKey] = (byOracleClassification[oracleKey] || 0) + 1;

    if (stateKey === "red" || stateKey === "yellow") {
      residencyByState[stateKey].push(row);
    }
    if (Array.isArray(row?.diagnostic_deltas) && row.diagnostic_deltas.length > 0) {
      const bucket = rowsByStateWithDeltas[stateKey] || (rowsByStateWithDeltas[stateKey] = []);
      bucket.push(row);
    }
  }

  const topDiagnosticDeltas = [];
  outer: for (const state of stateOrder) {
    const bucket = rowsByStateWithDeltas[state];
    if (!bucket) continue;
    for (const row of bucket) {
      const subsystemByLine = new Map();
      const subsystems = Array.isArray(row.diagnostic_subsystems) ? row.diagnostic_subsystems : [];
      for (const group of subsystems) {
        if (!group?.subsystem) continue;
        for (const example of group.examples || []) {
          if (!subsystemByLine.has(example)) subsystemByLine.set(example, group.subsystem);
        }
      }
      for (const delta of row.diagnostic_deltas) {
        const parsed = parseDiagnosticLine(delta) || { path: null, code: null };
        const subsystem = subsystemByLine.get(delta)
          || (parsed.code ? subsystemForCode(parsed.code) : "unattributed");
        topDiagnosticDeltas.push({
          project: row.name || null,
          oracle_classification: row.oracle_classification || "unknown",
          state: row.state || null,
          code: parsed.code || null,
          path: parsed.path || null,
          subsystem,
          delta,
        });
        if (topDiagnosticDeltas.length >= topDeltasLimit) break outer;
      }
    }
  }

  const residencyByRow = [];
  for (const state of stateOrder) {
    if (state !== "red" && state !== "yellow") continue;
    for (const row of residencyByState[state] || []) {
      residencyByRow.push({
        project: row?.name || null,
        state: row?.state || null,
        files_reached: row?.files_reached ?? null,
        files_reached_reason: row?.files_reached_reason ?? null,
        peak_memory_bytes: row?.peak_memory_bytes ?? null,
        peak_memory_bytes_reason: row?.peak_memory_bytes_reason ?? null,
      });
    }
  }

  return {
    byState,
    byOracleClassification,
    topDiagnosticDeltas,
    residencyByRow,
  };
}
