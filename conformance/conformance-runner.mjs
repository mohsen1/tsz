#!/usr/bin/env node
/**
 * Conformance Test Runner - Tests WASM compiler against TypeScript conformance tests
 */

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, resolve, basename } from 'path';
import { readFileSync, readdirSync, statSync } from 'fs';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CONFIG = {
  wasmPkgPath: resolve(__dirname, '../pkg'),
  conformanceDir: resolve(__dirname, '../TypeScript/tests/cases/conformance'),
};
const DEFAULT_LIB_PATH = resolve(__dirname, '../TypeScript/tests/lib/lib.d.ts');
const DEFAULT_LIB_SOURCE = readFileSync(DEFAULT_LIB_PATH, 'utf8');
const DEFAULT_LIB_NAME = 'lib.d.ts';

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

// Recursively get all .ts files
function getTestFiles(dir, maxFiles = 500) {
  const files = [];

  function walk(currentDir) {
    if (files.length >= maxFiles) return;

    const entries = readdirSync(currentDir);
    for (const entry of entries) {
      if (files.length >= maxFiles) break;

      const fullPath = join(currentDir, entry);
      const stat = statSync(fullPath);

      if (stat.isDirectory()) {
        walk(fullPath);
      } else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) {
        files.push(fullPath);
      }
    }
  }

  walk(dir);
  return files;
}

/**
 * Parse test directives from source code.
 * Returns { options: Object, isMultiFile: boolean, cleanCode: string, files: Array<{name, content}> }
 */
function parseTestDirectives(code) {
  const lines = code.split('\n');
  const options = {};
  let isMultiFile = false;
  const cleanLines = [];
  const files = []; // For multi-file tests: [{name: "a.ts", content: "..."}, ...]

  // First pass: extract options and detect multi-file
  let currentFileName = null;
  let currentFileLines = [];

  for (const line of lines) {
    const trimmed = line.trim();

    // Check for @filename directive (multi-file test)
    const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/);
    if (filenameMatch) {
      isMultiFile = true;
      // Save previous file if any
      if (currentFileName) {
        files.push({ name: currentFileName, content: currentFileLines.join('\n') });
      }
      currentFileName = filenameMatch[1].trim();
      currentFileLines = [];
      continue;
    }

    // Parse compiler options like // @strict: true
    const match = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/);
    if (match) {
      const [, key, value] = match;
      // Parse boolean/number values
      if (value === 'true') options[key.toLowerCase()] = true;
      else if (value === 'false') options[key.toLowerCase()] = false;
      else if (!isNaN(Number(value))) options[key.toLowerCase()] = Number(value);
      else options[key.toLowerCase()] = value;
      continue; // Don't include directive in clean code
    }

    if (isMultiFile && currentFileName) {
      currentFileLines.push(line);
    } else {
      cleanLines.push(line);
    }
  }

  // Save the last file for multi-file tests
  if (isMultiFile && currentFileName) {
    files.push({ name: currentFileName, content: currentFileLines.join('\n') });
  }

  return {
    options,
    isMultiFile,
    cleanCode: cleanLines.join('\n'),
    files,
  };
}

/**
 * Run tsc on a single file
 */
async function runTsc(code, fileName = 'test.ts', testOptions = {}) {
  const ts = require('typescript');

  // Build compiler options from test directives
  const compilerOptions = {
    strict: testOptions.strict !== false, // default true
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
  };

  // Apply test-specific options
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
    compilerOptions.target = targetMap[testOptions.target.toLowerCase()] || ts.ScriptTarget.ES2020;
  }

  if (testOptions.noimplicitany !== undefined) {
    compilerOptions.noImplicitAny = testOptions.noimplicitany;
  }
  if (testOptions.strictnullchecks !== undefined) {
    compilerOptions.strictNullChecks = testOptions.strictnullchecks;
  }
  if (testOptions.noimplicitreturns !== undefined) {
    compilerOptions.noImplicitReturns = testOptions.noimplicitreturns;
  }
  if (testOptions.noimplicitthis !== undefined) {
    compilerOptions.noImplicitThis = testOptions.noimplicitthis;
  }
  if (testOptions.strictfunctiontypes !== undefined) {
    compilerOptions.strictFunctionTypes = testOptions.strictfunctiontypes;
  }
  if (testOptions.strictpropertyinitialization !== undefined) {
    compilerOptions.strictPropertyInitialization = testOptions.strictpropertyinitialization;
  }

  const sourceFile = ts.createSourceFile(
    fileName,
    code,
    ts.ScriptTarget.ES2020,
    true
  );

  const host = ts.createCompilerHost(compilerOptions);
  const originalGetSourceFile = host.getSourceFile;
  host.getSourceFile = (name, languageVersion, onError) => {
    if (name === fileName) {
      return sourceFile;
    }
    return originalGetSourceFile.call(host, name, languageVersion, onError);
  };

  const program = ts.createProgram([fileName], compilerOptions, host);

  const allDiagnostics = [
    ...program.getSyntacticDiagnostics(sourceFile),
    ...program.getSemanticDiagnostics(sourceFile),
  ];

  return {
    diagnostics: allDiagnostics.map(d => ({
      code: d.code,
      message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
      category: ts.DiagnosticCategory[d.category],
    })),
  };
}

