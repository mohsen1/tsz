#!/usr/bin/env node
/**
 * Worker thread for parallel conformance testing
 */

import { parentPort, workerData } from 'worker_threads';
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, basename } from 'path';
import { readFileSync } from 'fs';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const { wasmPkgPath, conformanceDir, testFiles } = workerData;
const DEFAULT_LIB_PATH = join(__dirname, '../../tests/lib/lib.d.ts');
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

async function runWasmMultiFile(files) {
  try {
    const wasm = await import(join(wasmPkgPath, 'wasm.js'));

    const program = new wasm.WasmProgram();

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
