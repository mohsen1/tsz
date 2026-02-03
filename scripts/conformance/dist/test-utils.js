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
import * as path from 'path';
import * as fs from 'fs';
import * as semver from 'semver';
import { fileURLToPath } from 'url';
import { getFullLibNameForTarget } from './lib-manifest.js';
import { isHarnessBuilt, loadHarness, separateSettings as separateHarnessSettings, } from './ts-harness-loader.js';
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
/** List of harness-specific directive names (lowercase) */
const HARNESS_DIRECTIVES = new Set([
    'filename',
    'allownontextensions',
    'allownonttsextensions', // alternate spelling
    'usecasesensitivefilenames',
    'nocheck',
    'typescriptversion',
    'baselinefile',
    'noerrortruncation',
    'suppressoutputpathcheck',
    'notypesandsymbols',
    'fullemitpaths',
    'reportdiagnostics',
    'capturesuggestions',
    'noimplicitreferences',
    'currentdirectory',
    'symlink',
    'link',
]);
/**
 * Parse a @link or @symlink directive value.
 * Format: "target -> link" or "target=>link"
 */
function parseSymlinkDirective(value) {
    // Try -> first, then =>
    let parts = value.split('->').map(s => s.trim());
    if (parts.length !== 2) {
        parts = value.split('=>').map(s => s.trim());
    }
    if (parts.length === 2 && parts[0] && parts[1]) {
        return { target: parts[0], link: parts[1] };
    }
    return null;
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
export function parseTestCase(code, filePath) {
    // Try to use TypeScript's own TestCaseParser
    if (isHarnessBuilt()) {
        return parseTestCaseWithHarness(code, filePath);
    }
    // Fall back to legacy parser
    return parseTestCaseLegacy(code, filePath);
}
/**
 * Parse test case using TypeScript's TestCaseParser from the built harness.
 * This ensures exact compatibility with how tsc parses test files.
 */
function parseTestCaseWithHarness(code, filePath) {
    const { Harness } = loadHarness();
    // Use TypeScript's parser
    const parsed = Harness.TestCaseParser.makeUnitsFromTest(code, filePath);
    // Convert to our format
    const files = parsed.testUnitData.map((unit) => ({
        name: unit.name,
        content: unit.content ?? '', // Ensure content is always a string (handle undefined from duplicate @filename)
    }));
    // Separate harness directives from compiler options
    const { harness: harnessSettings, compiler: compilerSettings } = separateHarnessSettings(parsed.settings);
    // Convert settings to our ParsedDirectives format
    const directives = {};
    for (const [key, value] of Object.entries(compilerSettings)) {
        const lowKey = key.toLowerCase();
        // Parse value types
        if (value.toLowerCase() === 'true')
            directives[lowKey] = true;
        else if (value.toLowerCase() === 'false')
            directives[lowKey] = false;
        else if (!isNaN(Number(value)) && lowKey !== 'typescriptversion')
            directives[lowKey] = Number(value);
        else
            directives[lowKey] = value;
    }
    // Convert harness settings to HarnessOptions
    const harness = {};
    for (const [key, value] of Object.entries(harnessSettings)) {
        const lowKey = key.toLowerCase();
        const parsedValue = value.toLowerCase() === 'true' ? true :
            value.toLowerCase() === 'false' ? false : value;
        switch (lowKey) {
            case 'nocheck':
                harness.noCheck = parsedValue;
                break;
            case 'typescriptversion':
                harness.typeScriptVersion = parsedValue;
                break;
            case 'currentdirectory':
                harness.currentDirectory = parsedValue;
                break;
            case 'allownontextensions':
            case 'allownonttsextensions':
                harness.allowNonTsExtensions = parsedValue;
                break;
            case 'usecasesensitivefilenames':
                harness.useCaseSensitiveFileNames = parsedValue;
                break;
            case 'noimplicitreferences':
                harness.noImplicitReferences = parsedValue;
                break;
            case 'baselinefile':
                harness.baselineFile = parsedValue;
                break;
            case 'noerrortruncation':
                harness.noErrorTruncation = parsedValue;
                break;
            case 'suppressoutputpathcheck':
                harness.suppressOutputPathCheck = parsedValue;
                break;
            case 'notypesandsymbols':
                harness.noTypesAndSymbols = parsedValue;
                break;
            case 'fullemitpaths':
                harness.fullEmitPaths = parsedValue;
                break;
            case 'reportdiagnostics':
                harness.reportDiagnostics = parsedValue;
                break;
            case 'capturesuggestions':
                harness.captureSuggestions = parsedValue;
                break;
        }
    }
    // Handle symlinks from parsed content
    if (parsed.symlinks && parsed.symlinks.length > 0) {
        harness.symlinks = parsed.symlinks.map((s) => ({
            target: s.target,
            link: s.link,
        }));
    }
    const isMultiFile = files.length > 1 || parsed.testUnitData.some((u) => u.fileOptions && Object.keys(u.fileOptions).some(k => k.toLowerCase() === 'filename'));
    // Extract category from path
    const relativePath = filePath.replace(/.*tests\/cases\//, '');
    const category = relativePath.split(path.sep)[0] || 'unknown';
    return { directives, harness, isMultiFile, files, category };
}
/**
 * Legacy parser - used when TypeScript harness is not built.
 * This is the original implementation kept for backwards compatibility.
 */
function parseTestCaseLegacy(code, filePath) {
    const lines = code.split('\n');
    const directives = {};
    const harness = {};
    let isMultiFile = false;
    const files = [];
    let currentFileName = null;
    let currentFileLines = [];
    const cleanLines = [];
    for (const line of lines) {
        const trimmed = line.trim();
        // Handle @filename directive for multi-file tests
        const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/i);
        if (filenameMatch) {
            isMultiFile = true;
            if (currentFileName) {
                files.push({ name: currentFileName, content: currentFileLines.join('\n') });
            }
            currentFileName = filenameMatch[1].trim();
            currentFileLines = [];
            continue;
        }
        // Handle @link / @symlink directives specially (format: target -> link)
        const symlinkMatch = trimmed.match(/^\/\/\s*@(link|symlink):\s*(.+)$/i);
        if (symlinkMatch) {
            const parsed = parseSymlinkDirective(symlinkMatch[2]);
            if (parsed) {
                if (!harness.symlinks)
                    harness.symlinks = [];
                harness.symlinks.push(parsed);
            }
            continue;
        }
        // Handle other @option directives
        const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
        if (optionMatch) {
            const [, key, value] = optionMatch;
            const lowKey = key.toLowerCase();
            // Parse value - keep version-like strings as strings (e.g., "6.0", "5.5")
            let parsedValue;
            if (value.toLowerCase() === 'true')
                parsedValue = true;
            else if (value.toLowerCase() === 'false')
                parsedValue = false;
            else if (lowKey === 'typescriptversion') {
                // Keep TypeScript version as string to preserve minor version (e.g., "6.0")
                parsedValue = value;
            }
            else if (!isNaN(Number(value)))
                parsedValue = Number(value);
            else
                parsedValue = value;
            // Route to harness or directives based on directive type
            if (HARNESS_DIRECTIVES.has(lowKey)) {
                // Handle harness-specific directives
                switch (lowKey) {
                    case 'nocheck':
                        harness.noCheck = parsedValue;
                        break;
                    case 'typescriptversion':
                        harness.typeScriptVersion = parsedValue;
                        break;
                    case 'currentdirectory':
                        harness.currentDirectory = parsedValue;
                        break;
                    case 'allownontextensions':
                    case 'allownonttsextensions':
                        harness.allowNonTsExtensions = parsedValue;
                        break;
                    case 'usecasesensitivefilenames':
                        harness.useCaseSensitiveFileNames = parsedValue;
                        break;
                    case 'noimplicitreferences':
                        harness.noImplicitReferences = parsedValue;
                        break;
                    case 'baselinefile':
                        harness.baselineFile = parsedValue;
                        break;
                    case 'noerrortruncation':
                        harness.noErrorTruncation = parsedValue;
                        break;
                    case 'suppressoutputpathcheck':
                        harness.suppressOutputPathCheck = parsedValue;
                        break;
                    case 'notypesandsymbols':
                        harness.noTypesAndSymbols = parsedValue;
                        break;
                    case 'fullemitpaths':
                        harness.fullEmitPaths = parsedValue;
                        break;
                    case 'reportdiagnostics':
                        harness.reportDiagnostics = parsedValue;
                        break;
                    case 'capturesuggestions':
                        harness.captureSuggestions = parsedValue;
                        break;
                }
            }
            else {
                // Compiler option
                directives[lowKey] = parsedValue;
            }
            continue;
        }
        if (isMultiFile && currentFileName) {
            currentFileLines.push(line);
        }
        else {
            cleanLines.push(line);
        }
    }
    if (isMultiFile && currentFileName) {
        files.push({ name: currentFileName, content: currentFileLines.join('\n') });
    }
    // For multi-file tests, also include the main test file if it's not already included
    // This is needed so TypeScript can use the main file path as the root for resolution
    if (isMultiFile) {
        const mainFileName = path.basename(filePath);
        if (!files.some(f => f.name === mainFileName)) {
            files.push({ name: mainFileName, content: cleanLines.join('\n') });
        }
    }
    else {
        files.push({ name: path.basename(filePath), content: cleanLines.join('\n') });
    }
    // Extract category from path
    const relativePath = filePath.replace(/.*tests\/cases\//, '');
    const category = relativePath.split(path.sep)[0] || 'unknown';
    return { directives, harness, isMultiFile, files, category };
}
/**
 * Parse just the directives (simpler version for server mode).
 * Returns both compiler directives and harness options.
 * Uses TypeScript's extractCompilerSettings when harness is built.
 */
export function parseDirectivesOnly(content) {
    // Try to use TypeScript's harness
    if (isHarnessBuilt()) {
        return parseDirectivesWithHarness(content);
    }
    // Fall back to legacy parser
    return parseDirectivesOnlyLegacy(content);
}
/**
 * Parse directives using TypeScript's harness.
 */
function parseDirectivesWithHarness(content) {
    const { Harness } = loadHarness();
    // Use TypeScript's parser
    const settings = Harness.TestCaseParser.extractCompilerSettings(content);
    // Separate harness directives from compiler options
    const { harness: harnessSettings, compiler: compilerSettings } = separateHarnessSettings(settings);
    // Convert settings to our ParsedDirectives format
    const directives = {};
    for (const [key, value] of Object.entries(compilerSettings)) {
        const lowKey = key.toLowerCase();
        if (value.toLowerCase() === 'true')
            directives[lowKey] = true;
        else if (value.toLowerCase() === 'false')
            directives[lowKey] = false;
        else if (!isNaN(Number(value)) && lowKey !== 'typescriptversion')
            directives[lowKey] = Number(value);
        else
            directives[lowKey] = value;
    }
    // Convert harness settings to HarnessOptions
    const harness = {};
    for (const [key, value] of Object.entries(harnessSettings)) {
        const lowKey = key.toLowerCase();
        const parsedValue = value.toLowerCase() === 'true' ? true :
            value.toLowerCase() === 'false' ? false : value;
        switch (lowKey) {
            case 'nocheck':
                harness.noCheck = parsedValue;
                break;
            case 'typescriptversion':
                harness.typeScriptVersion = parsedValue;
                break;
            case 'currentdirectory':
                harness.currentDirectory = parsedValue;
                break;
            case 'allownontextensions':
            case 'allownonttsextensions':
                harness.allowNonTsExtensions = parsedValue;
                break;
            case 'usecasesensitivefilenames':
                harness.useCaseSensitiveFileNames = parsedValue;
                break;
            case 'noimplicitreferences':
                harness.noImplicitReferences = parsedValue;
                break;
            default: harness[lowKey] = parsedValue;
        }
    }
    return { directives, harness };
}
/**
 * Legacy directive parser.
 */
function parseDirectivesOnlyLegacy(content) {
    const directives = {};
    const harness = {};
    const lines = content.split('\n');
    for (const line of lines) {
        const trimmed = line.trim();
        // Stop parsing when we hit non-directive content
        if (!trimmed.startsWith('//') && trimmed.length > 0) {
            break;
        }
        // Handle @link / @symlink directives specially
        const symlinkMatch = trimmed.match(/^\/\/\s*@(link|symlink):\s*(.+)$/i);
        if (symlinkMatch) {
            const parsed = parseSymlinkDirective(symlinkMatch[2]);
            if (parsed) {
                if (!harness.symlinks)
                    harness.symlinks = [];
                harness.symlinks.push(parsed);
            }
            continue;
        }
        const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
        if (optionMatch) {
            const [, key, value] = optionMatch;
            const lowKey = key.toLowerCase();
            // Parse value
            let parsedValue;
            if (value.toLowerCase() === 'true')
                parsedValue = true;
            else if (value.toLowerCase() === 'false')
                parsedValue = false;
            else if (!isNaN(Number(value)))
                parsedValue = Number(value);
            else
                parsedValue = value;
            // Route to harness or directives
            if (HARNESS_DIRECTIVES.has(lowKey)) {
                switch (lowKey) {
                    case 'nocheck':
                        harness.noCheck = parsedValue;
                        break;
                    case 'typescriptversion':
                        harness.typeScriptVersion = parsedValue;
                        break;
                    case 'currentdirectory':
                        harness.currentDirectory = parsedValue;
                        break;
                    case 'allownontextensions':
                    case 'allownonttsextensions':
                        harness.allowNonTsExtensions = parsedValue;
                        break;
                    case 'usecasesensitivefilenames':
                        harness.useCaseSensitiveFileNames = parsedValue;
                        break;
                    case 'noimplicitreferences':
                        harness.noImplicitReferences = parsedValue;
                        break;
                    default:
                        // Other harness directives (baseline-related) - store generically
                        harness[lowKey] = parsedValue;
                }
            }
            else {
                directives[lowKey] = parsedValue;
            }
        }
    }
    return { directives, harness };
}
// ============================================================================
// Test Skip/Filter Logic
// ============================================================================
/**
 * Load the TypeScript version that tsz targets from typescript-versions.json.
 * Used to skip tests requiring newer TS features.
 */
function loadTargetTsVersion() {
    try {
        const versionsPath = path.resolve(__dirname, '../typescript-versions.json');
        const content = fs.readFileSync(versionsPath, 'utf-8');
        const versions = JSON.parse(content);
        // Try to get the current mapping version, fall back to default
        // The mappings contain the actual TS version we're testing against
        const mappings = versions.mappings || {};
        const mappingKeys = Object.keys(mappings);
        if (mappingKeys.length > 0) {
            // Use the first (most recent) mapping's npm version
            const latestMapping = mappings[mappingKeys[0]];
            const npmVersion = latestMapping?.npm;
            if (npmVersion) {
                // Coerce to valid semver (handles "6.0.0-dev.20260116" -> "6.0.0")
                const coerced = semver.coerce(npmVersion);
                if (coerced)
                    return coerced.version;
            }
        }
        // Fall back to default version
        const defaultNpm = versions.default?.npm;
        if (defaultNpm) {
            const coerced = semver.coerce(defaultNpm);
            if (coerced)
                return coerced.version;
        }
    }
    catch {
        // If we can't read the file, fall back to a safe default
    }
    // Ultimate fallback
    return '5.5.0';
}
/**
 * Current TypeScript version that tsz targets for compatibility.
 * Loaded from typescript-versions.json at startup.
 */
export const TSZ_TARGET_TS_VERSION = loadTargetTsVersion();
/**
 * Coerce a version string to a valid semver format.
 * Handles TypeScript's version formats like "5.5", "5", ">=5.0"
 */
function coerceVersion(versionStr) {
    // Extract operator and version parts
    const match = versionStr.match(/^(>=|>|<=|<|=|~|\^)?\s*(.+)$/);
    if (!match)
        return { range: versionStr, version: null };
    const operator = match[1] || '>='; // Default to >= for TypeScript version requirements
    const versionPart = match[2].trim();
    // Try to coerce to valid semver (handles "5.5" -> "5.5.0", "5" -> "5.0.0")
    const coerced = semver.coerce(versionPart);
    if (!coerced)
        return { range: versionStr, version: null };
    return {
        range: `${operator}${coerced.version}`,
        version: coerced.version,
    };
}
/**
 * Check if a test should be skipped based on @typeScriptVersion directive.
 * Returns true if the test requires a newer TS version than we support.
 *
 * Uses semver for robust version comparison.
 */
export function shouldSkipForVersion(harness) {
    if (!harness.typeScriptVersion)
        return false;
    const { range } = coerceVersion(harness.typeScriptVersion);
    // Check if our target version satisfies the requirement
    // If it doesn't satisfy, we should skip the test
    try {
        const satisfies = semver.satisfies(TSZ_TARGET_TS_VERSION, range);
        return !satisfies;
    }
    catch {
        // If semver can't parse the range, don't skip
        return false;
    }
}
/**
 * Check if a test should be skipped based on harness options.
 * Returns { skip: boolean, reason?: string }
 */
export function shouldSkipTest(harness) {
    // Skip @noCheck tests (parse-only, no semantic checking)
    if (harness.noCheck) {
        return { skip: true, reason: 'noCheck' };
    }
    // Skip tests requiring newer TypeScript versions
    if (shouldSkipForVersion(harness)) {
        return { skip: true, reason: `requires TS ${harness.typeScriptVersion}` };
    }
    return { skip: false };
}
/**
 * Convert parsed directives to CheckOptions for tsz-server.
 * Just passes through the directives - tsz handles lib loading.
 */
export function directivesToCheckOptions(directives, _libDirs = []) {
    const options = {};
    // Target - handle comma-separated targets (e.g., "es2015,es2017") by taking the first one
    if (directives.target !== undefined) {
        const targetStr = String(directives.target);
        const firstTarget = targetStr.split(',')[0].trim();
        options.target = firstTarget;
    }
    // noLib - pass through as-is
    if (directives.nolib !== undefined) {
        options.noLib = Boolean(directives.nolib);
    }
    // lib - pass through as-is, tsz handles resolution
    if (directives.lib !== undefined) {
        const libVal = directives.lib;
        if (typeof libVal === 'string') {
            options.lib = libVal.split(',').map(s => s.trim().toLowerCase()).filter(Boolean);
        }
        else if (Array.isArray(libVal)) {
            options.lib = libVal.map(s => String(s).trim().toLowerCase()).filter(Boolean);
        }
    }
    else if (!directives.nolib) {
        // Use FULL libs (like tsc does) to ensure all base types (Array, Object, Partial, Pick, etc.)
        // are available. The .full libs include ES5 base types via /// <reference lib="..." /> directives.
        // This matches tsc's actual default behavior and fixes TS2318/TS2583/TS2584 errors.
        const targetStr = String(directives.target ?? 'es5');
        const firstTarget = targetStr.split(',')[0].trim().toLowerCase();
        options.lib = [getFullLibNameForTarget(firstTarget)];
    }
    // Strict mode flags - ALWAYS set explicitly to ensure tsz-server gets the right default
    // When @strict is not specified, default to false (matching TSC's default behavior)
    options.strict = directives.strict !== undefined ? Boolean(directives.strict) : false;
    if (directives.strictnullchecks !== undefined) {
        options.strictNullChecks = Boolean(directives.strictnullchecks);
    }
    if (directives.strictfunctiontypes !== undefined) {
        options.strictFunctionTypes = Boolean(directives.strictfunctiontypes);
    }
    if (directives.strictpropertyinitialization !== undefined) {
        options.strictPropertyInitialization = Boolean(directives.strictpropertyinitialization);
    }
    if (directives.noimplicitany !== undefined) {
        options.noImplicitAny = Boolean(directives.noimplicitany);
    }
    if (directives.noimplicitthis !== undefined) {
        options.noImplicitThis = Boolean(directives.noimplicitthis);
    }
    if (directives.noimplicitreturns !== undefined) {
        options.noImplicitReturns = Boolean(directives.noimplicitreturns);
    }
    // Module options
    if (directives.module !== undefined) {
        options.module = String(directives.module);
    }
    if (directives.moduleresolution !== undefined) {
        options.moduleResolution = String(directives.moduleresolution);
    }
    // JSX
    if (directives.jsx !== undefined) {
        options.jsx = String(directives.jsx);
    }
    // JavaScript support
    if (directives.allowjs !== undefined) {
        options.allowJs = Boolean(directives.allowjs);
    }
    if (directives.checkjs !== undefined) {
        options.checkJs = Boolean(directives.checkjs);
    }
    // Declaration
    if (directives.declaration !== undefined) {
        options.declaration = Boolean(directives.declaration);
    }
    // Isolated modules
    if (directives.isolatedmodules !== undefined) {
        options.isolatedModules = Boolean(directives.isolatedmodules);
    }
    // Decorators
    if (directives.experimentaldecorators !== undefined) {
        options.experimentalDecorators = Boolean(directives.experimentaldecorators);
    }
    if (directives.emitdecoratormetadata !== undefined) {
        options.emitDecoratorMetadata = Boolean(directives.emitdecoratormetadata);
    }
    // Additional checks
    if (directives.nopropertyaccessfromindexsignature !== undefined) {
        options.noPropertyAccessFromIndexSignature = Boolean(directives.nopropertyaccessfromindexsignature);
    }
    if (directives.nouncheckedindexedaccess !== undefined) {
        options.noUncheckedIndexedAccess = Boolean(directives.nouncheckedindexedaccess);
    }
    if (directives.exactoptionalpropertytypes !== undefined) {
        options.exactOptionalPropertyTypes = Boolean(directives.exactoptionalpropertytypes);
    }
    if (directives.nofallthroughcasesinswitch !== undefined) {
        options.noFallthroughCasesInSwitch = Boolean(directives.nofallthroughcasesinswitch);
    }
    if (directives.nounusedlocals !== undefined) {
        options.noUnusedLocals = Boolean(directives.nounusedlocals);
    }
    if (directives.nounusedparameters !== undefined) {
        options.noUnusedParameters = Boolean(directives.nounusedparameters);
    }
    if (directives.allowunusedlabels !== undefined) {
        options.allowUnusedLabels = Boolean(directives.allowunusedlabels);
    }
    if (directives.allowunreachablecode !== undefined) {
        options.allowUnreachableCode = Boolean(directives.allowunreachablecode);
    }
    // Additional strictness flags
    if (directives.strictbindcallapply !== undefined) {
        options.strictBindCallApply = Boolean(directives.strictbindcallapply);
    }
    if (directives.useunknownincatchvariables !== undefined) {
        options.useUnknownInCatchVariables = Boolean(directives.useunknownincatchvariables);
    }
    if (directives.alwaysstrict !== undefined) {
        options.alwaysStrict = Boolean(directives.alwaysstrict);
    }
    if (directives.noimplicitoverride !== undefined) {
        options.noImplicitOverride = Boolean(directives.noimplicitoverride);
    }
    // Module resolution options
    if (directives.baseurl !== undefined) {
        options.baseUrl = String(directives.baseurl);
    }
    if (directives.paths !== undefined) {
        options.paths = directives.paths;
    }
    if (directives.rootdirs !== undefined) {
        options.rootDirs = directives.rootdirs;
    }
    if (directives.typeroots !== undefined) {
        options.typeRoots = directives.typeroots;
    }
    if (directives.types !== undefined) {
        options.types = directives.types;
    }
    if (directives.resolvejsonmodule !== undefined) {
        options.resolveJsonModule = Boolean(directives.resolvejsonmodule);
    }
    if (directives.moduledetection !== undefined) {
        options.moduleDetection = String(directives.moduledetection);
    }
    // Module interop options
    if (directives.esmoduleinterop !== undefined) {
        options.esModuleInterop = Boolean(directives.esmoduleinterop);
    }
    if (directives.allowsyntheticdefaultimports !== undefined) {
        options.allowSyntheticDefaultImports = Boolean(directives.allowsyntheticdefaultimports);
    }
    if (directives.preservesymlinks !== undefined) {
        options.preserveSymlinks = Boolean(directives.preservesymlinks);
    }
    if (directives.allowumdglobalaccess !== undefined) {
        options.allowUmdGlobalAccess = Boolean(directives.allowumdglobalaccess);
    }
    if (directives.verbatimmodulesyntax !== undefined) {
        options.verbatimModuleSyntax = Boolean(directives.verbatimmodulesyntax);
    }
    // JSX options
    if (directives.jsxfactory !== undefined) {
        options.jsxFactory = String(directives.jsxfactory);
    }
    if (directives.jsxfragmentfactory !== undefined) {
        options.jsxFragmentFactory = String(directives.jsxfragmentfactory);
    }
    if (directives.jsximportsource !== undefined) {
        options.jsxImportSource = String(directives.jsximportsource);
    }
    // Class features
    if (directives.usedefineforclassfields !== undefined) {
        options.useDefineForClassFields = Boolean(directives.usedefineforclassfields);
    }
    // Other options
    if (directives.forceconsistentcasinginfilenames !== undefined) {
        options.forceConsistentCasingInFileNames = Boolean(directives.forceconsistentcasinginfilenames);
    }
    if (directives.skiplibcheck !== undefined) {
        options.skipLibCheck = Boolean(directives.skiplibcheck);
    }
    if (directives.noresolve !== undefined) {
        options.noResolve = Boolean(directives.noresolve);
    }
    // Emit options (may be relevant for declaration checks)
    if (directives.noemit !== undefined) {
        options.noEmit = Boolean(directives.noemit);
    }
    if (directives.declarationmap !== undefined) {
        options.declarationMap = Boolean(directives.declarationmap);
    }
    if (directives.emitdeclarationonly !== undefined) {
        options.emitDeclarationOnly = Boolean(directives.emitdeclarationonly);
    }
    if (directives.outfile !== undefined) {
        options.outFile = String(directives.outfile);
    }
    if (directives.outdir !== undefined) {
        options.outDir = String(directives.outdir);
    }
    if (directives.rootdir !== undefined) {
        options.rootDir = String(directives.rootdir);
    }
    if (directives.declarationdir !== undefined) {
        options.declarationDir = String(directives.declarationdir);
    }
    if (directives.importhelpers !== undefined) {
        options.importHelpers = Boolean(directives.importhelpers);
    }
    if (directives.downleveliteration !== undefined) {
        options.downlevelIteration = Boolean(directives.downleveliteration);
    }
    return options;
}
/**
 * Get lib names for a test case.
 * Just parses the @lib directive - doesn't resolve dependencies.
 */
export function getLibNamesForDirectives(directives, _libDirs = []) {
    if (directives.nolib) {
        return [];
    }
    if (directives.lib === undefined) {
        return [];
    }
    return parseLibOption(directives.lib);
}
/**
 * Parse lib option value (string, array, or unknown) into array of lib names.
 * Shared utility for test runners.
 */
export function parseLibOption(libOpt) {
    if (typeof libOpt === 'string') {
        return libOpt
            .split(',')
            .map(s => s.trim().toLowerCase())
            .filter(Boolean);
    }
    if (Array.isArray(libOpt)) {
        return libOpt.map(v => String(v).trim().toLowerCase()).filter(Boolean);
    }
    return [];
}
//# sourceMappingURL=test-utils.js.map