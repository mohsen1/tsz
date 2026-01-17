#!/usr/bin/env node
/**
 * Batch test runner for Rust/WASM TypeScript compiler
 * Tests multiple files and reports statistics
 *
 * Usage:
 *   node scripts/batch-test-rust.mjs [limit]
 *   node scripts/batch-test-rust.mjs 100   # Test first 100 files
 */

import { readFileSync, existsSync, readdirSync } from 'fs';
import { basename, join, extname } from 'path';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

/**
 * Read a source file, handling UTF-16 BOM encoding.
 * TypeScript test files may be UTF-16 BE or LE encoded.
 */
function readSourceFile(path) {
    const buffer = readFileSync(path);

    // Check for UTF-16 BE BOM (FE FF)
    if (buffer.length >= 2 && buffer[0] === 0xFE && buffer[1] === 0xFF) {
        // UTF-16 BE: swap bytes and decode as UTF-16 LE
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

    // Check for UTF-8 BOM (EF BB BF) - strip it for consistency
    if (buffer.length >= 3 && buffer[0] === 0xEF && buffer[1] === 0xBB && buffer[2] === 0xBF) {
        return buffer.slice(3).toString('utf-8');
    }

    // Default: UTF-8
    return buffer.toString('utf-8');
}

/**
 * Parse a multi-file test case with @filename: directives.
 * Returns an array of { filename, content } objects.
 */
function parseMultiFileTest(source) {
    const files = [];
    const lines = source.split('\n');

    let currentFile = null;
    let currentContent = [];
    let headerLines = [];  // Lines before first @filename

    for (const line of lines) {
        // Match @filename: directive (case insensitive)
        const filenameMatch = line.match(/^\/\/\s*@filename:\s*(.+)$/i);

        if (filenameMatch) {
            // Save previous file if exists
            if (currentFile !== null) {
                files.push({
                    filename: currentFile,
                    content: currentContent.join('\n')
                });
            }

            currentFile = filenameMatch[1].trim();
            currentContent = [];
        } else if (currentFile !== null) {
            // Add line to current file content
            currentContent.push(line);
        } else {
            // Header line (compiler options, etc.) before first @filename
            headerLines.push(line);
        }
    }

    // Save last file
    if (currentFile !== null) {
        files.push({
            filename: currentFile,
            content: currentContent.join('\n')
        });
    }

    return { files, headerLines };
}

/**
 * Check if a filename is a TypeScript/JavaScript file we can parse.
 */
function isParseableFile(filename) {
    const ext = extname(filename).toLowerCase();
    return ['.ts', '.tsx', '.js', '.jsx', '.mts', '.cts', '.mjs', '.cjs'].includes(ext);
}

// Load WASM module
let wasm;
try {
    wasm = require('../pkg/wasm.js');
} catch (e) {
    console.error('Failed to load WASM module. Run: wasm-pack build wasm --target nodejs');
    process.exit(1);
}

const limit = parseInt(process.argv[2]) || 50;
const testDir = process.argv[3] || 'tests/cases/compiler';

// Get test files recursively
function getFiles(dir, files = []) {
    const entries = readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
        const path = join(dir, entry.name);
        if (entry.isDirectory()) {
            getFiles(path, files);
        } else if (entry.name.endsWith('.ts')) {
            files.push(path);
        }
    }
    return files;
}

const allFiles = getFiles(testDir);
const files = allFiles.slice(0, limit);

console.log(`\n=== Batch Testing ${files.length} files ===\n`);

let passed = 0;
let failed = 0;
let parseErrors = 0;
let typeErrors = 0;
let totalParseTime = 0;
let totalBindTime = 0;
let totalCheckTime = 0;

const failures = [];

let skipped = 0;
let multiFilePassed = 0;
let multiFileFailed = 0;
let multiFileSkipped = 0;

/**
 * Process a single file (parse, bind, check).
 * Returns { success, parseErrors, typeErrors, parseTime, bindTime, checkTime }
 */
function processFile(filename, source, useThinParser) {
    const parser = useThinParser
        ? wasm.createThinParser(filename, source)
        : wasm.createParser(filename, source);

    // Parse
    const parseStart = performance.now();
    const rootIdx = parser.parseSourceFile();
    const parseTime = performance.now() - parseStart;

    // Get parse diagnostics
    const diagnosticsJson = parser.getDiagnosticsJson();
    const diagnostics = JSON.parse(diagnosticsJson);

    // Bind
    const bindStart = performance.now();
    if (useThinParser) {
        parser.bindSourceFile();
    } else {
        parser.bindSourceFile(rootIdx);
    }
    const bindTime = performance.now() - bindStart;

    // Type check
    const checkStart = performance.now();
    const checkJson = parser.checkSourceFile();
    const checkTime = performance.now() - checkStart;
    const checkResult = JSON.parse(checkJson);

    parser.free();

    return {
        success: true,
        parseErrors: diagnostics.length,
        typeErrors: checkResult.diagnostics?.length || 0,
        parseTime,
        bindTime,
        checkTime
    };
}

