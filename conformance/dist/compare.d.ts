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
export declare function compareDiagnostics(tscResult: TestResult, wasmResult: TestResult, includeDetails?: boolean): DiagnosticComparison;
/**
 * Compare diagnostics by code frequency
 * Useful when order doesn't matter but counts do
 */
export declare function compareByCodeFrequency(tscResult: TestResult, wasmResult: TestResult): {
    exactMatch: boolean;
    tscCodeCounts: Map<number, number>;
    wasmCodeCounts: Map<number, number>;
    differences: Array<{
        code: number;
        tscCount: number;
        wasmCount: number;
    }>;
};
/**
 * Format comparison result as a human-readable string
 */
export declare function formatComparison(comparison: DiagnosticComparison): string;
/**
 * Format comparison result as JSON
 */
export declare function formatComparisonJson(comparison: DiagnosticComparison): string;
/**
 * Calculate pass rate from comparison results
 */
export declare function calculatePassRate(comparisons: DiagnosticComparison[]): {
    total: number;
    passed: number;
    exactMatch: number;
    passRate: number;
    exactMatchRate: number;
};
/**
 * Group comparisons by error code for analysis
 */
export declare function groupByErrorCode(comparisons: Array<{
    file: string;
    comparison: DiagnosticComparison;
}>): {
    missingCodes: Map<number, string[]>;
    extraCodes: Map<number, string[]>;
};
/**
 * Get the most impactful error codes (by frequency)
 */
export declare function getMostImpactfulCodes(comparisons: Array<{
    file: string;
    comparison: DiagnosticComparison;
}>, limit?: number): {
    missingCodes: Array<{
        code: number;
        count: number;
        files: string[];
    }>;
    extraCodes: Array<{
        code: number;
        count: number;
        files: string[];
    }>;
};
//# sourceMappingURL=compare.d.ts.map