/**
 * Run tsc on multiple files (for multi-file tests)
 */
async function runTscMultiFile(files, testOptions = {}) {
  const ts = require('typescript');

  // Build compiler options from test directives
  const compilerOptions = {
    strict: testOptions.strict !== false,
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
  };

  // Apply test-specific options
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
    compilerOptions.target = targetMap[testOptions.target.toLowerCase()] || ts.ScriptTarget.ES2020;
  }

  if (testOptions.noimplicitany !== undefined) {
    compilerOptions.noImplicitAny = testOptions.noimplicitany;
  }
  if (testOptions.strictnullchecks !== undefined) {
    compilerOptions.strictNullChecks = testOptions.strictnullchecks;
  }
  if (testOptions.noimplicitreturns !== undefined) {
    compilerOptions.noImplicitReturns = testOptions.noimplicitreturns;
  }
  if (testOptions.noimplicitthis !== undefined) {
    compilerOptions.noImplicitThis = testOptions.noimplicitthis;
  }
  if (testOptions.strictfunctiontypes !== undefined) {
    compilerOptions.strictFunctionTypes = testOptions.strictfunctiontypes;
  }
  if (testOptions.strictpropertyinitialization !== undefined) {
    compilerOptions.strictPropertyInitialization = testOptions.strictpropertyinitialization;
  }

  // Create source files for all files
  const sourceFiles = new Map();
  const fileNames = [];
  for (const file of files) {
    const sf = ts.createSourceFile(file.name, file.content, ts.ScriptTarget.ES2020, true);
    sourceFiles.set(file.name, sf);
    fileNames.push(file.name);
  }

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
    const file = files.find(f => f.name === name);
    if (file) return file.content;
    return ts.sys.readFile(name);
  };

  const program = ts.createProgram(fileNames, compilerOptions, host);

  const allDiagnostics = [];
  for (const sf of sourceFiles.values()) {
    allDiagnostics.push(...program.getSyntacticDiagnostics(sf));
    allDiagnostics.push(...program.getSemanticDiagnostics(sf));
  }

  return {
    diagnostics: allDiagnostics.map(d => ({
      code: d.code,
      message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
      category: ts.DiagnosticCategory[d.category],
    })),
  };
}

async function runWasm(code, fileName = 'test.ts', testOptions = {}) {
  try {
    const wasmModule = await import(join(CONFIG.wasmPkgPath, 'wasm.js'));

    // WASM module auto-initializes when built with --target nodejs
    const parser = new wasmModule.ThinParser(fileName, code);
    if (!testOptions.nolib) {
      parser.addLibFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
    }
    parser.parseSourceFile();

    const parseDiagsJson = parser.getDiagnosticsJson();
    const parseDiags = JSON.parse(parseDiagsJson);

    const checkResultJson = parser.checkSourceFile();
    const checkResult = JSON.parse(checkResultJson);

    const allDiagnostics = [
      ...parseDiags.map(d => ({
        code: d.code,
        message: d.message,
        category: 'Error',
        source: 'parser',
      })),
      ...(checkResult.diagnostics || []).map(d => ({
        code: d.code,
        message: d.message_text,
        category: d.category,
        source: 'checker',
      })),
    ];

    parser.free();

    return {
      diagnostics: allDiagnostics,
      crashed: false,
    };
  } catch (e) {
    return {
      diagnostics: [],
      crashed: true,
      error: e.message,
    };
  }
}

/**
 * Run WASM on multiple files (for multi-file tests)
 * Uses the new WasmProgram API for cross-file type checking
 */
