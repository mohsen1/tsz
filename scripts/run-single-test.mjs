#!/usr/bin/env node
/**
 * Single Test Runner - Run individual TypeScript test cases against WASM compiler
 * 
 * This script allows you to run a single test file against the Rust/WASM implementation
 * of the TypeScript compiler and see detailed output and diagnostics.
 *
 * Usage:
 *   node scripts/run-single-test.mjs [test-file]
 *   node scripts/run-single-test.mjs tests/cases/compiler/2dArrays.ts
 *
 * Flags:
 *   --thin    Use Parser (high-performance, 16-byte nodes)
 *   --legacy  Use legacy ParserState (208-byte nodes)
 *   --verbose Show detailed parsing/checking output
 *
 * Example:
 *   node scripts/run-single-test.mjs tests/cases/conformance/types/typeParameters/typeArgumentLists/wrappedAndRecursiveConstraints.ts --thin --verbose
 */

import { readFileSync, existsSync } from 'fs';
import { basename, join } from 'path';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

// Parse flags
const args = process.argv.slice(2);
const useThin = args.includes('--thin');
const useLegacy = args.includes('--legacy');
const testFile = args.find(arg => !arg.startsWith('--'));

// Load WASM module
let wasm;
try {
    wasm = require('../pkg/wasm.js');
} catch (e) {
    console.error('Failed to load WASM module. Run: wasm-pack build wasm --target nodejs');
    console.error(e.message);
    process.exit(1);
}

if (!testFile) {
    console.log('Usage: node scripts/test-rust-compiler.mjs <test-file>');
    console.log('Example: node scripts/test-rust-compiler.mjs tests/cases/compiler/2dArrays.ts');
    process.exit(1);
}

if (!existsSync(testFile)) {
    console.error(`File not found: ${testFile}`);
    process.exit(1);
}

const source = readFileSync(testFile, 'utf-8');
const testName = basename(testFile, '.ts');

console.log(`\n=== Testing: ${testName} ===\n`);
console.log('Source:');
console.log(source);
console.log('\n---\n');

// Check for baseline files
const baselineDir = 'tests/baselines/reference';
const jsBaseline = join(baselineDir, `${testName}.js`);
const typesBaseline = join(baselineDir, `${testName}.types`);
const errorsBaseline = join(baselineDir, `${testName}.errors.txt`);

console.log('Baseline files:');
console.log(`  .js:     ${existsSync(jsBaseline) ? '✓' : '✗'}`);
console.log(`  .types:  ${existsSync(typesBaseline) ? '✓' : '✗'}`);
console.log(`  .errors: ${existsSync(errorsBaseline) ? '✓' : '✗'}`);
console.log('');

// Determine which parser to use (default: Parser)
const parserType = useLegacy ? 'legacy' : 'parser';
console.log(`=== Rust Compiler Output (${parserType} parser) ===\n`);

const startTime = performance.now();

let parser, rootIdx, diagnostics, binding, checkResult;
let parseTime, bindTime, checkTime;