for (const file of files) {
    // Skip very large files
    const source = readSourceFile(file);
    if (source.length > 50000) {
        skipped++;
        continue;
    }

    const testName = basename(file, '.ts');
    const useThinParser = wasm.createThinParser !== undefined;

    // Check if this is a multi-file test
    const sourceLower = source.toLowerCase();
    const isMultiFile = sourceLower.includes('@filename:') || sourceLower.includes('// @filename');

    if (isMultiFile) {
        // Parse the multi-file test
        const { files: testFiles, headerLines } = parseMultiFileTest(source);

        // Filter to parseable files only
        const parseableFiles = testFiles.filter(f => isParseableFile(f.filename));

        if (parseableFiles.length === 0) {
            // No parseable files (e.g., only .json files)
            multiFileSkipped++;
            skipped++;
            continue;
        }

        try {
            let allPassed = true;
            let testParseErrors = 0;
            let testTypeErrors = 0;

            for (const testFile of parseableFiles) {
                const result = processFile(testFile.filename, testFile.content, useThinParser);
                testParseErrors += result.parseErrors;
                testTypeErrors += result.typeErrors;
                totalParseTime += result.parseTime;
                totalBindTime += result.bindTime;
                totalCheckTime += result.checkTime;
            }

            parseErrors += testParseErrors;
            typeErrors += testTypeErrors;
            multiFilePassed++;
            passed++;
        } catch (e) {
            failures.push({ file: testName, stage: 'crash', error: e.message, multiFile: true });
            multiFileFailed++;
            failed++;
        }
    } else {
        // Single file test
        try {
            const result = processFile(file, source, useThinParser);
            parseErrors += result.parseErrors;
            typeErrors += result.typeErrors;
            totalParseTime += result.parseTime;
            totalBindTime += result.bindTime;
            totalCheckTime += result.checkTime;
            passed++;
        } catch (e) {
            failures.push({ file: testName, stage: 'crash', error: e.message });
            failed++;
        }
    }
}

const tested = passed + failed;
const singleFilePassed = passed - multiFilePassed;
const singleFileFailed = failed - multiFileFailed;

console.log('=== Results ===\n');
console.log(`Tested:      ${tested}/${files.length} (skipped ${skipped} large/non-parseable)`);
console.log(`Passed:      ${passed}/${tested} (${(passed/tested*100).toFixed(1)}%)`);
console.log(`Failed:      ${failed}/${tested} (${(failed/tested*100).toFixed(1)}%)`);
console.log('');
console.log('=== Breakdown ===\n');
console.log(`Single-file: ${singleFilePassed} passed, ${singleFileFailed} failed`);
console.log(`Multi-file:  ${multiFilePassed} passed, ${multiFileFailed} failed`);
console.log(`Parse diagnostics: ${parseErrors} (expected for some tests)`);
console.log(`Type diagnostics:  ${typeErrors}`);
console.log('');
console.log('=== Timing ===\n');
console.log(`Parse total: ${totalParseTime.toFixed(2)}ms (${(totalParseTime/passed).toFixed(2)}ms/file)`);
console.log(`Bind total:  ${totalBindTime.toFixed(2)}ms (${(totalBindTime/passed).toFixed(2)}ms/file)`);
console.log(`Check total: ${totalCheckTime.toFixed(2)}ms (${(totalCheckTime/passed).toFixed(2)}ms/file)`);
console.log(`Total:       ${(totalParseTime+totalBindTime+totalCheckTime).toFixed(2)}ms`);

if (failures.length > 0) {
    console.log(`\n=== Failures (${failures.length}) ===\n`);
    const crashes = failures.filter(f => f.stage === 'crash');
    const parseFailures = failures.filter(f => f.stage === 'parse');
    console.log(`  Crashes: ${crashes.length}`);
    console.log(`  Parse failures: ${parseFailures.length}`);

    if (crashes.length > 0) {
        console.log('\n  Sample crashes (first 5):');
        crashes.slice(0, 5).forEach(f => {
            console.log(`    ${f.file}: ${f.error?.slice(0,100)}`);
        });
    }

    if (parseFailures.length > 0) {
        console.log('\n  Parse failures (first 10):');
        parseFailures.slice(0, 10).forEach(f => {
            console.log(`    ${f.file}: ${f.errors} errors`);
        });
    }
}
