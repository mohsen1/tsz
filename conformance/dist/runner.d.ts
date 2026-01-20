#!/usr/bin/env node
/**
 * Parallel Conformance Test Runner
 *
 * Runs each test in a separate child process with timeout.
 * This ensures hanging tests can be killed without affecting others.
 */
interface RunnerConfig {
    wasmPkgPath: string;
    testsBasePath: string;
    libPath: string;
    maxTests: number;
    verbose: boolean;
    categories: string[];
    workers: number;
    testTimeout: number;
}
interface TestStats {
    total: number;
    passed: number;
    failed: number;
    crashed: number;
    skipped: number;
    timedOut: number;
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