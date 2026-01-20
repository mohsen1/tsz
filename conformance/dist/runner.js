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
import * as ts from 'typescript';
import * as path from 'path';
import * as fs from 'fs';
import { compareDiagnostics, formatComparison } from './compare.js';
import { loadBaseline, compareWithBaseline, formatBaselineComparison } from './baseline.js';
const DEFAULT_CONFIG = {
    wasmPkgPath: path.resolve(import.meta.dirname || __dirname, '../../pkg'),
    testsBasePath: path.resolve(import.meta.dirname || __dirname, '../../TypeScript/tests/cases'),
    libPath: path.resolve(import.meta.dirname || __dirname, '../../TypeScript/tests/lib/lib.d.ts'),
    maxTests: 500,
    verbose: false,
    categories: ['conformance', 'compiler'],
    timeout: 10000,
};
/**
 * Color utilities for terminal output
 */
const colors = {
    reset: '\x1b[0m',
    red: '\x1b[31m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    blue: '\x1b[34m',
    cyan: '\x1b[36m',
    dim: '\x1b[2m',
    bold: '\x1b[1m',
};
function log(msg, color = '') {
    console.log(`${color}${msg}${colors.reset}`);
}
function logProgress(current, total, extra = '') {
    const percent = ((current / total) * 100).toFixed(1);
    process.stdout.write(`\r  Progress: ${current}/${total} (${percent}%)${extra}          `);
}
/**
 * Parse test directives from TypeScript test files
 *
 * Directives are case-insensitive (e.g., @filename and @Filename are equivalent)
 */
function parseTestDirectives(code, filePath) {
    const lines = code.split('\n');
    const options = {};
    let isMultiFile = false;
    const files = [];
    let currentFileName = null;
    let currentFileLines = [];
    const cleanLines = [];
    for (const line of lines) {
        const trimmed = line.trim();
        // Check for @filename directive (multi-file test) - case insensitive
        const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/i);
        if (filenameMatch) {
            isMultiFile = true;
            // Save previous file if any
            if (currentFileName) {
                files.push({
                    name: currentFileName,
                    content: currentFileLines.join('\n'),
                    relativePath: currentFileName,
                });
            }
            currentFileName = filenameMatch[1].trim();
            currentFileLines = [];
            continue;
        }
        // Parse compiler options like // @strict: true - case insensitive
        const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
        if (optionMatch) {
            const [, key, value] = optionMatch;
            const lowKey = key.toLowerCase(); // normalize to lowercase
            // Parse boolean/number values
            if (value.toLowerCase() === 'true')
                options[lowKey] = true;
            else if (value.toLowerCase() === 'false')
                options[lowKey] = false;
            else if (!isNaN(Number(value)))
                options[lowKey] = Number(value);
            else
                options[lowKey] = value;
            continue;
        }
        if (isMultiFile && currentFileName) {
            currentFileLines.push(line);
        }
        else {
            cleanLines.push(line);
        }
    }
    // Save the last file for multi-file tests
    if (isMultiFile && currentFileName) {
        files.push({
            name: currentFileName,
            content: currentFileLines.join('\n'),
            relativePath: currentFileName,
        });
    }
    // For single-file tests
    if (!isMultiFile) {
        const baseName = path.basename(filePath);
        files.push({
            name: baseName,
            content: cleanLines.join('\n'),
            relativePath: baseName,
        });
    }
    // Extract category and test name from path
    const relativePath = filePath.replace(/.*tests\/cases\//, '');
    const parts = relativePath.split(path.sep);
    const category = parts[0] || 'unknown';
    const testName = parts.slice(1).join('/').replace(/\.ts$/, '');
    return {
        options,
        isMultiFile,
        files,
        category,
        testName,
    };
}
/**
 * Convert test options to TypeScript CompilerOptions
 */
function toCompilerOptions(testOptions) {
    const options = {
        strict: testOptions.strict !== false,
        target: ts.ScriptTarget.ES2020,
        module: ts.ModuleKind.ESNext,
        noEmit: true,
        skipLibCheck: true,
    };
    // Map target option
    if (testOptions.target) {
        const targetMap = {
            'es5': ts.ScriptTarget.ES5,
            'es6': ts.ScriptTarget.ES2015,
            'es2015': ts.ScriptTarget.ES2015,
            'es2016': ts.ScriptTarget.ES2016,
            'es2017': ts.ScriptTarget.ES2017,
            'es2018': ts.ScriptTarget.ES2018,
            'es2019': ts.ScriptTarget.ES2019,
            'es2020': ts.ScriptTarget.ES2020,
            'es2021': ts.ScriptTarget.ES2021,
            'es2022': ts.ScriptTarget.ES2022,
            'esnext': ts.ScriptTarget.ESNext,
        };
        const target = String(testOptions.target).toLowerCase();
        options.target = targetMap[target] || ts.ScriptTarget.ES2020;
    }
    // Map module option
    if (testOptions.module) {
        const moduleMap = {
            'commonjs': ts.ModuleKind.CommonJS,
            'amd': ts.ModuleKind.AMD,
            'umd': ts.ModuleKind.UMD,
            'system': ts.ModuleKind.System,
            'es6': ts.ModuleKind.ES2015,
            'es2015': ts.ModuleKind.ES2015,
            'es2020': ts.ModuleKind.ES2020,
            'es2022': ts.ModuleKind.ES2022,
            'esnext': ts.ModuleKind.ESNext,
            'node16': ts.ModuleKind.Node16,
            'nodenext': ts.ModuleKind.NodeNext,
            'preserve': ts.ModuleKind.Preserve,
            'none': ts.ModuleKind.None,
        };
        const mod = String(testOptions.module).toLowerCase();
        options.module = moduleMap[mod] || ts.ModuleKind.ESNext;
    }
    // Map moduleResolution option
    if (testOptions.moduleresolution) {
        const resolutionMap = {
            'classic': ts.ModuleResolutionKind.Classic,
            'node': ts.ModuleResolutionKind.NodeJs,
            'node10': ts.ModuleResolutionKind.Node10,
            'node16': ts.ModuleResolutionKind.Node16,
            'nodenext': ts.ModuleResolutionKind.NodeNext,
            'bundler': ts.ModuleResolutionKind.Bundler,
        };
        const res = String(testOptions.moduleresolution).toLowerCase();
        options.moduleResolution = resolutionMap[res] || ts.ModuleResolutionKind.NodeJs;
    }
    // Map jsx option
    if (testOptions.jsx) {
        const jsxMap = {
            'preserve': ts.JsxEmit.Preserve,
            'react': ts.JsxEmit.React,
            'react-native': ts.JsxEmit.ReactNative,
            'react-jsx': ts.JsxEmit.ReactJSX,
            'react-jsxdev': ts.JsxEmit.ReactJSXDev,
        };
        const jsx = String(testOptions.jsx).toLowerCase();
        options.jsx = jsxMap[jsx] || ts.JsxEmit.React;
    }
    // Boolean options
    const booleanOptions = [
        ['noimplicitany', 'noImplicitAny'],
        ['strictnullchecks', 'strictNullChecks'],
        ['noimplicitreturns', 'noImplicitReturns'],
        ['noimplicitthis', 'noImplicitThis'],
        ['strictfunctiontypes', 'strictFunctionTypes'],
        ['strictpropertyinitialization', 'strictPropertyInitialization'],
        ['nounusedlocals', 'noUnusedLocals'],
        ['nounusedparameters', 'noUnusedParameters'],
        ['alwaysstrict', 'alwaysStrict'],
        ['declaration', 'declaration'],
        ['declarationmap', 'declarationMap'],
        ['sourcemap', 'sourceMap'],
        ['nolib', 'noLib'],
        ['skiplibcheck', 'skipLibCheck'],
        ['checkjs', 'checkJs'],
        ['allowjs', 'allowJs'],
        ['experimentaldecorators', 'experimentalDecorators'],
        ['emitdecoratormetadata', 'emitDecoratorMetadata'],
        ['usedefineforclassproperty', 'useDefineForClassFields'],
    ];
    for (const [testKey, compilerKey] of booleanOptions) {
        if (testOptions[testKey] !== undefined) {
            options[compilerKey] = testOptions[testKey];
        }
    }
    // Handle lib option (array of library names)
    if (testOptions.lib) {
        const libStr = String(testOptions.lib);
        options.lib = libStr.split(',').map(s => s.trim());
    }
    return options;
}
/**
 * Run TypeScript compiler on test files
 */
async function runTsc(testCase, libSource) {
    const compilerOptions = toCompilerOptions(testCase.options);
    // Create source files map
    const sourceFiles = new Map();
    const fileNames = [];
    for (const file of testCase.files) {
        const sf = ts.createSourceFile(file.name, file.content, ts.ScriptTarget.ES2020, true);
        sourceFiles.set(file.name, sf);
        fileNames.push(file.name);
    }
    // Add lib.d.ts if not explicitly disabled
    if (!testCase.options.nolib) {
        const libSf = ts.createSourceFile('lib.d.ts', libSource, ts.ScriptTarget.ES2020, true);
        sourceFiles.set('lib.d.ts', libSf);
    }
    // Create compiler host
    const host = ts.createCompilerHost(compilerOptions);
    const originalGetSourceFile = host.getSourceFile;
    host.getSourceFile = (name, languageVersion, onError) => {
        if (sourceFiles.has(name)) {
            return sourceFiles.get(name);
        }
        return originalGetSourceFile.call(host, name, languageVersion, onError);
    };
    host.fileExists = (name) => {
        return sourceFiles.has(name) || ts.sys.fileExists(name);
    };
    host.readFile = (name) => {
        const file = testCase.files.find(f => f.name === name);
        if (file)
            return file.content;
        if (name === 'lib.d.ts' && !testCase.options.nolib)
            return libSource;
        return ts.sys.readFile(name);
    };
    const program = ts.createProgram(fileNames, compilerOptions, host);
    // Collect diagnostics from all source files
    const allDiagnostics = [];
    for (const sf of sourceFiles.values()) {
        if (sf.fileName !== 'lib.d.ts') {
            allDiagnostics.push(...program.getSyntacticDiagnostics(sf));
            allDiagnostics.push(...program.getSemanticDiagnostics(sf));
        }
    }
    return {
        diagnostics: allDiagnostics.map(d => ({
            code: d.code,
            message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
            category: ts.DiagnosticCategory[d.category],
            file: d.file?.fileName,
            start: d.start,
            length: d.length,
        })),
        crashed: false,
    };
}
/**
 * Run WASM compiler on test files
 */
async function runWasm(testCase, wasmModule, libSource) {
    try {
        const wasm = wasmModule;
        if (testCase.isMultiFile || testCase.files.length > 1) {
            // Multi-file test - use WasmProgram
            const program = new wasm.WasmProgram();
            if (!testCase.options.nolib) {
                program.addFile('lib.d.ts', libSource);
            }
            for (const file of testCase.files) {
                program.addFile(file.name, file.content);
            }
            const codes = program.getAllDiagnosticCodes();
            return {
                diagnostics: Array.from(codes).map((code) => ({
                    code,
                    message: '',
                    category: 'Error',
                })),
                crashed: false,
            };
        }
        else {
            // Single-file test - use ThinParser
            const file = testCase.files[0];
            const parser = new wasm.ThinParser(file.name, file.content);
            if (!testCase.options.nolib) {
                parser.addLibFile('lib.d.ts', libSource);
            }
            // Set compiler options if method exists
            if (parser.setCompilerOptions) {
                parser.setCompilerOptions(JSON.stringify(testCase.options));
            }
            parser.parseSourceFile();
            const parseDiagsJson = parser.getDiagnosticsJson();
            const parseDiags = JSON.parse(parseDiagsJson);
            const checkResultJson = parser.checkSourceFile();
            const checkResult = JSON.parse(checkResultJson);
            const diagnostics = [
                ...parseDiags.map((d) => ({
                    code: d.code,
                    message: d.message,
                    category: 'Error',
                })),
                ...(checkResult.diagnostics || []).map((d) => ({
                    code: d.code,
                    message: d.message_text,
                    category: d.category,
                })),
            ];
            parser.free();
            return {
                diagnostics,
                crashed: false,
            };
        }
    }
    catch (error) {
        return {
            diagnostics: [],
            crashed: true,
            error: error instanceof Error ? error.message : String(error),
        };
    }
}
/**
 * Recursively collect test files
 */
function collectTestFiles(dir, maxFiles) {
    const files = [];
    function walk(currentDir) {
        if (files.length >= maxFiles)
            return;
        let entries;
        try {
            entries = fs.readdirSync(currentDir);
        }
        catch {
            return;
        }
        for (const entry of entries) {
            if (files.length >= maxFiles)
                break;
            const fullPath = path.join(currentDir, entry);
            let stat;
            try {
                stat = fs.statSync(fullPath);
            }
            catch {
                continue;
            }
            if (stat.isDirectory()) {
                walk(fullPath);
            }
            else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) {
                files.push(fullPath);
            }
        }
    }
    walk(dir);
    return files;
}
/**
 * Initialize statistics object
 */
