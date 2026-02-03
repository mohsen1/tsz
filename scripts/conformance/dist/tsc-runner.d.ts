/**
 * Shared TSC runner for conformance testing.
 *
 * Extracted from cache-worker.ts for reuse in:
 * - cache-worker.ts (generates TSC cache)
 * - runner-server.ts (--print-test mode)
 */
export interface TestFile {
    name: string;
    content: string;
}
export interface ParsedTestCase {
    options: Record<string, unknown>;
    isMultiFile: boolean;
    files: TestFile[];
}
export interface DiagnosticInfo {
    code: number;
    message: string;
    file?: string;
    line?: number;
    column?: number;
}
export interface TscResult {
    codes: number[];
    diagnostics: DiagnosticInfo[];
}
export declare function parseLibReferences(source: string): string[];
export declare function resolveLibFilePath(libName: string, libDir: string): string | null;
export declare function readLibContent(libName: string, libDir: string): string | null;
export declare function collectLibFiles(libNames: string[], libDir: string): Map<string, string>;
export declare function parseTestDirectives(code: string, filePath: string): ParsedTestCase;
/**
 * Run TSC on a test case and return results.
 *
 * @param testCase Parsed test case with files and options
 * @param libDir Directory containing lib.*.d.ts files
 * @param libSource Optional fallback lib.d.ts content
 * @param includeMessages If true, include full diagnostic messages (slower)
 */
export declare function runTsc(testCase: ParsedTestCase, libDir: string, libSource?: string, includeMessages?: boolean, rootFilePath?: string): TscResult;
/**
 * Run TSC on files (map of filename -> content) and return results.
 * Convenience wrapper for use with CheckOptions-style input.
 */
export declare function runTscOnFiles(files: Record<string, string>, options: Record<string, unknown>, libDir: string, includeMessages?: boolean): TscResult;
//# sourceMappingURL=tsc-runner.d.ts.map