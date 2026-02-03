/**
 * TypeScript Harness Loader
 *
 * This module loads TypeScript's test harness from the built submodule.
 * It provides access to TestCaseParser and other harness utilities.
 *
 * Prerequisites:
 * - TypeScript submodule must be built: cd TypeScript && npm ci && npx hereby tests --no-bundle
 *
 * Usage:
 *   import { loadHarness } from './ts-harness-loader.js';
 *   const { Harness, ts } = loadHarness();
 *   const parsed = Harness.TestCaseParser.makeUnitsFromTest(code, fileName);
 */
/**
 * TypeScript's TestCaseParser types (from harnessIO.ts)
 */
export interface CompilerSettings {
    [name: string]: string;
}
export interface TestUnitData {
    content: string;
    name: string;
    fileOptions: CompilerSettings;
    originalFilePath: string;
    references: string[];
}
export interface TestCaseContent {
    settings: CompilerSettings;
    testUnitData: TestUnitData[];
    tsConfig: any;
    tsConfigFileUnitData: TestUnitData | undefined;
    symlinks?: any;
}
export interface TestCaseParser {
    extractCompilerSettings(content: string): CompilerSettings;
    makeUnitsFromTest(code: string, fileName: string, settings?: CompilerSettings): TestCaseContent;
    parseSymlinkFromTest(line: string, symlinks: any, absoluteRootDir?: string): any;
}
export interface HarnessModule {
    TestCaseParser: TestCaseParser;
    Compiler: any;
    Baseline: any;
    IO: any;
}
/**
 * Check if TypeScript's harness is built
 */
export declare function isHarnessBuilt(): boolean;
/**
 * Get the path to TypeScript's built harness
 */
export declare function getHarnessPath(): string;
/**
 * Get the path to TypeScript's built compiler
 */
export declare function getTsPath(): string;
/**
 * Load TypeScript's harness modules.
 * Throws if the harness is not built.
 */
export declare function loadHarness(): {
    Harness: HarnessModule;
    ts: typeof import('typescript');
};
export declare function getHarness(): {
    Harness: HarnessModule;
    ts: typeof import('typescript');
};
/**
 * Parse a test file using TypeScript's TestCaseParser.
 * This is a convenience wrapper around makeUnitsFromTest.
 */
export declare function parseTestFile(code: string, fileName: string): TestCaseContent;
/**
 * Extract compiler settings from test file content.
 * This is a convenience wrapper around extractCompilerSettings.
 */
export declare function extractSettings(content: string): CompilerSettings;
/**
 * Check if a setting name is a harness directive (not a compiler option).
 */
export declare function isHarnessDirective(name: string): boolean;
/**
 * Separate harness directives from compiler options.
 */
export declare function separateSettings(settings: CompilerSettings): {
    harness: CompilerSettings;
    compiler: CompilerSettings;
};
/**
 * Check if a test should be skipped based on harness settings.
 */
export declare function shouldSkipTest(settings: CompilerSettings, targetTsVersion?: string): {
    skip: boolean;
    reason?: string;
};
//# sourceMappingURL=ts-harness-loader.d.ts.map