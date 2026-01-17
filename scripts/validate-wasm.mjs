#!/usr/bin/env node
/**
 * Test WASM implementation against TypeScript compiler test suite
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const rootDir = path.resolve(__dirname, '..', '..');
const testsDir = path.join(rootDir, 'tests/cases/compiler');

// Check if WASM module is available
let wasmModule;
try {
    const wasmPath = path.join(rootDir, 'built/local/wasm.js');
    if (fs.existsSync(wasmPath)) {
        wasmModule = await import(wasmPath);
        console.log('WASM module loaded successfully\n');
    } else {
        console.log('WASM module not found at:', wasmPath);
        console.log('Build WASM first: docker run --rm -v path/to/wasm:/wasm rust:latest ...');
        process.exit(1);
    }
} catch (e) {
    console.error('Failed to load WASM module:', e.message);
    process.exit(1);
}

// Get all test files
const allTestFiles = fs.readdirSync(testsDir).filter(f => f.endsWith('.ts'));
const maxTests = parseInt(process.argv[2]) || 50;
const testFiles = allTestFiles.slice(0, maxTests);

console.log(`Running ${testFiles.length} of ${allTestFiles.length} compiler tests...\n`);

let passed = 0;
let failed = 0;
let skipped = 0;
const passedTests = [];
const failedTests = [];

for (const file of testFiles) {
    const filePath = path.join(testsDir, file);
    const content = fs.readFileSync(filePath, 'utf-8');
    
    try {
        const parser = wasmModule.createParser(file, content);
        if (!parser) {
            skipped++;
            continue;
        }
        
        parser.parseSourceFile();
        const result = parser.checkSourceFile();
        parser.free();
        
        const parsed = JSON.parse(result);
        if (parsed.error) {
            failedTests.push({ file, error: parsed.error });
            failed++;
        } else {
            passedTests.push({ file, diagnostics: parsed.diagnostics.length, types: parsed.typeCount });
            passed++;
        }
    } catch (e) {
        failedTests.push({ file, error: e.message });
        failed++;
    }
}

console.log(`=== Summary ===`);
console.log(`Passed: ${passed}/${testFiles.length} (${(passed/testFiles.length*100).toFixed(1)}%)`);
console.log(`Failed: ${failed}`);
console.log(`Skipped: ${skipped}`);

if (failedTests.length > 0 && failedTests.length <= 20) {
    console.log(`\n=== Failed Tests ===`);
    for (const t of failedTests.slice(0, 10)) {
        console.log(`  ${t.file}: ${t.error.substring(0, 60)}`);
    }
}

console.log(`\n=== Sample Passed Tests ===`);
for (const t of passedTests.slice(0, 10)) {
    console.log(`  ${t.file}: ${t.diagnostics} diags, ${t.types} types`);
}
