#!/usr/bin/env node
/**
 * Embedded conformance test runner - can be imported as a module
 * Returns test results for metrics tracking
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
  conformanceDir: resolve(__dirname, '../../tests/cases/conformance'),
};
const DEFAULT_LIB_PATH = resolve(__dirname, '../../tests/lib/lib.d.ts');
const DEFAULT_LIB_SOURCE = readFileSync(DEFAULT_LIB_PATH, 'utf8');
const DEFAULT_LIB_NAME = 'lib.d.ts';

/**
 * Recursively get all .ts files
 */
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
 * Parse test directives from source code
 */
function parseTestDirectives(code) {
  const lines = code.split('\n');
  const options = {};
  let isMultiFile = false;
  const cleanLines = [];
  const files = [];
  let currentFileName = null;
  let currentFileLines = [];

  for (const line of lines) {
    const trimmed = line.trim();

    const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/);
    if (filenameMatch) {
      isMultiFile = true;
      if (currentFileName) {
        files.push({ name: currentFileName, content: currentFileLines.join('\n') });
      }
      currentFileName = filenameMatch[1].trim();
      currentFileLines = [];
      continue;
    }

    const match = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/);
    if (match) {
      const [, key, value] = match;
      if (value === 'true') options[key.toLowerCase()] = true;
      else if (value === 'false') options[key.toLowerCase()] = false;
      else if (!isNaN(Number(value))) options[key.toLowerCase()] = Number(value);
      else options[key.toLowerCase()] = value;
      continue;
    }

    if (isMultiFile && currentFileName) {
      currentFileLines.push(line);
    } else {
      cleanLines.push(line);
    }
  }

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

  const compilerOptions = {
    strict: testOptions.strict !== false,
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
  };

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

  const sourceFile = ts.createSourceFile(fileName, code, ts.ScriptTarget.ES2020, true);

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
 * Run WASM on a single file
 */
async function runWasm(code, fileName = 'test.ts', testOptions = {}) {
  try {
    const wasm = await import(join(CONFIG.wasmPkgPath, 'wasm.js'));

    const parser = new wasm.ThinParser(fileName, code);
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
 * Compare diagnostics between TSC and WASM
 */
function compareDiagnostics(tscResult, wasmResult) {
  const tscCodes = new Set(tscResult.diagnostics.map(d => d.code));
  const wasmCodes = new Set(wasmResult.diagnostics.map(d => d.code));

  const missingInWasm = [...tscCodes].filter(c => !wasmCodes.has(c));
  const extraInWasm = [...wasmCodes].filter(c => !tscCodes.has(c));

  const exactMatch = missingInWasm.length === 0 && extraInWasm.length === 0;
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

/**
 * Run conformance tests and return metrics
 * Can be called programmatically or as main
 */
async function runTests(options = {}) {
  const { maxTests = 200, category = null, verbose = false } = options;

  let testDir = CONFIG.conformanceDir;
  if (category) {
    testDir = join(CONFIG.conformanceDir, category);
  }

  const testFiles = getTestFiles(testDir, maxTests);

  const stats = {
    total: 0,
    exactMatch: 0,
    sameCount: 0,
    crashed: 0,
    missingErrors: 0,
    extraErrors: 0,
    byCategory: {},
  };

  const missingCodeCounts = {};
  const extraCodeCounts = {};

  for (let i = 0; i < testFiles.length; i++) {
    const filePath = testFiles[i];
    const fileName = basename(filePath);
    const relPath = filePath.replace(CONFIG.conformanceDir + '/', '');
    const cat = relPath.split('/')[0];

    try {
      const rawCode = readFileSync(filePath, 'utf-8');
      const { options, isMultiFile, cleanCode } = parseTestDirectives(rawCode);

      // Skip multi-file tests for now (would need WasmProgram API)
      if (isMultiFile) continue;

      const [tscResult, wasmResult] = await Promise.all([
        runTsc(cleanCode, fileName, options),
        runWasm(cleanCode, fileName, options),
      ]);

      stats.total++;
      stats.byCategory[cat] = stats.byCategory[cat] || { total: 0, exact: 0, same: 0 };
      stats.byCategory[cat].total++;

      if (wasmResult.crashed) {
        stats.crashed++;
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

    } catch (e) {
      // Skip files that can't be read
    }
  }

  // Calculate percentages
  const exactMatchPercent = stats.total > 0 ? (stats.exactMatch / stats.total * 100) : 0;
  const sameCountPercent = stats.total > 0 ? (stats.sameCount / stats.total * 100) : 0;
  const missingErrorsPercent = stats.total > 0 ? (stats.missingErrors / stats.total * 100) : 0;
  const extraErrorsPercent = stats.total > 0 ? (stats.extraErrors / stats.total * 100) : 0;

  return {
    totalTests: stats.total,
    exactMatch: stats.exactMatch,
    exactMatchPercent: parseFloat(exactMatchPercent.toFixed(2)),
    sameCount: stats.sameCount,
    sameCountPercent: parseFloat(sameCountPercent.toFixed(2)),
    missingErrors: stats.missingErrors,
    missingErrorsPercent: parseFloat(missingErrorsPercent.toFixed(2)),
    extraErrors: stats.extraErrors,
    extraErrorsPercent: parseFloat(extraErrorsPercent.toFixed(2)),
    crashed: stats.crashed,
    missingCodeCounts,
    extraCodeCounts,
    byCategory: stats.byCategory,
  };
}

// Export for module use
export { runTests };

// Allow running directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const maxTests = parseInt(args.find(a => a.startsWith('--max='))?.split('=')[1] || '200', 10);
  const category = args.find(a => !a.startsWith('-'))?.toLowerCase();

  runTests({ maxTests, category, verbose: true }).then(results => {
    console.log('\nConformance Test Results:');
    console.log(`  Total Tests:     ${results.totalTests}`);
    console.log(`  Exact Match:     ${results.exactMatch} (${results.exactMatchPercent}%)`);
    console.log(`  Same Count:      ${results.sameCount} (${results.sameCountPercent}%)`);
    console.log(`  Missing Errors:  ${results.missingErrors} (${results.missingErrorsPercent}%)`);
    console.log(`  Extra Errors:    ${results.extraErrors} (${results.extraErrorsPercent}%)`);
    console.log(`  Crashed:         ${results.crashed}`);
  }).catch(e => {
    console.error('Error:', e);
    process.exit(1);
  });
}
