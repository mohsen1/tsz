#!/usr/bin/env node
/**
 * Unified Conformance Test Runner
 *
 * Supports running tests from:
 * - compiler/ tests
 * - conformance/ tests
 * - projects/ tests
 *
 * Compares output with TypeScript (tsc) and tracks pass rates.
 */
interface RunnerConfig {
    wasmPkgPath: string;
    testsBasePath: string;
    libPath: string;
    maxTests: number;
    verbose: boolean;
    categories: string[];
    timeout: number;
}
interface TestStats {
    total: number;
    passed: number;
    failed: number;
    crashed: number;
    skipped: number;
    exactMatch: number;
    sameErrorCount: number;
    missingErrors: number;
    extraErrors: number;
    byCategory: Record<string, CategoryStats>;
    byErrorCode: Record<number, ErrorCodeStats>;
}
interface CategoryStats {
    total: number;
    passed: number;
    failed: number;
    exactMatch: number;
}
interface ErrorCodeStats {
    missingCount: number;
    extraCount: number;
    testFiles: string[];
}
/**
 * Main entry point
 */
export declare function runConformanceTests(config?: Partial<RunnerConfig>): Promise<TestStats>;
export {};
//# sourceMappingURL=runner.d.ts.map