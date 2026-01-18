#!/usr/bin/env node
/**
 * Test suite for mapToCompilerOptions with actual TypeScript module
 *
 * Verifies that parsed directives are correctly mapped to TypeScript
 * ScriptTarget, ModuleKind, and other enum values.
 */

import { createRequire } from 'module';
import { parseTestDirectives, mapToCompilerOptions } from './directive-parser.mjs';

const require = createRequire(import.meta.url);
const ts = require('typescript');

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
  if (actual !== expected) {
    throw new Error(`${message}\n    Expected: ${expected}\n    Actual:   ${actual}`);
  }
}

log('\nDirective to TypeScript Compiler Options Mapping', colors.bold);
log('═'.repeat(60), colors.cyan);

// Test Target mapping
log('\n1. Target Mapping', colors.cyan);

test('@target: es5 maps to ES5', () => {
  const { options } = parseTestDirectives('// @target: es5\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.target, ts.ScriptTarget.ES5);
});

test('@target: es2015 maps to ES2015', () => {
  const { options } = parseTestDirectives('// @target: es2015\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.target, ts.ScriptTarget.ES2015);
});

test('@target: es6 maps to ES2015 (alias)', () => {
  const { options } = parseTestDirectives('// @target: es6\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.target, ts.ScriptTarget.ES2015);
});

test('@target: es2020 maps to ES2020', () => {
  const { options } = parseTestDirectives('// @target: es2020\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.target, ts.ScriptTarget.ES2020);
});

test('@target: esnext maps to ESNext', () => {
  const { options } = parseTestDirectives('// @target: esnext\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.target, ts.ScriptTarget.ESNext);
});

// Test Module mapping
log('\n2. Module Mapping', colors.cyan);

test('@module: commonjs maps to CommonJS', () => {
  const { options } = parseTestDirectives('// @module: commonjs\nimport x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.module, ts.ModuleKind.CommonJS);
});

test('@module: esnext maps to ESNext', () => {
  const { options } = parseTestDirectives('// @module: esnext\nimport x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.module, ts.ModuleKind.ESNext);
});

test('@module: es2020 maps to ES2020', () => {
  const { options } = parseTestDirectives('// @module: es2020\nimport x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.module, ts.ModuleKind.ES2020);
});

if (ts.ModuleKind.Node16) {
  test('@module: node16 maps to Node16', () => {
    const { options } = parseTestDirectives('// @module: node16\nimport x;');
    const compilerOptions = mapToCompilerOptions(options, ts);
    assertEqual(compilerOptions.module, ts.ModuleKind.Node16);
  });
}

if (ts.ModuleKind.NodeNext) {
  test('@module: nodenext maps to NodeNext', () => {
    const { options } = parseTestDirectives('// @module: nodenext\nimport x;');
    const compilerOptions = mapToCompilerOptions(options, ts);
    assertEqual(compilerOptions.module, ts.ModuleKind.NodeNext);
  });
}

// Test Module Resolution mapping
log('\n3. Module Resolution Mapping', colors.cyan);

test('@moduleResolution: node maps correctly', () => {
  const { options } = parseTestDirectives('// @moduleResolution: node\nimport x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  // Node10 is the newer name for the classic Node resolution
  const expectedValue = ts.ModuleResolutionKind.Node10 ?? ts.ModuleResolutionKind.NodeJs;
  assertEqual(compilerOptions.moduleResolution, expectedValue);
});

if (ts.ModuleResolutionKind.Bundler) {
  test('@moduleResolution: bundler maps to Bundler', () => {
    const { options } = parseTestDirectives('// @moduleResolution: bundler\nimport x;');
    const compilerOptions = mapToCompilerOptions(options, ts);
    assertEqual(compilerOptions.moduleResolution, ts.ModuleResolutionKind.Bundler);
  });
}

// Test JSX mapping
log('\n4. JSX Mapping', colors.cyan);

test('@jsx: preserve maps to Preserve', () => {
  const { options } = parseTestDirectives('// @jsx: preserve\nconst x = <div/>;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.jsx, ts.JsxEmit.Preserve);
});

test('@jsx: react maps to React', () => {
  const { options } = parseTestDirectives('// @jsx: react\nconst x = <div/>;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.jsx, ts.JsxEmit.React);
});

if (ts.JsxEmit.ReactJSX) {
  test('@jsx: react-jsx maps to ReactJSX', () => {
    const { options } = parseTestDirectives('// @jsx: react-jsx\nconst x = <div/>;');
    const compilerOptions = mapToCompilerOptions(options, ts);
    assertEqual(compilerOptions.jsx, ts.JsxEmit.ReactJSX);
  });
}

// Test strict options mapping
log('\n5. Strict Mode Options Mapping', colors.cyan);

test('@strict: true maps to strict: true', () => {
  const { options } = parseTestDirectives('// @strict: true\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.strict, true);
});

test('@noImplicitAny: true maps correctly', () => {
  const { options } = parseTestDirectives('// @noImplicitAny: true\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.noImplicitAny, true);
});

test('@strictNullChecks: false maps correctly', () => {
  const { options } = parseTestDirectives('// @strictNullChecks: false\nlet x;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.strictNullChecks, false);
});

test('@noImplicitReturns: true maps correctly', () => {
  const { options } = parseTestDirectives('// @noImplicitReturns: true\nfunction f() {}');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.noImplicitReturns, true);
});

test('@noImplicitThis: true maps correctly', () => {
  const { options } = parseTestDirectives('// @noImplicitThis: true\nconst f = function() { this; };');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.noImplicitThis, true);
});

// Test decorator options
log('\n6. Decorator Options Mapping', colors.cyan);

test('@experimentalDecorators: true maps correctly', () => {
  const { options } = parseTestDirectives('// @experimentalDecorators: true\n@dec class A {}');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.experimentalDecorators, true);
});

test('@emitDecoratorMetadata: true maps correctly', () => {
  const { options } = parseTestDirectives('// @emitDecoratorMetadata: true\n@dec class A {}');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.emitDecoratorMetadata, true);
});

// Test emit options
log('\n7. Emit Options Mapping', colors.cyan);

test('@declaration: true maps correctly', () => {
  const { options } = parseTestDirectives('// @declaration: true\nexport const x = 1;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.declaration, true);
});

test('@noEmit: true maps correctly', () => {
  const { options } = parseTestDirectives('// @noEmit: true\nlet x = 1;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.noEmit, true);
});

test('@sourceMap: true maps correctly', () => {
  const { options } = parseTestDirectives('// @sourceMap: true\nlet x = 1;');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.sourceMap, true);
});

// Test module interop options
log('\n8. Module Interop Options Mapping', colors.cyan);

test('@esModuleInterop: true maps correctly', () => {
  const { options } = parseTestDirectives('// @esModuleInterop: true\nimport x from "x";');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.esModuleInterop, true);
});

test('@allowSyntheticDefaultImports: true maps correctly', () => {
  const { options } = parseTestDirectives('// @allowSyntheticDefaultImports: true\nimport x from "x";');
  const compilerOptions = mapToCompilerOptions(options, ts);
  assertEqual(compilerOptions.allowSyntheticDefaultImports, true);
});

// Test combined directives
log('\n9. Combined Directives', colors.cyan);

test('Multiple directives are all mapped correctly', () => {
  const code = `// @strict: true
// @target: es2020
// @module: esnext
// @noImplicitAny: false
// @experimentalDecorators: true
let x;`;
  const { options } = parseTestDirectives(code);
  const compilerOptions = mapToCompilerOptions(options, ts);

  assertEqual(compilerOptions.strict, true, 'strict should be true');
  assertEqual(compilerOptions.target, ts.ScriptTarget.ES2020, 'target should be ES2020');
  assertEqual(compilerOptions.module, ts.ModuleKind.ESNext, 'module should be ESNext');
  assertEqual(compilerOptions.noImplicitAny, false, 'noImplicitAny should be false');
  assertEqual(compilerOptions.experimentalDecorators, true, 'experimentalDecorators should be true');
});

// Summary
log('\n' + '═'.repeat(60), colors.cyan);
log(`\nTest Results: ${passed} passed, ${failed} failed`, failed > 0 ? colors.red : colors.green);

if (failed > 0) {
  process.exit(1);
}
