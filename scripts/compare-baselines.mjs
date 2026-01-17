#!/usr/bin/env node
/**
 * Baseline Comparison Tool - Compare WASM compiler output against TypeScript baselines
 * 
 * This script runs TypeScript test cases through the WASM compiler and compares
 * the output (diagnostics, types, etc.) against the expected baseline files.
 * Useful for regression testing and understanding differences.
 *
 * Usage:
 *   node scripts/compare-baselines.mjs [limit] [category]
 *   node scripts/compare-baselines.mjs 100 compiler      # Test first 100 compiler tests
 *   node scripts/compare-baselines.mjs 100 conformance   # Test first 100 conformance tests
 *   node scripts/compare-baselines.mjs --summary         # Just show summary
 *   
 * Output:
 *   - Shows pass/fail for each test
 *   - Highlights differences in error messages
 *   - Provides overall statistics
 */

import { readFileSync, existsSync, readdirSync } from 'fs';
import { basename, join, extname } from 'path';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

// ========================================
// Utilities
// ========================================

/**
 * Read a source file, handling UTF-16 BOM encoding.
 */
function readSourceFile(path) {
    const buffer = readFileSync(path);

    // Check for UTF-16 BE BOM (FE FF)
    if (buffer.length >= 2 && buffer[0] === 0xFE && buffer[1] === 0xFF) {
        const swapped = Buffer.alloc(buffer.length - 2);
        for (let i = 2; i < buffer.length; i += 2) {
            if (i + 1 < buffer.length) {
                swapped[i - 2] = buffer[i + 1];
                swapped[i - 1] = buffer[i];
            }
        }
        return swapped.toString('utf16le');
    }

    // Check for UTF-16 LE BOM (FF FE)
    if (buffer.length >= 2 && buffer[0] === 0xFF && buffer[1] === 0xFE) {
        return buffer.slice(2).toString('utf16le');
    }

    // Check for UTF-8 BOM (EF BB BF)
    if (buffer.length >= 3 && buffer[0] === 0xEF && buffer[1] === 0xBB && buffer[2] === 0xBF) {
        return buffer.slice(3).toString('utf-8');
    }

    return buffer.toString('utf-8');
}

/**
 * Parse expected error codes from a .errors.txt baseline file.
 */
function parseExpectedErrors(content) {
    const codes = [];
    const regex = /: error TS(\d+):/g;
    let match;
    while ((match = regex.exec(content)) !== null) {
        codes.push(parseInt(match[1], 10));
    }
    return codes.sort((a, b) => a - b);
}

/**
 * Extract the JS emit portion from a baseline file.
 * Baseline files have format:
 *   //// [tests/cases/...] ////
 *   //// [filename.ts] ////
 *   <source>
 *   //// [filename.js] ////
 *   <emitted js>
 */
function extractJsFromBaseline(baselineContent, testName) {
    const lines = baselineContent.split('\n');
    const jsMarkerRegex = /^\/\/\/\/\s*\[([^\]]+\.js)\]\s*$/;

    let inJsSection = false;
    let jsLines = [];

    for (const line of lines) {
        const markerMatch = line.match(jsMarkerRegex);

        if (markerMatch) {
            // Found a .js marker, start capturing
            inJsSection = true;
            jsLines = [];
            continue;
        }

        // Check if we hit another marker (like .d.ts) which ends the JS section
        if (inJsSection && line.startsWith('//// [')) {
            break;
        }

        if (inJsSection) {
            jsLines.push(line);
        }
    }

    return jsLines.join('\n');
}

/**
 * Compare error code arrays.
 */
function compareErrorCodes(expected, actual) {
    // For now, just compare the sets of codes (ignore counts)
    const expectedSet = new Set(expected);
    const actualSet = new Set(actual);

    const missing = [...expectedSet].filter(c => !actualSet.has(c));
    const extra = [...actualSet].filter(c => !expectedSet.has(c));
    const matched = [...expectedSet].filter(c => actualSet.has(c));

    return {
        missing,
        extra,
        matched,
        pass: missing.length === 0 && extra.length === 0
    };
}

/**
 * Parse a multi-file test case with @filename: directives.
 */
function parseMultiFileTest(source) {
    const files = [];
    const lines = source.split('\n');
    let currentFile = null;
    let currentContent = [];
    let headerLines = [];

    for (const line of lines) {
        const filenameMatch = line.match(/^\/\/\s*@filename:\s*(.+)$/i);

        if (filenameMatch) {
            if (currentFile !== null) {
                files.push({
                    filename: currentFile,
                    content: currentContent.join('\n')
                });
            }
            currentFile = filenameMatch[1].trim();
            currentContent = [];
        } else if (currentFile !== null) {
            currentContent.push(line);
        } else {
            headerLines.push(line);
        }
    }

    if (currentFile !== null) {
        files.push({
            filename: currentFile,
            content: currentContent.join('\n')
        });
    }

    return { files, headerLines };
}

