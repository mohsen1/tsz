#!/usr/bin/env node
/**
 * Test suite for directive-parser.mjs
 *
 * Verifies that @ directives are correctly parsed from test files
 * and mapped to TypeScript compiler options.
 */

import { parseTestDirectives, mapToWasmCompilerOptions } from './directive-parser.mjs';

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

let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    passed++;
    log(`  PASS: ${name}`, colors.green);
  } catch (e) {
    failed++;
    log(`  FAIL: ${name}`, colors.red);
    log(`        ${e.message}`, colors.red);
  }
}

function assertEqual(actual, expected, message = '') {
  const actualStr = JSON.stringify(actual);
  const expectedStr = JSON.stringify(expected);
  if (actualStr !== expectedStr) {
    throw new Error(`${message}\n    Expected: ${expectedStr}\n    Actual:   ${actualStr}`);
  }
}

function assertTrue(condition, message = 'Expected true') {
  if (!condition) {
    throw new Error(message);
  }
}

function assertFalse(condition, message = 'Expected false') {
  if (condition) {
    throw new Error(message);
  }
}

log('\nDirective Parser Test Suite', colors.bold);
log('═'.repeat(60), colors.cyan);

// Test 1: Parse @strict: true
log('\n1. Boolean Directives', colors.cyan);
test('@strict: true is parsed correctly', () => {
  const { options } = parseTestDirectives('// @strict: true\nlet x = 1;');
  assertEqual(options.strict, true);
});

test('@strict: false is parsed correctly', () => {
  const { options } = parseTestDirectives('// @strict: false\nlet x = 1;');
  assertEqual(options.strict, false);
});

test('@noImplicitAny: true is parsed (case insensitive)', () => {
  const { options } = parseTestDirectives('// @noImplicitAny: true\nlet x;');
  assertEqual(options.noimplicitany, true);
});

// Test 2: Parse @target directive
log('\n2. Target Directive', colors.cyan);
test('@target: es5 is parsed correctly', () => {
  const { options } = parseTestDirectives('// @target: es5\nlet x = 1;');
  assertEqual(options.target, 'es5');
});

test('@target: ES2020 is parsed correctly (case preserved)', () => {
  const { options } = parseTestDirectives('// @target: ES2020\nlet x = 1;');
  assertEqual(options.target, 'ES2020');
});

test('@target: esnext is parsed correctly', () => {
  const { options } = parseTestDirectives('// @target: esnext\nlet x = 1;');
  assertEqual(options.target, 'esnext');
});

// Test 3: Parse @noImplicitAny directive
log('\n3. NoImplicitAny Directive', colors.cyan);
test('@noImplicitAny: true is parsed correctly', () => {
  const { options } = parseTestDirectives('// @noImplicitAny: true\nlet x;');
  assertEqual(options.noimplicitany, true);
});

test('@noImplicitAny: false is parsed correctly', () => {
  const { options } = parseTestDirectives('// @noImplicitAny: false\nlet x;');
  assertEqual(options.noimplicitany, false);
});

// Test 4: Parse @lib directive (comma-separated)
log('\n4. Lib Directive (Comma-Separated)', colors.cyan);
test('@lib: es2020,dom is parsed as array', () => {
  const { options } = parseTestDirectives('// @lib: es2020,dom\nlet x;');
  assertTrue(Array.isArray(options.lib), 'lib should be an array');
  assertEqual(options.lib, ['es2020', 'dom']);
});

test('Multiple @lib directives are accumulated', () => {
  const { options } = parseTestDirectives('// @lib: es2020\n// @lib: dom\nlet x;');
  assertTrue(Array.isArray(options.lib), 'lib should be an array');
  assertEqual(options.lib, ['es2020', 'dom']);
});

// Test 5: Parse @module directive
log('\n5. Module Directive', colors.cyan);
test('@module: commonjs is parsed correctly', () => {
  const { options } = parseTestDirectives('// @module: commonjs\nimport x from "x";');
  assertEqual(options.module, 'commonjs');
});

test('@module: esnext is parsed correctly', () => {
  const { options } = parseTestDirectives('// @module: esnext\nimport x from "x";');
  assertEqual(options.module, 'esnext');
});

// Test 6: Parse @filename directive (multi-file)
log('\n6. Multi-file Tests (@filename)', colors.cyan);
test('@filename directive marks multi-file test', () => {
  const code = `// @strict: true
// @filename: a.ts
export const x = 1;
// @filename: b.ts
import { x } from "./a";
`;
  const { isMultiFile, files, options } = parseTestDirectives(code);
  assertTrue(isMultiFile, 'Should be multi-file');
  assertEqual(files.length, 2, 'Should have 2 files');
  assertEqual(files[0].name, 'a.ts');
  assertEqual(files[1].name, 'b.ts');
  assertTrue(files[0].content.includes('export const x'), 'First file should contain export');
  assertTrue(files[1].content.includes('import { x }'), 'Second file should contain import');
  assertEqual(options.strict, true, 'Options should still be parsed');
});