async function runWasmMultiFile(files, testOptions = {}) {
  try {
    const wasmModule = await import(join(CONFIG.wasmPkgPath, 'wasm.js'));

    // WASM module auto-initializes when built with --target nodejs
    // Use the new WasmProgram API for multi-file support
    const program = new wasmModule.WasmProgram();

    if (!testOptions.nolib) {
      program.addFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
    }

    // Add all files to the program
    for (const file of files) {
      program.addFile(file.name, file.content);
    }

    // Get all diagnostic codes from the program
    const codes = program.getAllDiagnosticCodes();

    // Convert to diagnostic format
    const allDiagnostics = Array.from(codes).map(code => ({
      code,
      message: '', // We don't have messages in this API
      category: 'Error',
      source: 'program',
    }));

    return {
      diagnostics: allDiagnostics,
      crashed: false,
    };
  } catch (e) {
    return {
      diagnostics: [],
      crashed: true,
      error: e.message,
    };
  }
}

function compareDiagnostics(tscResult, wasmResult) {
  const tscCodes = new Set(tscResult.diagnostics.map(d => d.code));
  const wasmCodes = new Set(wasmResult.diagnostics.map(d => d.code));

  const missingInWasm = [...tscCodes].filter(c => !wasmCodes.has(c));
  const extraInWasm = [...wasmCodes].filter(c => !tscCodes.has(c));

  // Perfect match
  const exactMatch = missingInWasm.length === 0 && extraInWasm.length === 0;

  // Same error count (might have different codes)
  const sameCount = tscResult.diagnostics.length === wasmResult.diagnostics.length;

  return {
    exactMatch,
    sameCount,
    tscCount: tscResult.diagnostics.length,
    wasmCount: wasmResult.diagnostics.length,
    missingInWasm,
    extraInWasm,
  };
}