if (useLegacy) {
    // Legacy parser (208-byte nodes)
    parser = wasm.createParser(testFile, source);
    rootIdx = parser.parseSourceFile();
    parseTime = performance.now() - startTime;

    console.log(`Parse time: ${parseTime.toFixed(2)}ms`);
    console.log(`Node count: ${parser.getNodeCount()}`);

    const diagnosticsJson = parser.getDiagnosticsJson();
    diagnostics = JSON.parse(diagnosticsJson);
    if (diagnostics.length > 0) {
        console.log(`\nParse errors (${diagnostics.length}):`);
        diagnostics.forEach((d, i) => {
            console.log(`  ${i + 1}. ${d.message} (${d.start}-${d.end})`);
        });
    }

    const bindStart = performance.now();
    const bindingJson = parser.bindSourceFile(rootIdx);
    bindTime = performance.now() - bindStart;
    binding = JSON.parse(bindingJson);

    console.log(`\nBind time: ${bindTime.toFixed(2)}ms`);
    console.log(`Symbols: ${Object.keys(binding).length}`);
    if (Object.keys(binding).length > 0) {
        console.log('  ' + Object.keys(binding).slice(0, 10).join(', ') + (Object.keys(binding).length > 10 ? '...' : ''));
    }

    const checkStart = performance.now();
    const checkJson = parser.checkSourceFile();
    checkTime = performance.now() - checkStart;
    checkResult = JSON.parse(checkJson);
} else {
    // Parser (16-byte nodes) - High performance path
    if (!wasm.Parser && !wasm.createParser) {
        console.error('Parser not available in WASM module. Rebuild with: wasm-pack build wasm --target nodejs');
        console.error('Falling back to legacy parser...\n');
        // Fall through to legacy
        parser = wasm.createParser(testFile, source);
        rootIdx = parser.parseSourceFile();
        parseTime = performance.now() - startTime;
        console.log(`Parse time: ${parseTime.toFixed(2)}ms`);
        console.log(`Node count: ${parser.getNodeCount()}`);
        diagnostics = JSON.parse(parser.getDiagnosticsJson());
        const bindStart = performance.now();
        binding = JSON.parse(parser.bindSourceFile(rootIdx));
        bindTime = performance.now() - bindStart;
        const checkStart = performance.now();
        checkResult = JSON.parse(parser.checkSourceFile());
        checkTime = performance.now() - checkStart;
    } else {
        parser = wasm.createParser ? wasm.createParser(testFile, source) : new wasm.Parser(testFile, source);
        rootIdx = parser.parseSourceFile();
        parseTime = performance.now() - startTime;

        console.log(`Parse time: ${parseTime.toFixed(2)}ms`);
        console.log(`Node count: ${parser.getNodeCount()}`);

        const diagnosticsJson = parser.getDiagnosticsJson();
        diagnostics = JSON.parse(diagnosticsJson);
        if (diagnostics.length > 0) {
            console.log(`\nParse errors (${diagnostics.length}):`);
            diagnostics.forEach((d, i) => {
                console.log(`  ${i + 1}. ${d.message} (${d.start}-${d.length})`);
            });
        }

        const bindStart = performance.now();
        const bindingJson = parser.bindSourceFile();  // No rootIdx param for Parser
        bindTime = performance.now() - bindStart;
        binding = JSON.parse(bindingJson);

        console.log(`\nBind time: ${bindTime.toFixed(2)}ms`);
        console.log(`Symbols: ${binding.symbolCount || 0}`);
        if (binding.symbols) {
            const symbolNames = Object.keys(binding.symbols);
            if (symbolNames.length > 0) {
                console.log('  ' + symbolNames.slice(0, 10).join(', ') + (symbolNames.length > 10 ? '...' : ''));
            }
        }

        const checkStart = performance.now();
        const checkJson = parser.checkSourceFile();
        checkTime = performance.now() - checkStart;
        checkResult = JSON.parse(checkJson);
    }
}

console.log(`\nCheck time: ${checkTime.toFixed(2)}ms`);
console.log(`Types created: ${checkResult.typeCount}`);
if (checkResult.diagnostics && checkResult.diagnostics.length > 0) {
    console.log(`Type errors (${checkResult.diagnostics.length}):`);
    checkResult.diagnostics.slice(0, 5).forEach((d, i) => {
        console.log(`  ${i + 1}. ${d.message}`);
    });
    if (checkResult.diagnostics.length > 5) {
        console.log(`  ... and ${checkResult.diagnostics.length - 5} more`);
    }
}

// Summary
const totalTime = performance.now() - startTime;
console.log(`\n=== Summary ===`);
console.log(`Parser: ${parserType}`);
console.log(`Total time: ${totalTime.toFixed(2)}ms`);
console.log(`Parse errors: ${diagnostics.length}`);
console.log(`Type errors: ${checkResult.diagnostics?.length || 0}`);

// === Baseline Comparison ===
console.log('\n=== Baseline Comparison ===');

