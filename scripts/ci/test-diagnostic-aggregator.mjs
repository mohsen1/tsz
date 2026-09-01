#!/usr/bin/env node
import assert from "node:assert/strict";
import {
  aggregateRowDeltas,
  aggregateRowsForSummary,
  parseDiagnosticLine,
} from "./diagnostic-aggregator.mjs";

// Basic shape: codes are deduped in encounter order; subsystems group by
// owning track; firstLocation captures the first parsable line; per-source
// buckets are populated from the source label.
{
  const deltas = [
    "tsc: src/a.ts(1,1): error TS2304: Cannot find name 'foo'.",
    "tsz: src/b.ts(2,2): error TS2322: assignability failed.",
    "tsz: src/b.ts(3,3): error TS2322: assignability failed again.",
    "src/c.ts(4,4): error TS2345: argument mismatch.",
  ];
  const agg = aggregateRowDeltas(deltas);
  assert.deepEqual(agg.codes, ["TS2304", "TS2322", "TS2345"]);
  assert.deepEqual(agg.codesBySource.tsc, ["TS2304"]);
  assert.deepEqual(agg.codesBySource.tsz, ["TS2322"]);
  assert.deepEqual(agg.codesBySource.tsgo, []);
  assert.deepEqual(agg.codesBySource.unattributed, ["TS2345"]);
  assert.equal(agg.bodiesBySource.tsc.length, 1);
  assert.equal(agg.bodiesBySource.tsz.length, 2);
  assert.equal(agg.bodiesBySource.unattributed.length, 1);
  assert.equal(agg.firstLocation.code, "TS2304");
  assert.equal(agg.firstLocation.path, "src/a.ts");
  assert.equal(agg.firstLocation.line, 1);

  const subsystems = Object.fromEntries(agg.subsystems.map((g) => [g.subsystem, g]));
  assert.equal(subsystems["module-symbol-resolution"].count, 1);
  assert.equal(subsystems["relations-assignability"].count, 3);
}

// Encounter-order regression: when source labels interleave, the global
// `codes` list must reflect input order, NOT the source-bucket order. A
// previous refactor derived `codes` post-walk by iterating buckets in the
// fixed sequence (tsc, tsz, tsgo, unattributed); that reshuffled the order
// recorded into row.diagnostic_codes when a `tsz:` line preceded a `tsc:`
// line, which downstream feeds known_blockers/summary routing.
{
  const agg = aggregateRowDeltas([
    "tsz: src/a.ts(1,1): error TS2322: assignability failed.",
    "tsc: src/b.ts(2,2): error TS2304: Cannot find name 'foo'.",
    "tsz: src/c.ts(3,3): error TS2345: argument mismatch.",
  ]);
  assert.deepEqual(agg.codes, ["TS2322", "TS2304", "TS2345"]);
  // Per-source codes still group by source.
  assert.deepEqual(agg.codesBySource.tsz, ["TS2322", "TS2345"]);
  assert.deepEqual(agg.codesBySource.tsc, ["TS2304"]);
}

// Single-pass property: aggregateRowDeltas must populate every output bucket
// using a single linear walk. Drive that with a counted accessor and assert
// each delta line is read at most once for bucket population.
{
  const lines = Array.from({ length: 64 }, (_, i) => `tsz: src/x${i}.ts(${i + 1},1): error TS2322: m${i}`);
  let reads = 0;
  const probe = new Proxy(lines, {
    get(target, key) {
      if (typeof key === "string" && /^\d+$/.test(key)) reads++;
      return Reflect.get(target, key);
    },
  });
  aggregateRowDeltas(probe);
  // We expect one read per element (the for..of iteration). Subsystem rules
  // are looked up O(1) via the Map, so no per-line constant blow-up.
  assert.ok(reads <= lines.length + 2, `expected ~${lines.length} reads, got ${reads}`);
}

// Uncoded lines fall into the "uncoded diagnostic" subsystem and are kept
// out of the codes list. They still seed reductionCandidates as a fallback.
{
  const agg = aggregateRowDeltas([
    "tsz: runner note without a diagnostic code",
    "tsz: another note",
  ]);
  assert.deepEqual(agg.codes, []);
  assert.equal(agg.subsystems.length, 1);
  assert.equal(agg.subsystems[0].subsystem, "uncoded diagnostic");
  assert.equal(agg.subsystems[0].count, 2);
  assert.equal(agg.reductionCandidates.length, 2);
  assert.equal(agg.firstLocation, null);
}