async function main() {
  const args = process.argv.slice(2);
  const maxTests = parseInt(args.find(a => a.startsWith('--max='))?.split('=')[1] || '200', 10);
  const verbose = args.includes('--verbose') || args.includes('-v');
  const categoryArg = args.find(a => a.startsWith('--category='))?.split('=')[1];
  const categories = categoryArg ? categoryArg.split(',') : ['conformance'];

  log('Conformance Test Runner', colors.bold);
  log('═'.repeat(60), colors.dim);
  log(`  Categories: ${categories.join(', ')}`, colors.cyan);

  log(`\nCollecting test files (max ${maxTests})...`, colors.cyan);
  const testFiles = [];
  const testsBasePath = resolve(__dirname, '../../tests/cases');

  for (const category of categories) {
    const testDir = join(testsBasePath, category);
    const files = getTestFiles(testDir, maxTests - testFiles.length);
    testFiles.push(...files);
    log(`  ${category}: ${files.length} files`, colors.dim);
  }

  log(`  Total: ${testFiles.length} test files`, colors.dim);

  const stats = {
    total: 0,
    multiFile: 0,      // Multi-file tests (using WasmProgram API)
    exactMatch: 0,
    sameCount: 0,
    crashed: 0,
    missingErrors: 0,  // WASM missed errors TSC found
    extraErrors: 0,    // WASM found errors TSC didn't
    byCategory: {},
  };

  const missingCodeCounts = {};
  const extraCodeCounts = {};
  const crashedFiles = [];

  log(`\nRunning tests...`, colors.cyan);

  for (let i = 0; i < testFiles.length; i++) {
    const filePath = testFiles[i];
    const fileName = basename(filePath);
    const relPath = filePath.replace(testsBasePath + '/', '');
    const cat = relPath.split('/')[0];

    if (!verbose) {
      process.stdout.write(`\r  Progress: ${i + 1}/${testFiles.length}`);
    }

    try {
      const rawCode = readFileSync(filePath, 'utf-8');

      // Parse test directives
      const { options, isMultiFile, cleanCode, files } = parseTestDirectives(rawCode);

      let tscResult, wasmResult;

      if (isMultiFile && files.length > 0) {
        // Multi-file test - use the new APIs
        stats.multiFile++;
        [tscResult, wasmResult] = await Promise.all([
          runTscMultiFile(files, options),
          runWasmMultiFile(files, options),
        ]);
      } else {
        // Single-file test
        [tscResult, wasmResult] = await Promise.all([
          runTsc(cleanCode, fileName, options),
          runWasm(cleanCode, fileName, options),
        ]);
      }

      stats.total++;
      stats.byCategory[cat] = stats.byCategory[cat] || { total: 0, exact: 0, same: 0 };
      stats.byCategory[cat].total++;

      if (wasmResult.crashed) {
        stats.crashed++;
        crashedFiles.push({ file: relPath, error: wasmResult.error });
        continue;
      }

      const comparison = compareDiagnostics(tscResult, wasmResult);

      if (comparison.exactMatch) {
        stats.exactMatch++;
        stats.byCategory[cat].exact++;
      }

      if (comparison.sameCount) {
        stats.sameCount++;
        stats.byCategory[cat].same++;
      }

      if (comparison.missingInWasm.length > 0) {
        stats.missingErrors++;
        for (const code of comparison.missingInWasm) {
          missingCodeCounts[code] = (missingCodeCounts[code] || 0) + 1;
        }
      }

      if (comparison.extraInWasm.length > 0) {
        stats.extraErrors++;
        for (const code of comparison.extraInWasm) {
          extraCodeCounts[code] = (extraCodeCounts[code] || 0) + 1;
        }
      }

      if (verbose && !comparison.exactMatch) {
        log(`\n${relPath}:`, colors.yellow);
        log(`  TSC: ${comparison.tscCount} errors, WASM: ${comparison.wasmCount} errors`, colors.dim);
        if (comparison.missingInWasm.length > 0) {
          log(`  Missing: TS${comparison.missingInWasm.join(', TS')}`, colors.red);
        }
        if (comparison.extraInWasm.length > 0) {
          log(`  Extra: TS${comparison.extraInWasm.join(', TS')}`, colors.yellow);
        }
      }

    } catch (e) {
      // Skip files that can't be read
    }
  }

  if (!verbose) {
    console.log('');
  }

  // Print report
  log('\n' + '═'.repeat(60), colors.bold);
  log('  CONFORMANCE TEST REPORT', colors.bold);
  log('═'.repeat(60), colors.bold);

  log(`\n  Summary:`, colors.cyan);
  log(`    Files Found:      ${testFiles.length}`);
  log(`    Multi-File Tests:   ${stats.multiFile}`, colors.cyan);
  log(`    Tests Run:        ${stats.total}`);
  log(`    Exact Match:      ${stats.exactMatch} (${(stats.exactMatch / stats.total * 100).toFixed(1)}%)`, colors.green);
  log(`    Same Error Count: ${stats.sameCount} (${(stats.sameCount / stats.total * 100).toFixed(1)}%)`, colors.blue);
  log(`    WASM Crashed:     ${stats.crashed}`, stats.crashed > 0 ? colors.red : '');

  log(`\n  Parity Issues:`, colors.cyan);
  log(`    Tests with missing errors: ${stats.missingErrors} (${(stats.missingErrors / stats.total * 100).toFixed(1)}%)`, stats.missingErrors > 0 ? colors.red : colors.green);
  log(`    Tests with extra errors:   ${stats.extraErrors} (${(stats.extraErrors / stats.total * 100).toFixed(1)}%)`, stats.extraErrors > 0 ? colors.yellow : colors.green);

  if (Object.keys(missingCodeCounts).length > 0) {
    log(`\n  Most Common Missing Error Codes:`, colors.cyan);
    const sorted = Object.entries(missingCodeCounts).sort((a, b) => b[1] - a[1]).slice(0, 10);
    for (const [code, count] of sorted) {
      log(`    TS${code}: ${count} occurrences`, colors.red);
    }
  }

  if (Object.keys(extraCodeCounts).length > 0) {
    log(`\n  Most Common Extra Error Codes:`, colors.cyan);
    const sorted = Object.entries(extraCodeCounts).sort((a, b) => b[1] - a[1]).slice(0, 10);
    for (const [code, count] of sorted) {
      log(`    TS${code}: ${count} occurrences`, colors.yellow);
    }
  }

  if (Object.keys(stats.byCategory).length > 1) {
    log(`\n  By Category:`, colors.cyan);
    const sorted = Object.entries(stats.byCategory).sort((a, b) => b[1].total - a[1].total);
    for (const [cat, data] of sorted.slice(0, 15)) {
      const pct = (data.exact / data.total * 100).toFixed(0);
      const bar = '█'.repeat(Math.floor(pct / 5)) + '░'.repeat(20 - Math.floor(pct / 5));
      log(`    ${cat.padEnd(20)} ${bar} ${pct}% exact (${data.exact}/${data.total})`);
    }
  }

  if (crashedFiles.length > 0 && crashedFiles.length <= 10) {
    log(`\n  Crashed Files:`, colors.red);
    for (const { file, error } of crashedFiles) {
      log(`    ${file}: ${error?.slice(0, 80) || 'Unknown error'}`, colors.dim);
    }
  }

  log('\n' + '═'.repeat(60) + '\n', colors.bold);
}

main().catch(e => {
  console.error('Fatal error:', e);
  process.exit(1);
});