function createStats() {
    return {
        total: 0,
        passed: 0,
        failed: 0,
        crashed: 0,
        skipped: 0,
        exactMatch: 0,
        sameErrorCount: 0,
        missingErrors: 0,
        extraErrors: 0,
        baselineMatches: 0,
        baselineTotal: 0,
        byCategory: {},
        byErrorCode: {},
    };
}
/**
 * Update statistics with test result
 */
function updateStats(stats, category, comparison, crashed, testFile) {
    stats.total++;
    if (!stats.byCategory[category]) {
        stats.byCategory[category] = { total: 0, passed: 0, failed: 0, exactMatch: 0 };
    }
    stats.byCategory[category].total++;
    if (crashed) {
        stats.crashed++;
        stats.failed++;
        stats.byCategory[category].failed++;
        return;
    }
    if (comparison.exactMatch) {
        stats.passed++;
        stats.exactMatch++;
        stats.byCategory[category].passed++;
        stats.byCategory[category].exactMatch++;
    }
    else {
        stats.failed++;
        stats.byCategory[category].failed++;
        if (comparison.sameCount) {
            stats.sameErrorCount++;
        }
    }
    stats.missingErrors += comparison.missingInWasm.length;
    stats.extraErrors += comparison.extraInWasm.length;
    // Track error codes
    for (const code of comparison.missingInWasm) {
        if (!stats.byErrorCode[code]) {
            stats.byErrorCode[code] = { missingCount: 0, extraCount: 0, testFiles: [] };
        }
        stats.byErrorCode[code].missingCount++;
        if (!stats.byErrorCode[code].testFiles.includes(testFile)) {
            stats.byErrorCode[code].testFiles.push(testFile);
        }
    }
    for (const code of comparison.extraInWasm) {
        if (!stats.byErrorCode[code]) {
            stats.byErrorCode[code] = { missingCount: 0, extraCount: 0, testFiles: [] };
        }
        stats.byErrorCode[code].extraCount++;
        if (!stats.byErrorCode[code].testFiles.includes(testFile)) {
            stats.byErrorCode[code].testFiles.push(testFile);
        }
    }
}
/**
 * Print final statistics report
 */