// Test 7: Clean code extraction
log('\n7. Clean Code Extraction', colors.cyan);
test('Directives are removed from clean code', () => {
  const { cleanCode } = parseTestDirectives('// @strict: true\nlet x = 1;');
  assertFalse(cleanCode.includes('@strict'), 'Clean code should not contain directive');
  assertTrue(cleanCode.includes('let x = 1'), 'Clean code should contain source');
});

test('Non-directive comments are preserved', () => {
  const { cleanCode } = parseTestDirectives('// @strict: true\n// This is a comment\nlet x = 1;');
  assertTrue(cleanCode.includes('// This is a comment'), 'Regular comments should be preserved');
});

// Test 8: Numeric values
log('\n8. Numeric Values', colors.cyan);
test('Numeric values are parsed as numbers', () => {
  const { options } = parseTestDirectives('// @maxNodeModuleJsDepth: 5\nlet x;');
  assertEqual(options.maxnodemodulejsdepth, 5);
  assertEqual(typeof options.maxnodemodulejsdepth, 'number');
});

// Test 9: mapToWasmCompilerOptions
log('\n9. mapToWasmCompilerOptions', colors.cyan);
test('Maps parsed options to WASM format', () => {
  const parsed = {
    strict: true,
    noimplicitany: false,
    target: 'es2020',
    module: 'esnext',
  };
  const wasmOptions = mapToWasmCompilerOptions(parsed);
  assertEqual(wasmOptions.strict, true);
  assertEqual(wasmOptions.noImplicitAny, false);
  assertEqual(wasmOptions.target, 'es2020');
  assertEqual(wasmOptions.module, 'esnext');
});

// Test 10: Additional strict mode options
log('\n10. Additional Strict Mode Options', colors.cyan);
test('@strictNullChecks is parsed correctly', () => {
  const { options } = parseTestDirectives('// @strictNullChecks: true\nlet x;');
  assertEqual(options.strictnullchecks, true);
});

test('@strictFunctionTypes is parsed correctly', () => {
  const { options } = parseTestDirectives('// @strictFunctionTypes: false\nlet x;');
  assertEqual(options.strictfunctiontypes, false);
});

test('@strictPropertyInitialization is parsed correctly', () => {
  const { options } = parseTestDirectives('// @strictPropertyInitialization: true\nlet x;');
  assertEqual(options.strictpropertyinitialization, true);
});

test('@strictBindCallApply is parsed correctly', () => {
  const { options } = parseTestDirectives('// @strictBindCallApply: true\nlet x;');
  assertEqual(options.strictbindcallapply, true);
});

// Test 11: Module resolution options
log('\n11. Module Resolution Options', colors.cyan);
test('@moduleResolution: node is parsed correctly', () => {
  const { options } = parseTestDirectives('// @moduleResolution: node\nimport x;');
  assertEqual(options.moduleresolution, 'node');
});

test('@moduleResolution: bundler is parsed correctly', () => {
  const { options } = parseTestDirectives('// @moduleResolution: bundler\nimport x;');
  assertEqual(options.moduleresolution, 'bundler');
});

// Test 12: JSX options
log('\n12. JSX Options', colors.cyan);
test('@jsx: preserve is parsed correctly', () => {
  const { options } = parseTestDirectives('// @jsx: preserve\nconst x = <div/>;');
  assertEqual(options.jsx, 'preserve');
});

test('@jsx: react-jsx is parsed correctly', () => {
  const { options } = parseTestDirectives('// @jsx: react-jsx\nconst x = <div/>;');
  assertEqual(options.jsx, 'react-jsx');
});

// Test 13: Experimental options
log('\n13. Experimental Options', colors.cyan);
test('@experimentalDecorators is parsed correctly', () => {
  const { options } = parseTestDirectives('// @experimentalDecorators: true\n@dec class A {}');
  assertEqual(options.experimentaldecorators, true);
});

test('@emitDecoratorMetadata is parsed correctly', () => {
  const { options } = parseTestDirectives('// @emitDecoratorMetadata: true\n@dec class A {}');
  assertEqual(options.emitdecoratormetadata, true);
});

// Test 14: esModuleInterop and related
log('\n14. Module Interop Options', colors.cyan);
test('@esModuleInterop is parsed correctly', () => {
  const { options } = parseTestDirectives('// @esModuleInterop: true\nimport x from "x";');
  assertEqual(options.esmoduleinterop, true);
});

test('@allowSyntheticDefaultImports is parsed correctly', () => {
  const { options } = parseTestDirectives('// @allowSyntheticDefaultImports: true\nimport x;');
  assertEqual(options.allowsyntheticdefaultimports, true);
});

test('@verbatimModuleSyntax is parsed correctly', () => {
  const { options } = parseTestDirectives('// @verbatimModuleSyntax: true\nimport x;');
  assertEqual(options.verbatimmodulesyntax, true);
});

// Test 15: Directive without value (treated as true)
log('\n15. Directives Without Values', colors.cyan);
test('@strict without value is treated as true', () => {
  const { options } = parseTestDirectives('// @strict\nlet x;');
  assertEqual(options.strict, true);
});

// Summary
log('\n' + '═'.repeat(60), colors.cyan);
log(`\nTest Results: ${passed} passed, ${failed} failed`, failed > 0 ? colors.red : colors.green);

if (failed > 0) {
  process.exit(1);
}
