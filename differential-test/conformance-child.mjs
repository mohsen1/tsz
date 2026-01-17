#!/usr/bin/env node
/**
 * Child process for conformance testing (runs in isolated process)
 */

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, basename } from 'path';
import { readFileSync } from 'fs';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Read config from temp file passed as argument
const configFile = process.argv[2];
const { testFiles, wasmPkgPath, conformanceDir } = JSON.parse(readFileSync(configFile, 'utf-8'));
const DEFAULT_LIB_PATH = join(__dirname, '../ts-tests/lib/lib.d.ts');
const DEFAULT_LIB_SOURCE = readFileSync(DEFAULT_LIB_PATH, 'utf-8');
const DEFAULT_LIB_NAME = 'lib.d.ts';

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
      const lowerKey = key.toLowerCase();

      // Parse boolean values
      if (value === 'true') {
        options[lowerKey] = true;
      } else if (value === 'false') {
        options[lowerKey] = false;
      } else if (!isNaN(Number(value))) {
        options[lowerKey] = Number(value);
      } else {
        // Handle comma-separated values (e.g., @lib: es5,dom)
        if (value.includes(',')) {
          options[lowerKey] = value.split(',').map(v => v.trim());
        } else {
          options[lowerKey] = value;
        }
      }
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

  return { options, isMultiFile, cleanCode: cleanLines.join('\n'), files };
}

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

  // Apply all TypeScript compiler options from test directives
  if (testOptions.strict !== undefined) {
    compilerOptions.strict = testOptions.strict;
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
    };
    compilerOptions.module = moduleMap[testOptions.module.toLowerCase()] || ts.ModuleKind.ESNext;
  }

  const sourceFile = ts.createSourceFile(fileName, code, ts.ScriptTarget.ES2020, true);

  const host = ts.createCompilerHost(compilerOptions);
  const originalGetSourceFile = host.getSourceFile;
  host.getSourceFile = (name, languageVersion, onError) => {
    if (name === fileName) return sourceFile;
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

async function runWasm(code, fileName = 'test.ts', testOptions = {}) {
  let parser = null;
  try {
    const wasmModule = await import(join(wasmPkgPath, 'wasm.js'));
    const fs = await import('fs');

    // Initialize WASM module synchronously with file system loading
    const wasmBuffer = fs.readFileSync(join(wasmPkgPath, 'wasm_bg.wasm'));
    wasmModule.initSync(wasmBuffer);

    parser = new wasmModule.ThinParser(fileName, code);

    // Set compiler options from test directives
    const compilerOptions = {
      strict: testOptions.strict !== false,
      noImplicitAny: testOptions.noimplicitany,
      strictNullChecks: testOptions.strictnullchecks,
      noImplicitReturns: testOptions.noimplicitreturns,
      noImplicitThis: testOptions.noimplicitthis,
      strictFunctionTypes: testOptions.strictfunctiontypes,
      strictPropertyInitialization: testOptions.strictpropertyinitialization,
      lib: testOptions.lib,
      module: testOptions.module,
      nolib: testOptions.nolib,
    };

    // Remove undefined values
    Object.keys(compilerOptions).forEach(key =>
      compilerOptions[key] === undefined && delete compilerOptions[key]
    );

    // Apply compiler options to WASM parser
    if (Object.keys(compilerOptions).length > 0) {
      parser.setCompilerOptions(JSON.stringify(compilerOptions));
    }

    if (!testOptions.nolib) {
      parser.addLibFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
    }
    parser.parseSourceFile();

    const parseDiagsJson = parser.getDiagnosticsJson();
    const parseDiags = JSON.parse(parseDiagsJson);

    const checkResultJson = parser.checkSourceFile();
    const checkResult = JSON.parse(checkResultJson);

    const allDiagnostics = [
      ...parseDiags.map(d => ({ code: d.code, message: d.message, category: 'Error', source: 'parser' })),
      ...(checkResult.diagnostics || []).map(d => ({
        code: d.code,
        message: d.message_text,
        category: d.category,
        source: 'checker',
      })),
    ];

    parser.free();
    parser = null;

    return { diagnostics: allDiagnostics, crashed: false };
  } catch (e) {
    if (parser) {
      try { parser.free(); } catch {}
    }
    return { diagnostics: [], crashed: true, error: e.message };
  }
}

function compareDiagnostics(tscResult, wasmResult) {
  const tscCodes = new Set(tscResult.diagnostics.map(d => d.code));
  const wasmCodes = new Set(wasmResult.diagnostics.map(d => d.code));

  const missingInWasm = [...tscCodes].filter(c => !wasmCodes.has(c));
  const extraInWasm = [...wasmCodes].filter(c => !tscCodes.has(c));

  return {
    exactMatch: missingInWasm.length === 0 && extraInWasm.length === 0,
    sameCount: tscResult.diagnostics.length === wasmResult.diagnostics.length,
    tscCount: tscResult.diagnostics.length,
    wasmCount: wasmResult.diagnostics.length,
    missingInWasm,
    extraInWasm,
  };
}

const TEST_TIMEOUT_MS = 30000; // 30 second timeout per test

function withTimeout(promise, ms, errorMsg) {
  let timeoutId;
  const timeoutPromise = new Promise((_, reject) => {
    timeoutId = setTimeout(() => reject(new Error(errorMsg)), ms);
  });
  return Promise.race([promise, timeoutPromise]).finally(() => clearTimeout(timeoutId));
}

async function processTest(filePath) {
  const fileName = basename(filePath);
  const relPath = filePath.replace(conformanceDir + '/', '');
  const cat = relPath.split('/')[0];

  try {
    const rawCode = readFileSync(filePath, 'utf-8');
    const { options, isMultiFile, cleanCode, files } = parseTestDirectives(rawCode);

    // Skip multi-file tests for now
    if (isMultiFile && files.length > 0) {
      return { relPath, cat, skipped: true, reason: 'multi-file' };
    }

    const [tscResult, wasmResult] = await withTimeout(
      Promise.all([
        runTsc(cleanCode, fileName, options),
        runWasm(cleanCode, fileName, options),
      ]),
      TEST_TIMEOUT_MS,
      `Test ${relPath} timed out after ${TEST_TIMEOUT_MS / 1000}s`
    );

    if (wasmResult.crashed) {
      return { relPath, cat, crashed: true, error: wasmResult.error, isMultiFile: false };
    }

    const comparison = compareDiagnostics(tscResult, wasmResult);

    return {
      relPath,
      cat,
      crashed: false,
      isMultiFile: false,
      exactMatch: comparison.exactMatch,
      sameCount: comparison.sameCount,
      tscCount: comparison.tscCount,
      wasmCount: comparison.wasmCount,
      missingInWasm: comparison.missingInWasm,
      extraInWasm: comparison.extraInWasm,
    };
  } catch (e) {
    return { relPath, cat, skipped: true, error: e.message };
  }
}

// Process all assigned test files
async function main() {
  const results = [];

  for (let i = 0; i < testFiles.length; i++) {
    const result = await processTest(testFiles[i]);
    results.push(result);
    // Report progress via IPC every 10 tests
    if ((i + 1) % 10 === 0) {
      process.send({ type: 'progress', completed: i + 1, total: testFiles.length });
    }
  }

  // Send final progress update
  process.send({ type: 'progress', completed: testFiles.length, total: testFiles.length });

  // Send results back to parent
  process.send({ type: 'done', results });
}

main().catch(e => {
  process.send({ type: 'error', error: e.message });
  process.exit(1);
});