/**
 * Parse expected error codes from a .errors.txt baseline file
 * Format: "file.ts(1,13): error TS1110: Type expected."
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
 * Compare two sorted arrays of error codes
 */
function compareErrorCodes(expected, actual) {
    const expectedSet = new Set(expected);
    const actualSet = new Set(actual);

    const missing = expected.filter(c => !actualSet.has(c));
    const extra = actual.filter(c => !expectedSet.has(c));
    const matched = expected.filter(c => actualSet.has(c));

    return { missing, extra, matched };
}

// Collect actual error codes from Rust output
const actualErrorCodes = [];
if (diagnostics.length > 0) {
    // Parse errors typically have codes in message or as separate field
    diagnostics.forEach(d => {
        if (d.code) actualErrorCodes.push(d.code);
    });
}
if (checkResult.diagnostics) {
    checkResult.diagnostics.forEach(d => {
        if (d.code) actualErrorCodes.push(d.code);
    });
}
actualErrorCodes.sort((a, b) => a - b);

// Compare against .errors.txt baseline
if (existsSync(errorsBaseline)) {
    const baselineContent = readFileSync(errorsBaseline, 'utf-8');
    const expectedCodes = parseExpectedErrors(baselineContent);
    const { missing, extra, matched } = compareErrorCodes(expectedCodes, actualErrorCodes);

    console.log(`\n.errors.txt comparison:`);
    console.log(`  Expected: ${expectedCodes.length} errors (codes: ${expectedCodes.slice(0, 10).join(', ')}${expectedCodes.length > 10 ? '...' : ''})`);
    console.log(`  Actual:   ${actualErrorCodes.length} errors (codes: ${actualErrorCodes.slice(0, 10).join(', ')}${actualErrorCodes.length > 10 ? '...' : ''})`);
    console.log(`  Matched:  ${matched.length}`);

    if (missing.length === 0 && extra.length === 0) {
        console.log(`  ✓ PASS - Error codes match!`);
    } else {
        if (missing.length > 0) {
            console.log(`  ✗ Missing: ${missing.join(', ')} (expected but not produced)`);
        }
        if (extra.length > 0) {
            console.log(`  ✗ Extra: ${extra.join(', ')} (produced but not expected)`);
        }
    }
} else {
    // No baseline means we expect no errors
    if (actualErrorCodes.length === 0) {
        console.log(`\n.errors.txt: ✓ No baseline, no errors produced (PASS)`);
    } else {
        console.log(`\n.errors.txt: ✗ No baseline exists, but produced ${actualErrorCodes.length} errors`);
        console.log(`  Codes: ${actualErrorCodes.join(', ')}`);
    }
}

// Compare against .js baseline (emit comparison)
if (existsSync(jsBaseline)) {
    const baselineJs = readFileSync(jsBaseline, 'utf-8');
    let emittedJs = '';
    try {
        emittedJs = parser.emit ? parser.emit() : '';
    } catch (e) {
        emittedJs = `[emit failed: ${e.message}]`;
    }

    console.log(`\n.js comparison:`);
    if (emittedJs && !emittedJs.startsWith('[emit failed')) {
        // Simple comparison - normalize whitespace
        const normalizeJs = (s) => s.replace(/\r\n/g, '\n').trim();
        const expectedNorm = normalizeJs(baselineJs);
        const actualNorm = normalizeJs(emittedJs);

        if (expectedNorm === actualNorm) {
            console.log(`  ✓ PASS - Emitted JS matches baseline!`);
        } else {
            console.log(`  ✗ FAIL - Emitted JS differs from baseline`);
            console.log(`  Expected length: ${expectedNorm.length}, Actual length: ${actualNorm.length}`);
        }
    } else {
        console.log(`  ⚠ Emit not available or failed`);
    }
} else {
    console.log(`\n.js: No baseline (declaration-only or other)`);
}

// Note about .types comparison
if (existsSync(typesBaseline)) {
    console.log(`\n.types: ⚠ Comparison not yet implemented (requires type export API)`);
} else {
    console.log(`\n.types: No baseline`);
}

// Cleanup
if (parser.free) parser.free();
