#!/usr/bin/env node
/**
 * Worker thread for parallel conformance testing
 */

import { parentPort, workerData } from 'worker_threads';
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, basename } from 'path';
import { readFileSync } from 'fs';
import { parseTestDirectives, mapToCompilerOptions, mapToWasmCompilerOptions } from './directive-parser.mjs';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const { wasmPkgPath, conformanceDir, testFiles } = workerData;
const DEFAULT_LIB_PATH = join(__dirname, '../../tests/lib/lib.d.ts');
const DEFAULT_LIB_SOURCE = readFileSync(DEFAULT_LIB_PATH, 'utf-8');
const DEFAULT_LIB_NAME = 'lib.d.ts';

// parseTestDirectives is imported from directive-parser.mjs

async function runTsc(code, fileName = 'test.ts', testOptions = {}) {
  const ts = require('typescript');

  // Build compiler options from test directives using the centralized mapper
  const mappedOptions = mapToCompilerOptions(testOptions, ts);

  // Set defaults and merge with mapped options
  const compilerOptions = {
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
    ...mappedOptions,
  };

  // Handle strict mode default: if strict is not explicitly set, default to false
  if (testOptions.strict === undefined) {
    compilerOptions.strict = false;
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

async function runTscMultiFile(files, testOptions = {}) {
  const ts = require('typescript');

  // Build compiler options from test directives using the centralized mapper
  const mappedOptions = mapToCompilerOptions(testOptions, ts);

  // Set defaults and merge with mapped options
  const compilerOptions = {
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
    ...mappedOptions,
  };

  // Handle strict mode default: if strict is not explicitly set, default to false
  if (testOptions.strict === undefined) {
    compilerOptions.strict = false;
  }

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
    if (sourceFiles.has(name)) return sourceFiles.get(name);
    return originalGetSourceFile.call(host, name, languageVersion, onError);
  };

  host.fileExists = (name) => sourceFiles.has(name) || ts.sys.fileExists(name);
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
    const wasm = await import(join(wasmPkgPath, 'wasm.js'));

    const parser = new wasm.ThinParser(fileName, code);

    // Build compiler options from test directives using the centralized mapper
    const wasmOptions = mapToWasmCompilerOptions(testOptions);

    // Handle strict mode default: if strict is not explicitly set, default to false
    if (testOptions.strict === undefined) {
      wasmOptions.strict = false;
    }

    // Apply compiler options to WASM parser
    if (Object.keys(wasmOptions).length > 0) {
      parser.setCompilerOptions(JSON.stringify(wasmOptions));
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

    return { diagnostics: allDiagnostics, crashed: false };
  } catch (e) {
    return { diagnostics: [], crashed: true, error: e.message };
  }
}

async function runWasmMultiFile(files, testOptions = {}) {
  try {
    const wasm = await import(join(wasmPkgPath, 'wasm.js'));

    const program = new wasm.WasmProgram();

    // Build compiler options from test directives using the centralized mapper
    const wasmOptions = mapToWasmCompilerOptions(testOptions);

    // Handle strict mode default: if strict is not explicitly set, default to false
    if (testOptions.strict === undefined) {
      wasmOptions.strict = false;
    }

    // Apply compiler options to the program if the method exists
    if (Object.keys(wasmOptions).length > 0 && typeof program.setCompilerOptions === 'function') {
      program.setCompilerOptions(JSON.stringify(wasmOptions));
    }

    if (!testOptions.nolib) {
      program.addFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
    }

    for (const file of files) {
      program.addFile(file.name, file.content);
    }

    const codes = program.getAllDiagnosticCodes();

    const allDiagnostics = Array.from(codes).map(code => ({
      code,
      message: '',
      category: 'Error',
      source: 'program',
    }));

    return { diagnostics: allDiagnostics, crashed: false };
  } catch (e) {
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

async function processTest(filePath) {
  const fileName = basename(filePath);
  const relPath = filePath.replace(conformanceDir + '/', '');
  const cat = relPath.split('/')[0];

  try {
    const rawCode = readFileSync(filePath, 'utf-8');
    const { options, isMultiFile, cleanCode, files } = parseTestDirectives(rawCode);

    let tscResult, wasmResult;

    // Skip multi-file tests - WasmProgram has memory bugs in finalization
    if (isMultiFile && files.length > 0) {
      return { relPath, cat, skipped: true, reason: 'multi-file' };
    } else {
      [tscResult, wasmResult] = await Promise.all([
        runTsc(cleanCode, fileName, options),
        runWasm(cleanCode, fileName, options),
      ]);
    }

    if (wasmResult.crashed) {
      return { relPath, cat, crashed: true, error: wasmResult.error, isMultiFile };
    }

    const comparison = compareDiagnostics(tscResult, wasmResult);

    return {
      relPath,
      cat,
      crashed: false,
      isMultiFile,
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
    // Report progress
    parentPort.postMessage({ type: 'progress', index: i + 1, total: testFiles.length });
  }

  parentPort.postMessage({ type: 'done', results });
}

main().catch(e => {
  parentPort.postMessage({ type: 'error', error: e.message });
});
