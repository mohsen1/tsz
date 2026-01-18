#!/usr/bin/env node
/**
 * Standalone test to verify @strict directive handling
 *
 * Tests that the compiler correctly respects the strict: false directive
 * and produces different diagnostics than with strict: true.
 */

import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { readFileSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Test case: untyped variable declaration
// With strict: false -> should NOT error with TS7005 (implicitly has 'any' type)
// With strict: true -> SHOULD error with TS7005
const TEST_CODE = `let x;
x = 5;
`;

const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  bold: '\x1b[1m',
};

function log(msg, color = '') {
  console.log(`${color}${msg}${colors.reset}`);
}

async function main() {
  log('Strict Directive Test', colors.bold);
  log('='.repeat(60), colors.cyan);

  try {
    // Import WASM module (Node.js build auto-initializes)
    const wasmPkgPath = join(__dirname, '../pkg');
    const wasmModule = await import(join(wasmPkgPath, 'wasm.js'));

    log('\nTest: Variable declaration without type annotation', colors.cyan);
    log('Code:', colors.cyan);
    log(TEST_CODE);

    // Test 1: With strict: false
    log('\n1. Testing with strict: false (should NOT produce TS7005)', colors.cyan);
    let parser1 = new wasmModule.ThinParser('test.ts', TEST_CODE);
    parser1.setCompilerOptions(JSON.stringify({ strict: false }));
    parser1.parseSourceFile();

    const checkResult1Json = parser1.checkSourceFile();
    const checkResult1 = JSON.parse(checkResult1Json);
    const diags1 = checkResult1.diagnostics || [];

    log(`  Diagnostics found: ${diags1.length}`, diags1.length === 0 ? colors.green : colors.yellow);
    if (diags1.length > 0) {
      for (const diag of diags1) {
        log(`    TS${diag.code}: ${diag.message_text}`, colors.yellow);
      }
    }

    const hasTS7005_strict_false = diags1.some(d => d.code === 7005);
    parser1.free();

    // Test 2: With strict: true
    log('\n2. Testing with strict: true (SHOULD produce TS7005)', colors.cyan);
    let parser2 = new wasmModule.ThinParser('test.ts', TEST_CODE);
    parser2.setCompilerOptions(JSON.stringify({ strict: true }));
    parser2.parseSourceFile();

    const checkResult2Json = parser2.checkSourceFile();
    const checkResult2 = JSON.parse(checkResult2Json);
    const diags2 = checkResult2.diagnostics || [];

    log(`  Diagnostics found: ${diags2.length}`, diags2.length > 0 ? colors.green : colors.red);
    if (diags2.length > 0) {
      for (const diag of diags2) {
        log(`    TS${diag.code}: ${diag.message_text}`, colors.yellow);
      }
    }

    const hasTS7005_strict_true = diags2.some(d => d.code === 7005);
    parser2.free();

    // Verify results
    log('\n' + '='.repeat(60), colors.cyan);
    log('Results:', colors.bold);
    log(`  strict: false has TS7005: ${hasTS7005_strict_false}`, hasTS7005_strict_false ? colors.red : colors.green);
    log(`  strict: true has TS7005:  ${hasTS7005_strict_true}`, hasTS7005_strict_true ? colors.green : colors.red);

    // Test passes if:
    // - strict: false does NOT have TS7005
    // - strict: true DOES have TS7005
    const testPassed = !hasTS7005_strict_false && hasTS7005_strict_true;

    log('\n' + '='.repeat(60), colors.cyan);
    if (testPassed) {
      log('PASS: @strict directive handling works correctly', colors.green + colors.bold);
      process.exit(0);
    } else {
      log('FAIL: @strict directive handling is incorrect', colors.red + colors.bold);
      if (hasTS7005_strict_false) {
        log('  Issue: strict: false incorrectly produces TS7005', colors.red);
      }
      if (!hasTS7005_strict_true) {
        log('  Issue: strict: true does not produce TS7005', colors.red);
      }
      process.exit(1);
    }

  } catch (e) {
    log('\n' + '='.repeat(60), colors.cyan);
    log('FAIL: Test crashed with error', colors.red + colors.bold);
    log(`Error: ${e.message}`, colors.red);
    if (e.stack) {
      log(`\nStack trace:\n${e.stack}`, colors.red);
    }
    process.exit(1);
  }
}

main().catch(e => {
  console.error('Fatal error:', e);
  process.exit(1);
});
