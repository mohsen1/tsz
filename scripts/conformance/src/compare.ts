/**
 * Diagnostic Comparison Utilities
 *
 * Compare diagnostic output between TypeScript compiler and tsz (WASM)
 * to measure conformance accuracy.
 */

/**
 * A single diagnostic from either compiler
 */
export interface Diagnostic {
  code: number;
  message: string;
  category: string;
  file?: string;
  start?: number;
  length?: number;
}

/**
 * Result from running a compiler on test files
 */
export interface TestResult {
  diagnostics: Diagnostic[];
  crashed: boolean;
  error?: string;
}

/**
 * Result of comparing diagnostics between two compilers
 */
export interface DiagnosticComparison {
  /** Diagnostics match exactly (same codes in same order) */
  exactMatch: boolean;
  /** Same number of diagnostics (possibly different codes) */
  sameCount: boolean;
  /** Number of diagnostics from tsc */
  tscCount: number;
  /** Number of diagnostics from wasm */
  wasmCount: number;
  /** Error codes present in tsc but missing in wasm */
  missingInWasm: number[];
  /** Error codes present in wasm but not in tsc */
  extraInWasm: number[];
  /** Number of codes that match */
  matchingCodes: number;
  /** Detailed comparison of each diagnostic */
  details?: DiagnosticComparisonDetail[];
}

/**
 * Detailed comparison of a single diagnostic
 */
export interface DiagnosticComparisonDetail {
  code: number;
  inTsc: boolean;
  inWasm: boolean;
  tscMessage?: string;
  wasmMessage?: string;
  messageMatch?: boolean;
}

/**
 * Compare diagnostics from tsc and wasm compilers
 */
export function compareDiagnostics(
  tscResult: TestResult,
  wasmResult: TestResult,
  includeDetails = false
): DiagnosticComparison {
  const tscCodes = tscResult.diagnostics.map(d => d.code);
  const wasmCodes = wasmResult.diagnostics.map(d => d.code);

  const tscCodeSet = new Set(tscCodes);
  const wasmCodeSet = new Set(wasmCodes);

  const missingInWasm = [...tscCodeSet].filter(c => !wasmCodeSet.has(c));
  const extraInWasm = [...wasmCodeSet].filter(c => !tscCodeSet.has(c));

  // Count matching codes (not necessarily same position)
  const matchingCodes = [...tscCodeSet].filter(c => wasmCodeSet.has(c)).length;

  // Check for exact match (same codes in same positions)
  const exactMatch =
    tscCodes.length === wasmCodes.length &&
    tscCodes.every((code, i) => wasmCodes[i] === code);

  const sameCount = tscResult.diagnostics.length === wasmResult.diagnostics.length;

  const result: DiagnosticComparison = {
    exactMatch,
    sameCount,
    tscCount: tscResult.diagnostics.length,
    wasmCount: wasmResult.diagnostics.length,
    missingInWasm,
    extraInWasm,
    matchingCodes,
  };

  if (includeDetails) {
    result.details = generateComparisonDetails(tscResult, wasmResult);
  }

  return result;
}

/**
 * Generate detailed comparison for each diagnostic
 */
function generateComparisonDetails(
  tscResult: TestResult,
  wasmResult: TestResult
): DiagnosticComparisonDetail[] {
  const details: DiagnosticComparisonDetail[] = [];
  const allCodes = new Set([
    ...tscResult.diagnostics.map(d => d.code),
    ...wasmResult.diagnostics.map(d => d.code),
  ]);

  for (const code of allCodes) {
    const tscDiag = tscResult.diagnostics.find(d => d.code === code);
    const wasmDiag = wasmResult.diagnostics.find(d => d.code === code);

    const detail: DiagnosticComparisonDetail = {
      code,
      inTsc: !!tscDiag,
      inWasm: !!wasmDiag,
    };

    if (tscDiag) {
      detail.tscMessage = tscDiag.message;
    }
    if (wasmDiag) {
      detail.wasmMessage = wasmDiag.message;
    }
    if (tscDiag && wasmDiag) {
      detail.messageMatch = tscDiag.message === wasmDiag.message;
    }

    details.push(detail);
  }

  return details;
}

/**
 * Compare diagnostics by code frequency
 * Useful when order doesn't matter but counts do
 */