function isParseableFile(filename) {
    const ext = extname(filename).toLowerCase();
    return ['.ts', '.tsx', '.js', '.jsx', '.mts', '.cts', '.mjs', '.cjs'].includes(ext);
}

// ========================================
// Main
// ========================================

// Load WASM module
let wasm;
try {
    wasm = require('../pkg/wasm.js');
} catch (e) {
    console.error('Failed to load WASM module. Run: wasm-pack build wasm --target nodejs');
    process.exit(1);
}

// Parse args
const args = process.argv.slice(2);
const showSummaryOnly = args.includes('--summary');
const limit = parseInt(args.find(a => /^\d+$/.test(a))) || 100;
const category = args.find(a => ['compiler', 'conformance'].includes(a)) || 'compiler';
const testDir = `tests/cases/${category}`;
const baselineDir = 'tests/baselines/reference';

// Get test files recursively
function getFiles(dir, files = []) {
    const entries = readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) {
            getFiles(path, files);
        } else if (entry.name.endsWith('.ts') || entry.name.endsWith('.tsx')) {
            files.push(path);
        }
    }
    return files;
}

const allFiles = getFiles(testDir);
const files = allFiles.slice(0, limit);

console.log(`\n=== Baseline Comparison: ${category} (${files.length} files) ===\n`);

// Stats
const stats = {
    total: 0,
    skipped: 0,
    crashed: 0,
    errorBaseline: {
        total: 0,
        pass: 0,
        fail: 0,
        noBaseline: 0
    },
    jsBaseline: {
        total: 0,
        pass: 0,
        fail: 0,
        noBaseline: 0
    }
};

const failures = [];

/**
 * Process a single file and compare against baselines.
 */
function processFile(filePath, source, testName) {
    const useThinParser = wasm.createThinParser !== undefined;
    const parser = useThinParser
        ? wasm.createThinParser(basename(filePath), source)
        : wasm.createParser(basename(filePath), source);

    try {
        parser.parseSourceFile();
        const diagnosticsJson = parser.getDiagnosticsJson();
        const diagnostics = JSON.parse(diagnosticsJson);

        if (useThinParser) {
            parser.bindSourceFile();
        } else {
            parser.bindSourceFile(0);
        }

        const checkJson = parser.checkSourceFile();
        const checkResult = JSON.parse(checkJson);

        // Collect actual error codes from both parse and check phases
        const actualCodes = [];
        // Parse errors (1xxx range)
        if (diagnostics) {
            diagnostics.forEach(d => {
                if (d.code) actualCodes.push(d.code);
            });
        }
        // Type check errors (2xxx range)
        if (checkResult.diagnostics) {
            checkResult.diagnostics.forEach(d => {
                if (d.code) actualCodes.push(d.code);
            });
        }
        actualCodes.sort((a, b) => a - b);

        // Get emitted JS
        let emittedJs = '';
        try {
            emittedJs = parser.emit ? parser.emit() : '';
        } catch (e) {
            emittedJs = '';
        }

        try { parser.free(); } catch (e) { /* ignore free errors */ }

        return {
            success: true,
            actualCodes,
            emittedJs
        };
    } catch (e) {
        try { parser.free && parser.free(); } catch (e2) { /* ignore */ }
        return {
            success: false,
            error: e.message
        };
    }
}

