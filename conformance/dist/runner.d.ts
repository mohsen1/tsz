#!/usr/bin/env node
/**
 * Parallel Conformance Test Runner
 *
 * Runs TypeScript conformance tests using worker threads for high parallelism.
 */
interface RunnerConfig {
    wasmPkgPath: string;
    testsBasePath: string;
    libPath: string;
    maxTests: number;
    verbose: boolean;
    categories: string[];
    workers: number;
}
interface TestStats {
    total: number;
    passed: number;
    failed: number;
    crashed: number;
    skipped: number;
    byCategory: Record<string, {
        total: number;
        passed: number;
    }>;
    missingCodes: Map<number, number>;
    extraCodes: Map<number, number>;
}
export declare function runConformanceTests(config?: Partial<RunnerConfig>): Promise<TestStats>;
export {};
//# sourceMappingURL=runner.d.ts.map