export function compareByCodeFrequency(
  tscResult: TestResult,
  wasmResult: TestResult
): {
  exactMatch: boolean;
  tscCodeCounts: Map<number, number>;
  wasmCodeCounts: Map<number, number>;
  differences: Array<{ code: number; tscCount: number; wasmCount: number }>;
} {
  const tscCodeCounts = new Map<number, number>();
  const wasmCodeCounts = new Map<number, number>();

  for (const d of tscResult.diagnostics) {
    tscCodeCounts.set(d.code, (tscCodeCounts.get(d.code) || 0) + 1);
  }

  for (const d of wasmResult.diagnostics) {
    wasmCodeCounts.set(d.code, (wasmCodeCounts.get(d.code) || 0) + 1);
  }

  const allCodes = new Set([...tscCodeCounts.keys(), ...wasmCodeCounts.keys()]);
  const differences: Array<{ code: number; tscCount: number; wasmCount: number }> = [];

  let exactMatch = true;
  for (const code of allCodes) {
    const tscCount = tscCodeCounts.get(code) || 0;
    const wasmCount = wasmCodeCounts.get(code) || 0;
    if (tscCount !== wasmCount) {
      exactMatch = false;
      differences.push({ code, tscCount, wasmCount });
    }
  }

  return {
    exactMatch,
    tscCodeCounts,
    wasmCodeCounts,
    differences,
  };
}

/**
 * Format comparison result as a human-readable string
 */
export function formatComparison(comparison: DiagnosticComparison): string {
  if (comparison.exactMatch) {
    return `EXACT MATCH (${comparison.tscCount} diagnostics)`;
  }

  const parts: string[] = [];

  if (comparison.sameCount) {
    parts.push(`same count (${comparison.tscCount})`);
  } else {
    parts.push(`tsc: ${comparison.tscCount}, wasm: ${comparison.wasmCount}`);
  }

  if (comparison.missingInWasm.length > 0) {
    const missing = comparison.missingInWasm.slice(0, 5).map(c => `TS${c}`).join(', ');
    const more = comparison.missingInWasm.length > 5
      ? ` +${comparison.missingInWasm.length - 5} more`
      : '';
    parts.push(`missing: ${missing}${more}`);
  }

  if (comparison.extraInWasm.length > 0) {
    const extra = comparison.extraInWasm.slice(0, 5).map(c => `TS${c}`).join(', ');
    const more = comparison.extraInWasm.length > 5
      ? ` +${comparison.extraInWasm.length - 5} more`
      : '';
    parts.push(`extra: ${extra}${more}`);
  }

  return parts.join(' | ');
}

/**
 * Format comparison result as JSON
 */
export function formatComparisonJson(comparison: DiagnosticComparison): string {
  return JSON.stringify(comparison, null, 2);
}

/**
 * Calculate pass rate from comparison results
 */
export function calculatePassRate(comparisons: DiagnosticComparison[]): {
  total: number;
  passed: number;
  exactMatch: number;
  passRate: number;
  exactMatchRate: number;
} {
  const total = comparisons.length;
  const passed = comparisons.filter(c => c.exactMatch || c.sameCount).length;
  const exactMatch = comparisons.filter(c => c.exactMatch).length;

  return {
    total,
    passed,
    exactMatch,
    passRate: total > 0 ? (passed / total) * 100 : 0,
    exactMatchRate: total > 0 ? (exactMatch / total) * 100 : 0,
  };
}

/**
 * Group comparisons by error code for analysis
 */
export function groupByErrorCode(comparisons: Array<{ file: string; comparison: DiagnosticComparison }>): {
  missingCodes: Map<number, string[]>;
  extraCodes: Map<number, string[]>;
} {
  const missingCodes = new Map<number, string[]>();
  const extraCodes = new Map<number, string[]>();

  for (const { file, comparison } of comparisons) {
    for (const code of comparison.missingInWasm) {
      if (!missingCodes.has(code)) {
        missingCodes.set(code, []);
      }
      missingCodes.get(code)!.push(file);
    }

    for (const code of comparison.extraInWasm) {
      if (!extraCodes.has(code)) {
        extraCodes.set(code, []);
      }
      extraCodes.get(code)!.push(file);
    }
  }

  return { missingCodes, extraCodes };
}

/**
 * Get the most impactful error codes (by frequency)
 */
export function getMostImpactfulCodes(
  comparisons: Array<{ file: string; comparison: DiagnosticComparison }>,
  limit = 10
): {
  missingCodes: Array<{ code: number; count: number; files: string[] }>;
  extraCodes: Array<{ code: number; count: number; files: string[] }>;
} {
  const { missingCodes, extraCodes } = groupByErrorCode(comparisons);

  const sortByCount = (
    codeMap: Map<number, string[]>
  ): Array<{ code: number; count: number; files: string[] }> => {
    return [...codeMap.entries()]
      .map(([code, files]) => ({ code, count: files.length, files }))
      .sort((a, b) => b.count - a.count)
      .slice(0, limit);
  };

  return {
    missingCodes: sortByCount(missingCodes),
    extraCodes: sortByCount(extraCodes),
  };
}
