/**
 * Shared test utilities for conformance testing.
 *
 * This module provides common functionality used by both server-mode
 * and WASM-mode conformance runners to ensure consistent behavior.
 *
 * Now uses TypeScript's own TestCaseParser from the built harness
 * to ensure exact compatibility with tsc test parsing.
 *
 * Handles TSC test directives including:
 * - Compiler options (@target, @strict, @lib, etc.)
 * - Test harness-specific directives (@filename, @noCheck, @typeScriptVersion, etc.)
 *
 * See docs/specs/TSC_DIRECTIVES.md for the full reference.
 */
/**
 * Test harness-specific options that control HOW the test runs,
 * NOT passed to the compiler as options.
 */
export interface HarnessOptions {
    filename?: string;
    allowNonTsExtensions?: boolean;
    useCaseSensitiveFileNames?: boolean;
    noCheck?: boolean;
    typeScriptVersion?: string;
    baselineFile?: string;
    noErrorTruncation?: boolean;
    suppressOutputPathCheck?: boolean;
    noTypesAndSymbols?: boolean;
    fullEmitPaths?: boolean;
    reportDiagnostics?: boolean;
    captureSuggestions?: boolean;
    noImplicitReferences?: boolean;
    currentDirectory?: string;
    symlinks?: Array<{
        target: string;
        link: string;
    }>;
}
/**
 * Compiler options parsed from test directives.
 * All keys are lowercase for case-insensitive matching.
 */
export interface ParsedDirectives {
    target?: string;
    lib?: string[];
    nolib?: boolean;
    jsx?: string;
    jsxfactory?: string;
    jsxfragmentfactory?: string;
    jsximportsource?: string;
    usedefineforclassfields?: boolean;
    experimentaldecorators?: boolean;
    emitdecoratormetadata?: boolean;
    moduledetection?: string;
    strict?: boolean;
    strictnullchecks?: boolean;
    strictfunctiontypes?: boolean;
    strictbindcallapply?: boolean;
    strictpropertyinitialization?: boolean;
    strictbuiltiniteratorreturn?: boolean;
    noimplicitany?: boolean;
    noimplicitthis?: boolean;
    useunknownincatchvariables?: boolean;
    alwaysstrict?: boolean;
    nounusedlocals?: boolean;
    nounusedparameters?: boolean;
    exactoptionalpropertytypes?: boolean;
    noimplicitreturns?: boolean;
    nofallthroughcasesinswitch?: boolean;
    nouncheckedindexedaccess?: boolean;
    noimplicitoverride?: boolean;
    nopropertyaccessfromindexsignature?: boolean;
    allowunusedlabels?: boolean;
    allowunreachablecode?: boolean;
    module?: string;
    moduleresolution?: string;
    baseurl?: string;
    paths?: Record<string, string[]>;
    rootdirs?: string[];
    typeroots?: string[];
    types?: string[];
    allowsyntheticdefaultimports?: boolean;
    esmoduleinterop?: boolean;
    preservesymlinks?: boolean;
    allowumdglobalaccess?: boolean;
    modulesuffixes?: string[];
    allowimportingtsextensions?: boolean;
    rewriterelativeimportextensions?: boolean;
    resolvepackagejsonexports?: boolean;
    resolvepackagejsonimports?: boolean;
    customconditions?: string[];
    nouncheckedsideeffectimports?: boolean;
    resolvejsonmodule?: boolean;
    allowarbitraryextensions?: boolean;
    noresolve?: boolean;
    allowjs?: boolean;
    checkjs?: boolean;
    maxnodemodulejsdepth?: number;
    noemit?: boolean;
    declaration?: boolean;
    declarationmap?: boolean;
    emitdeclarationonly?: boolean;
    sourcemap?: boolean;
    inlinesourcemap?: boolean;
    inlinesources?: boolean;
    outfile?: string;
    outdir?: string;
    rootdir?: string;
    declarationdir?: string;
    removecomments?: boolean;
    importhelpers?: boolean;
    downleveliteration?: boolean;
    preserveconstenums?: boolean;
    stripinternal?: boolean;
    noemithelpers?: boolean;
    noemitonerror?: boolean;
    emitbom?: boolean;
    newline?: string;
    sourceroot?: string;
    maproot?: string;
    incremental?: boolean;
    composite?: boolean;
    tsbuildinfofile?: string;
    disablesourceofprojectreferenceredirect?: boolean;
    disablesolutionsearching?: boolean;
    disablereferencedprojectload?: boolean;
    isolatedmodules?: boolean;
    verbatimmodulesyntax?: boolean;
    isolateddeclarations?: boolean;
    erasablesyntaxonly?: boolean;
    forceconsistentcasinginfilenames?: boolean;
    skiplibcheck?: boolean;
    skipdefaultlibcheck?: boolean;
    libreplacement?: boolean;
    disablesizelimit?: boolean;
    suppressexcesspropertyerrors?: boolean;
    suppressimplicitanyindexerrors?: boolean;
    noimplicitusestrict?: boolean;
    nostrictgenericchecks?: boolean;
    keyofstringsonly?: boolean;
    preservevalueimports?: boolean;
    importsnotusedasvalues?: string;
    charset?: string;
    out?: string;
    reactnamespace?: string;
    [key: string]: unknown;
}
export interface TestFile {
    name: string;
    content: string;
}
export interface ParsedTestCase {
    directives: ParsedDirectives;
    harness: HarnessOptions;
    isMultiFile: boolean;
    files: TestFile[];
    category: string;
}
/**
 * Parse test directives from TypeScript conformance test file.
 * Uses TypeScript's own TestCaseParser when the harness is built,
 * falling back to the legacy parser when it's not.
 *
 * Extracts @target, @lib, @strict, etc. from comment headers.
 * Also handles @filename directives for multi-file tests.
 *
 * Separates harness-specific directives from compiler options:
 * - Harness directives control the test environment (e.g., @noCheck, @typeScriptVersion)
 * - Compiler options are passed to tsz (e.g., @strict, @target, @lib)
 */