for (const file of files) {
    const source = readSourceFile(file);

    // Skip very large files
    if (source.length > 50000) {
        stats.skipped++;
        continue;
    }

    stats.total++;
    const testName = basename(file, extname(file));

    // Check if multi-file test
    const isMultiFile = source.toLowerCase().includes('@filename:');

    let result;
    if (isMultiFile) {
        // For multi-file tests, just process the first file for now
        const { files: testFiles } = parseMultiFileTest(source);
        const parseableFiles = testFiles.filter(f => isParseableFile(f.filename));

        if (parseableFiles.length === 0) {
            stats.skipped++;
            continue;
        }

        // Process first file
        result = processFile(parseableFiles[0].filename, parseableFiles[0].content, testName);
    } else {
        result = processFile(file, source, testName);
    }

    if (!result.success) {
        stats.crashed++;
        failures.push({ file: testName, error: result.error, type: 'crash' });
        continue;
    }

    // === Compare .errors.txt baseline ===
    const errorsBaseline = join(baselineDir, `${testName}.errors.txt`);
    if (existsSync(errorsBaseline)) {
        stats.errorBaseline.total++;
        const baselineContent = readFileSync(errorsBaseline, 'utf-8');
        const expectedCodes = parseExpectedErrors(baselineContent);
        const comparison = compareErrorCodes(expectedCodes, result.actualCodes);

        if (comparison.pass) {
            stats.errorBaseline.pass++;
        } else {
            stats.errorBaseline.fail++;
            if (!showSummaryOnly) {
                failures.push({
                    file: testName,
                    type: 'errors',
                    expected: expectedCodes,
                    actual: result.actualCodes,
                    missing: comparison.missing,
                    extra: comparison.extra
                });
            }
        }
    } else {
        stats.errorBaseline.noBaseline++;
        // No baseline means we expect no errors
        if (result.actualCodes.length > 0) {
            stats.errorBaseline.fail++;
            if (!showSummaryOnly) {
                failures.push({
                    file: testName,
                    type: 'errors-unexpected',
                    actual: result.actualCodes
                });
            }
        } else {
            stats.errorBaseline.pass++;
        }
    }

    // === Compare .js baseline ===
    const jsBaseline = join(baselineDir, `${testName}.js`);
    if (existsSync(jsBaseline)) {
        stats.jsBaseline.total++;
        const baselineContent = readFileSync(jsBaseline, 'utf-8');
        const expectedJs = extractJsFromBaseline(baselineContent, testName);

        if (result.emittedJs !== undefined) {
            const normalizeJs = (s) => s.replace(/\r\n/g, '\n').trim();
            const expectedNorm = normalizeJs(expectedJs);
            const actualNorm = normalizeJs(result.emittedJs);

            if (expectedNorm === actualNorm) {
                stats.jsBaseline.pass++;
            } else {
                stats.jsBaseline.fail++;
                if (!showSummaryOnly) {
                    failures.push({
                        file: testName,
                        type: 'js',
                        expected: expectedNorm.substring(0, 200),
                        actual: actualNorm.substring(0, 200)
                    });
                }
            }
        } else {
            stats.jsBaseline.fail++;
            if (!showSummaryOnly) {
                failures.push({
                    file: testName,
                    type: 'js-missing',
                    expected: expectedJs.substring(0, 200)
                });
            }
        }
    } else {
        stats.jsBaseline.noBaseline++;
    }
}

// ========================================
// Report
// ========================================

console.log('=== Results ===\n');
console.log(`Tested:   ${stats.total} files`);
console.log(`Skipped:  ${stats.skipped} (large or non-parseable)`);
console.log(`Crashed:  ${stats.crashed}\n`);

console.log('=== Error Baseline (.errors.txt) ===\n');
const errorTotal = stats.errorBaseline.pass + stats.errorBaseline.fail;
const errorRate = errorTotal > 0 ? (stats.errorBaseline.pass / errorTotal * 100).toFixed(1) : 0;
console.log(`Pass:        ${stats.errorBaseline.pass}/${errorTotal} (${errorRate}%)`);
console.log(`Fail:        ${stats.errorBaseline.fail}`);
console.log(`No baseline: ${stats.errorBaseline.noBaseline}\n`);

console.log('=== JS Baseline (.js) ===\n');
const jsTotal = stats.jsBaseline.pass + stats.jsBaseline.fail;
const jsRate = jsTotal > 0 ? (stats.jsBaseline.pass / jsTotal * 100).toFixed(1) : 0;
console.log(`Pass:        ${stats.jsBaseline.pass}/${jsTotal} (${jsRate}%)`);
console.log(`Fail:        ${stats.jsBaseline.fail}`);
console.log(`No baseline: ${stats.jsBaseline.noBaseline}\n`);

// Show failures
if (!showSummaryOnly && failures.length > 0) {
    console.log(`=== Failures (showing first 20) ===\n`);

    const crashFailures = failures.filter(f => f.type === 'crash');
    const errorFailures = failures.filter(f => f.type === 'errors' || f.type === 'errors-unexpected');

    if (crashFailures.length > 0) {
        console.log(`Crashes (${crashFailures.length}):`);
        crashFailures.slice(0, 5).forEach(f => {
            console.log(`  ${f.file}: ${f.error?.slice(0, 80)}`);
        });
        console.log('');
    }

    if (errorFailures.length > 0) {
        console.log(`Error mismatches (${errorFailures.length}):`);
        errorFailures.slice(0, 15).forEach(f => {
            if (f.type === 'errors-unexpected') {
                console.log(`  ${f.file}: unexpected errors ${f.actual.join(', ')}`);
            } else {
                console.log(`  ${f.file}: missing=${f.missing.join(',')} extra=${f.extra.join(',')}`);
            }
        });
        console.log('');
    }

    const jsFailures = failures.filter(f => f.type === 'js' || f.type === 'js-missing');
    if (jsFailures.length > 0) {
        console.log(`JS emit mismatches (${jsFailures.length}):`);
        jsFailures.slice(0, 10).forEach(f => {
            if (f.type === 'js-missing') {
                console.log(`  ${f.file}: no emit output`);
            } else {
                console.log(`  ${f.file}:`);
                console.log(`    expected: ${f.expected.replace(/\n/g, '\\n').substring(0, 80)}...`);
                console.log(`    actual:   ${f.actual.replace(/\n/g, '\\n').substring(0, 80)}...`);
            }
        });
    }
}

// Exit with error code if failures
process.exit(stats.crashed > 0 || stats.errorBaseline.fail > 0 ? 1 : 0);