function printReport(stats, verbose) {
    log('\n' + '═'.repeat(60), colors.dim);
    log('CONFORMANCE TEST RESULTS', colors.bold);
    log('═'.repeat(60), colors.dim);
    const passRate = stats.total > 0 ? ((stats.passed / stats.total) * 100).toFixed(1) : '0.0';
    const exactMatchRate = stats.total > 0 ? ((stats.exactMatch / stats.total) * 100).toFixed(1) : '0.0';
    const baselineRate = stats.baselineTotal > 0
        ? ((stats.baselineMatches / stats.baselineTotal) * 100).toFixed(1)
        : '0.0';
    log(`\nOverall Pass Rate: ${passRate}%`, stats.passed === stats.total ? colors.green : colors.yellow);
    log(`Exact Match Rate:  ${exactMatchRate}%`, colors.cyan);
    log(`Baseline Match:    ${baselineRate}% (${stats.baselineMatches}/${stats.baselineTotal})`, colors.cyan);
    log('\nSummary:', colors.bold);
    log(`  Total:        ${stats.total}`);
    log(`  Passed:       ${stats.passed}`, colors.green);
    log(`  Failed:       ${stats.failed}`, stats.failed > 0 ? colors.red : colors.dim);
    log(`  Crashed:      ${stats.crashed}`, stats.crashed > 0 ? colors.red : colors.dim);
    log(`  Skipped:      ${stats.skipped}`, colors.dim);
    log('\nDiagnostic Accuracy (vs TSC):', colors.bold);
    log(`  Exact Match:  ${stats.exactMatch}`);
    log(`  Same Count:   ${stats.sameErrorCount}`);
    log(`  Missing:      ${stats.missingErrors}`, stats.missingErrors > 0 ? colors.yellow : colors.dim);
    log(`  Extra:        ${stats.extraErrors}`, stats.extraErrors > 0 ? colors.yellow : colors.dim);
    if (Object.keys(stats.byCategory).length > 0) {
        log('\nBy Category:', colors.bold);
        for (const [cat, catStats] of Object.entries(stats.byCategory)) {
            const catRate = catStats.total > 0 ? ((catStats.passed / catStats.total) * 100).toFixed(1) : '0.0';
            log(`  ${cat}: ${catStats.passed}/${catStats.total} (${catRate}%)`, catStats.passed === catStats.total ? colors.green : colors.yellow);
        }
    }
    if (verbose && Object.keys(stats.byErrorCode).length > 0) {
        log('\nTop Missing Error Codes:', colors.bold);
        const sortedMissing = Object.entries(stats.byErrorCode)
            .filter(([, s]) => s.missingCount > 0)
            .sort((a, b) => b[1].missingCount - a[1].missingCount)
            .slice(0, 10);
        for (const [code, codeStats] of sortedMissing) {
            log(`  TS${code}: missing ${codeStats.missingCount}x`, colors.yellow);
        }
        log('\nTop Extra Error Codes:', colors.bold);
        const sortedExtra = Object.entries(stats.byErrorCode)
            .filter(([, s]) => s.extraCount > 0)
            .sort((a, b) => b[1].extraCount - a[1].extraCount)
            .slice(0, 10);
        for (const [code, codeStats] of sortedExtra) {
            log(`  TS${code}: extra ${codeStats.extraCount}x`, colors.yellow);
        }
    }
    log('\n' + '═'.repeat(60), colors.dim);
}
/**
 * Main entry point
 */