// Reduction candidates prefer coded lines when any exist.
{
  const agg = aggregateRowDeltas([
    "src/a.ts(1,1): error TS2322: coded",
    "tsz: note without code",
    "src/b.ts(2,2): error TS2345: coded too",
  ]);
  assert.equal(agg.reductionCandidates.length, 2);
  assert.ok(agg.reductionCandidates[0].includes("TS2322"));
  assert.ok(agg.reductionCandidates[1].includes("TS2345"));
}

// Caps mirror the previous individual helpers exactly (deduped to 8 codes,
// 8 subsystem codes, 3 examples per group, 5 reduction candidates).
{
  const lines = Array.from({ length: 30 }, (_, i) => `tsz: src/x.ts(1,1): error TS23${String(i).padStart(2, "0")}: m`);
  const agg = aggregateRowDeltas(lines);
  assert.equal(agg.codes.length, 8);
  // First 8 unique codes win.
  assert.equal(agg.codes[0], "TS2300");
  assert.equal(agg.codes[7], "TS2307");
  assert.equal(agg.reductionCandidates.length, 5);
  for (const group of agg.subsystems) {
    assert.ok(group.codes.length <= 8);
    assert.ok(group.examples.length <= 3);
  }
}

// summary aggregator: state counts + oracle counts + top deltas +
// residency table built in one row pass with state-priority ordering.
{
  const rows = [
    { name: "alpha", state: "green", oracle_classification: "both-pass", diagnostic_deltas: [], diagnostic_subsystems: [] },
    {
      name: "beta",
      state: "yellow",
      oracle_classification: "tsz-fails-only",
      diagnostic_deltas: [
        "src/index.ts(1,1): error TS2322: mismatch one",
        "src/index.ts(2,2): error TS2322: mismatch two",
      ],
      diagnostic_subsystems: [
        {
          subsystem: "relations-assignability",
          codes: ["TS2322"],
          count: 2,
          examples: [
            "src/index.ts(1,1): error TS2322: mismatch one",
            "src/index.ts(2,2): error TS2322: mismatch two",
          ],
        },
      ],
      files_reached: 200,
      files_reached_reason: null,
      peak_memory_bytes: null,
      peak_memory_bytes_reason: "not measured on platform",
    },
    {
      name: "gamma",
      state: "red",
      oracle_classification: "tsc-fails-only",
      diagnostic_deltas: ["tsc: src/a.ts(1,1): error TS2304: Cannot find name 'foo'."],
      diagnostic_subsystems: [
        {
          subsystem: "module-symbol-resolution",
          codes: ["TS2304"],
          count: 1,
          examples: ["tsc: src/a.ts(1,1): error TS2304: Cannot find name 'foo'."],
        },
      ],
      files_reached: null,
      files_reached_reason: "runner did not count",
      peak_memory_bytes: null,
      peak_memory_bytes_reason: "process exited before sampling",
    },
  ];

  const result = aggregateRowsForSummary(rows, {
    topDeltasLimit: 3,
  });

  assert.deepEqual(result.byState, { green: 1, yellow: 1, red: 1 });
  assert.deepEqual(result.byOracleClassification, {
    "both-pass": 1,
    "tsc-fails-only": 1,
    "tsz-fails-only": 1,
  });
  assert.equal(result.topDiagnosticDeltas.length, 3);
  // Red rows come first by priority ordering.
  assert.equal(result.topDiagnosticDeltas[0].project, "gamma");
  assert.equal(result.topDiagnosticDeltas[0].subsystem, "module-symbol-resolution");
  assert.equal(result.topDiagnosticDeltas[1].project, "beta");
  assert.equal(result.topDiagnosticDeltas[2].project, "beta");
  // Residency includes only red + yellow rows, in priority order.
  assert.equal(result.residencyByRow.length, 2);
  assert.equal(result.residencyByRow[0].project, "gamma");
  assert.equal(result.residencyByRow[0].peak_memory_bytes_reason, "process exited before sampling");
  assert.equal(result.residencyByRow[1].project, "beta");
}

