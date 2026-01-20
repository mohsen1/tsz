/**
 * Baseline File Loading and Comparison
 *
 * Loads and parses TypeScript baseline files (.errors.txt, .types, .symbols)
 * for comparison with our compiler's output.
 */
/**
 * A parsed error from a .errors.txt baseline file
 */
export interface BaselineError {
    file: string;
    line: number;
    column: number;
    code: number;
    message: string;
}
/**
 * Result of loading a baseline
 */
export interface BaselineResult {
    exists: boolean;
    errors: BaselineError[];
    raw?: string;
}
/**
 * Comparison between actual and expected errors
 */
export interface BaselineComparison {
    hasBaseline: boolean;
    expectedErrors: BaselineError[];
    actualCodes: number[];
    exactMatch: boolean;
    matchingCodes: number[];
    missingCodes: number[];
    extraCodes: number[];
    matchRate: number;
}
/**
 * Parse errors from a .errors.txt baseline file
 */
export declare function parseErrorsBaseline(content: string): BaselineError[];
/**
 * Get the baseline path for a test file
 */
export declare function getBaselinePath(testPath: string, testsBasePath: string, baselineType?: 'errors.txt' | 'types' | 'symbols' | 'js' | 'd.ts'): string;
/**
 * Load baseline for a test file
 */
export declare function loadBaseline(testPath: string, testsBasePath: string): BaselineResult;
/**
 * Compare actual diagnostic codes with baseline
 */
export declare function compareWithBaseline(actualCodes: number[], baseline: BaselineResult): BaselineComparison;
/**
 * Format baseline comparison for display
 */
export declare function formatBaselineComparison(comparison: BaselineComparison): string;
//# sourceMappingURL=baseline.d.ts.map