export async function runConformanceTests(config = {}) {
    const cfg = { ...DEFAULT_CONFIG, ...config };
    log('Conformance Test Runner', colors.bold);
    log('═'.repeat(60), colors.dim);
    // Load lib.d.ts
    let libSource = '';
    try {
        libSource = fs.readFileSync(cfg.libPath, 'utf8');
        log(`  Loaded lib.d.ts (${(libSource.length / 1024).toFixed(1)}KB)`, colors.dim);
    }
    catch {
        log('  Warning: Could not load lib.d.ts', colors.yellow);
    }
    // Load WASM module
    let wasmModule;
    try {
        const wasmPath = path.join(cfg.wasmPkgPath, 'wasm.js');
        const module = await import(wasmPath);
        // Initialize the WASM module - this is required before using any exports
        if (typeof module.default === 'function') {
            await module.default();
        }
        wasmModule = module;
        log(`  Loaded WASM module`, colors.dim);
    }
    catch (error) {
        log(`  Error loading WASM module: ${error}`, colors.red);
        return createStats();
    }
    // Collect test files - distribute evenly across categories
    log(`\nCollecting test files...`, colors.cyan);
    const allTestFiles = [];
    const testsPerCategory = Math.ceil(cfg.maxTests / cfg.categories.length);
    for (const category of cfg.categories) {
        const categoryDir = path.join(cfg.testsBasePath, category);
        if (fs.existsSync(categoryDir)) {
            const remaining = cfg.maxTests - allTestFiles.length;
            const limit = Math.min(testsPerCategory, remaining);
            const files = collectTestFiles(categoryDir, limit);
            allTestFiles.push(...files);
            log(`  ${category}: ${files.length} files`, colors.dim);
        }
    }
    log(`  Total: ${allTestFiles.length} test files`, colors.cyan);
    if (allTestFiles.length === 0) {
        log('\nNo test files found!', colors.yellow);
        return createStats();
    }
    // Run tests
    log(`\nRunning tests...`, colors.cyan);
    const stats = createStats();
    const failedTests = [];
    const crashedTests = [];
    for (let i = 0; i < allTestFiles.length; i++) {
        const filePath = allTestFiles[i];
        const relPath = filePath.replace(cfg.testsBasePath + path.sep, '');
        if (!cfg.verbose) {
            logProgress(i + 1, allTestFiles.length);
        }
        try {
            const code = fs.readFileSync(filePath, 'utf8');
            const testCase = parseTestDirectives(code, filePath);
            // Run both compilers
            const [tscResult, wasmResult] = await Promise.all([
                runTsc(testCase, libSource),
                runWasm(testCase, wasmModule, libSource),
            ]);
            const comparison = compareDiagnostics(tscResult, wasmResult);
            // Load and compare with TypeScript baseline
            const baseline = loadBaseline(filePath, cfg.testsBasePath);
            const wasmCodes = wasmResult.diagnostics.map(d => d.code);
            const baselineComparison = compareWithBaseline(wasmCodes, baseline);
            // Track baseline statistics
            if (baseline.exists) {
                stats.baselineTotal++;
                if (baselineComparison.exactMatch) {
                    stats.baselineMatches++;
                }
            }
            else if (wasmCodes.length === 0) {
                // No baseline and no errors = match
                stats.baselineTotal++;
                stats.baselineMatches++;
            }
            updateStats(stats, testCase.category, comparison, wasmResult.crashed, relPath);
            if (wasmResult.crashed) {
                crashedTests.push({ file: relPath, error: wasmResult.error || 'Unknown error' });
            }
            else if (!comparison.exactMatch) {
                failedTests.push({ file: relPath, comparison, baseline: baselineComparison });
            }
            if (cfg.verbose && !comparison.exactMatch) {
                log(`\n  ${relPath}:`, colors.yellow);
                log(`    TSC: ${formatComparison(comparison)}`, colors.dim);
                log(`    Baseline: ${formatBaselineComparison(baselineComparison)}`, colors.dim);
            }
        }
        catch (error) {
            stats.skipped++;
            if (cfg.verbose) {
                log(`\n  ${relPath}: SKIPPED (${error})`, colors.yellow);
            }
        }
    }
    // Clear progress line
    if (!cfg.verbose) {
        process.stdout.write('\r' + ' '.repeat(60) + '\r');
    }
    // Print results
    printReport(stats, cfg.verbose);
    // Print crashed tests if any
    if (crashedTests.length > 0 && cfg.verbose) {
        log('\nCrashed Tests:', colors.red);
        for (const { file, error } of crashedTests.slice(0, 10)) {
            log(`  ${file}: ${error}`, colors.dim);
        }
        if (crashedTests.length > 10) {
            log(`  ... and ${crashedTests.length - 10} more`, colors.dim);
        }
    }
    return stats;
}
// CLI support
if (import.meta.url === `file://${process.argv[1]}`) {
    const args = process.argv.slice(2);
    const config = {};
    for (const arg of args) {
        if (arg.startsWith('--max=')) {
            config.maxTests = parseInt(arg.split('=')[1], 10);
        }
        else if (arg === '--verbose' || arg === '-v') {
            config.verbose = true;
        }
        else if (arg.startsWith('--category=')) {
            config.categories = arg.split('=')[1].split(',');
        }
    }
    runConformanceTests(config).then(stats => {
        process.exit(stats.failed > 0 ? 1 : 0);
    });
}
//# sourceMappingURL=runner.js.map