export declare function parseTestCase(code: string, filePath: string): ParsedTestCase;
/**
 * Parse just the directives (simpler version for server mode).
 * Returns both compiler directives and harness options.
 * Uses TypeScript's extractCompilerSettings when harness is built.
 */
export declare function parseDirectivesOnly(content: string): {
    directives: ParsedDirectives;
    harness: HarnessOptions;
};
/**
 * Current TypeScript version that tsz targets for compatibility.
 * Loaded from typescript-versions.json at startup.
 */
export declare const TSZ_TARGET_TS_VERSION: string;
/**
 * Check if a test should be skipped based on @typeScriptVersion directive.
 * Returns true if the test requires a newer TS version than we support.
 *
 * Uses semver for robust version comparison.
 */
export declare function shouldSkipForVersion(harness: HarnessOptions): boolean;
/**
 * Check if a test should be skipped based on harness options.
 * Returns { skip: boolean, reason?: string }
 */
export declare function shouldSkipTest(harness: HarnessOptions): {
    skip: boolean;
    reason?: string;
};
export interface CheckOptions {
    target?: string;
    lib?: string[];
    noLib?: boolean;
    strict?: boolean;
    strictNullChecks?: boolean;
    strictFunctionTypes?: boolean;
    strictPropertyInitialization?: boolean;
    strictBindCallApply?: boolean;
    noImplicitAny?: boolean;
    noImplicitThis?: boolean;
    noImplicitReturns?: boolean;
    noImplicitOverride?: boolean;
    useUnknownInCatchVariables?: boolean;
    alwaysStrict?: boolean;
    module?: string;
    moduleResolution?: string;
    moduleDetection?: string;
    jsx?: string;
    jsxFactory?: string;
    jsxFragmentFactory?: string;
    jsxImportSource?: string;
    allowJs?: boolean;
    checkJs?: boolean;
    declaration?: boolean;
    declarationMap?: boolean;
    emitDeclarationOnly?: boolean;
    declarationDir?: string;
    isolatedModules?: boolean;
    experimentalDecorators?: boolean;
    emitDecoratorMetadata?: boolean;
    useDefineForClassFields?: boolean;
    baseUrl?: string;
    paths?: Record<string, string[]>;
    rootDirs?: string[];
    typeRoots?: string[];
    types?: string[];
    resolveJsonModule?: boolean;
    esModuleInterop?: boolean;
    allowSyntheticDefaultImports?: boolean;
    preserveSymlinks?: boolean;
    allowUmdGlobalAccess?: boolean;
    verbatimModuleSyntax?: boolean;
    noPropertyAccessFromIndexSignature?: boolean;
    noUncheckedIndexedAccess?: boolean;
    exactOptionalPropertyTypes?: boolean;
    noFallthroughCasesInSwitch?: boolean;
    noUnusedLocals?: boolean;
    noUnusedParameters?: boolean;
    allowUnusedLabels?: boolean;
    allowUnreachableCode?: boolean;
    forceConsistentCasingInFileNames?: boolean;
    skipLibCheck?: boolean;
    noResolve?: boolean;
    noEmit?: boolean;
    outFile?: string;
    outDir?: string;
    rootDir?: string;
    importHelpers?: boolean;
    downlevelIteration?: boolean;
    [key: string]: unknown;
}
/**
 * Convert parsed directives to CheckOptions for tsz-server.
 * Just passes through the directives - tsz handles lib loading.
 */
export declare function directivesToCheckOptions(directives: ParsedDirectives, _libDirs?: string[]): CheckOptions;
/**
 * Get lib names for a test case.
 * Just parses the @lib directive - doesn't resolve dependencies.
 */
export declare function getLibNamesForDirectives(directives: ParsedDirectives, _libDirs?: string[]): string[];
/**
 * Parse lib option value (string, array, or unknown) into array of lib names.
 * Shared utility for test runners.
 */
export declare function parseLibOption(libOpt: unknown): string[];
//# sourceMappingURL=test-utils.d.ts.map