// The subsystemFor hook lets callers (e.g. type-challenges) classify lines
// without re-iterating the delta list outside the single pass.
{
  const deltas = [
    "src/a.ts(1,1): error TS2322: assignability failed.",
    "src/b.ts(2,2): error TS2345: argument mismatch.",
  ];
  let calls = 0;
  const agg = aggregateRowDeltas(deltas, {
    subsystemFor: (_line, parsed) => {
      calls++;
      return parsed?.path?.startsWith("src/a.") ? ["type-challenges utility"] : null;
    },
  });
  // Hook fires once per line (single pass).
  assert.equal(calls, deltas.length);
  const labels = agg.subsystems.map((g) => g.subsystem).sort();
  assert.deepEqual(labels, ["relations-assignability", "type-challenges utility"]);
  const tcGroup = agg.subsystems.find((g) => g.subsystem === "type-challenges utility");
  assert.equal(tcGroup.count, 1);
  assert.deepEqual(tcGroup.codes, ["TS2322"]);
}

// The exported parseDiagnosticLine handles colon-style locations the
// aggregator's other tests don't otherwise drive; this guards both shapes
// of the canonical parser since callers (type-challenges hook etc.)
// depend on a single source of truth.
{
  assert.deepEqual(
    parseDiagnosticLine("src/a.ts:5:6 - error TS2322: bad."),
    { path: "src/a.ts", line: 5, column: 6, code: "TS2322" },
  );
  assert.deepEqual(
    parseDiagnosticLine("tsc: error TS18003: No inputs were found in config file."),
    { path: null, line: null, column: null, code: "TS18003" },
    "pathless global diagnostics must retain their code without inventing a location",
  );
  assert.equal(parseDiagnosticLine("noise without a location"), null);
}

// Pathless config diagnostics are still structured aggregator input: they
// retain source attribution, code/subsystem routing, and a null location.
{
  const agg = aggregateRowDeltas([
    "tsc: error TS18003: No inputs were found in config file.",
  ]);
  assert.deepEqual(agg.codes, ["TS18003"]);
  assert.deepEqual(agg.codesBySource.tsc, ["TS18003"]);
  assert.deepEqual(agg.firstLocation, {
    path: null,
    line: null,
    column: null,
    code: "TS18003",
  });
  assert.equal(agg.reductionCandidates.length, 1);
}

// Linear scaling property: doubling the row count + diagnostic count must
// roughly double the work, not quadruple it. Concretely: the number of
// per-line callback invocations the aggregator makes scales as O(R * D)
// not O((R * D)^2). We exercise this by counting how many times we touch
// each delta string via Proxy reads.
{
  function makeRows(rowCount, deltasPerRow) {
    return Array.from({ length: rowCount }, (_, r) => ({
      name: `row-${r}`,
      state: r % 3 === 0 ? "red" : r % 3 === 1 ? "yellow" : "green",
      oracle_classification: "tsz-fails-only",
      diagnostic_deltas: Array.from(
        { length: deltasPerRow },
        (_, d) => `src/r${r}d${d}.ts(${d + 1},1): error TS2322: m${d}`,
      ),
      diagnostic_subsystems: [],
    }));
  }

  function countDeltaReads(rows) {
    let reads = 0;
    const wrapped = rows.map((row) => ({
      ...row,
      diagnostic_deltas: new Proxy(row.diagnostic_deltas, {
        get(target, key) {
          if (typeof key === "string" && /^\d+$/.test(key)) reads++;
          return Reflect.get(target, key);
        },
      }),
    }));
    aggregateRowsForSummary(wrapped, { topDeltasLimit: 3 });
    return reads;
  }

  // Top deltas is capped at 3, so once we've found 3 we stop walking. The
  // important invariant is: number of delta reads <= 3 (it never iterates
  // beyond the limit, regardless of total row/delta counts).
  const small = countDeltaReads(makeRows(5, 4));
  const big = countDeltaReads(makeRows(50, 40));
  assert.ok(small <= 3, `small reads should be <= 3, got ${small}`);
  assert.ok(big <= 3, `big reads should be <= 3, got ${big}`);
}

console.log("test-diagnostic-aggregator: all tests passed");
