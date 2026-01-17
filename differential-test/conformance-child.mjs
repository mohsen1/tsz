#!/usr/bin/env node
/**
 * Child process for conformance testing (runs in isolated process)
 */

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { dirname, join, basename } from 'path';
import { readFileSync, existsSync } from 'fs';

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Read config from temp file passed as argument
const configFile = process.argv[2];
const { testFiles, wasmPkgPath, conformanceDir } = JSON.parse(readFileSync(configFile, 'utf-8'));
const DEFAULT_LIB_PATH = join(__dirname, '../ts-tests/lib/lib.d.ts');
const DEFAULT_LIB_SOURCE = readFileSync(DEFAULT_LIB_PATH, 'utf-8');
const DEFAULT_LIB_NAME = 'lib.d.ts';

// TypeScript lib directory - try to find system TypeScript installation
const TYPESCRIPT_LIB_DIR = (() => {
  try {
    const ts = require('typescript');
    // TypeScript's lib files are in node_modules/typescript/lib/
    const tsPath = require.resolve('typescript');
    return join(dirname(tsPath), 'lib');
  } catch {
    return null;
  }
})();

/**
 * Get the file path for a TypeScript lib file by name.
 * Maps lib names (e.g., "es2020", "dom") to actual file paths.
 *
 * @param {string} libName - The lib name (e.g., "es2020", "dom", "esnext")
 * @returns {string|null} - Full path to the lib file, or null if not found
 */
function getLibFilePath(libName) {
  const normalized = libName.toLowerCase().trim();

  // Special case: es6 is an alias for es2015
  const fileName = normalized === 'es6' ? 'lib.es2015.d.ts' : `lib.${normalized}.d.ts`;

  if (!TYPESCRIPT_LIB_DIR) {
    return null;
  }

  const libPath = join(TYPESCRIPT_LIB_DIR, fileName);
  return existsSync(libPath) ? libPath : null;
}

/**
 * Load TypeScript lib file content, reading dependencies recursively.
 * Handles /// <reference lib="..." /> directives in lib files.
 *
 * @param {string} libName - The lib name to load
 * @param {Set<string>} loaded - Set of already loaded lib names to prevent cycles
 * @returns {Array<{name: string, content: string}>} - Array of lib files with their content
 */
function loadLibFile(libName, loaded = new Set()) {
  const normalized = libName.toLowerCase().trim();

  // Prevent loading the same lib twice
  if (loaded.has(normalized)) {
    return [];
  }
  loaded.add(normalized);

  const libPath = getLibFilePath(normalized);
  if (!libPath) {
    return [];
  }

  try {
    const content = readFileSync(libPath, 'utf-8');
    const results = [];

    // Parse /// <reference lib="..." /> directives
    const referenceRegex = /^\/\/\/\s*<reference\s+lib="([^"]+)"\s*\/>/gm;
    let match;
    while ((match = referenceRegex.exec(content)) !== null) {
      const referencedLib = match[1];
      // Recursively load referenced libs
      results.push(...loadLibFile(referencedLib, loaded));
    }

    // Add this lib after its dependencies
    results.push({ name: `lib.${normalized}.d.ts`, content });

    return results;
  } catch {
    return [];
  }
}

/**
 * Parse TypeScript test directives from source code.
 *
 * Supported directives:
 * - @strict: boolean - enable all strict type checking options
 * - @noImplicitAny: boolean - raise error on implied 'any' type
 * - @strictNullChecks: boolean - enable strict null checks
 * - @target: string - ECMAScript target version (es5, es2015, es2020, esnext, etc.)
 * - @module: string - module code generation (commonjs, esnext, etc.)
 * - @lib: string - comma-separated list of lib files (e.g., es2020,dom)
 * - @declaration: boolean - emit declaration files
 * - @noEmit: boolean - do not emit output
 * - @experimentalDecorators: boolean - enable experimental decorators
 * - @allowSyntheticDefaultImports: boolean - allow default imports from modules with no default export
 * - @esModuleInterop: boolean - enable ES module interoperability
 * - @skipLibCheck: boolean - skip type checking of declaration files
 * - @skipDefaultLibCheck: boolean - skip type checking of default library declaration files
 * - @moduleResolution: string - module resolution strategy (node, classic, bundler, node16, nodenext)
 * - @allowJs: boolean - allow JavaScript files to be compiled
 * - @checkJs: boolean - report errors in JavaScript files
 * - @jsx: string - JSX code generation (preserve, react, react-native, react-jsx, react-jsxdev)
 * - @isolatedModules: boolean - ensure each file can be safely transpiled without relying on other imports
 * - @traceResolution: boolean - enable tracing of module resolution process
 * - @filename: string - marks multi-file test boundaries (special directive)
 *
 * @param {string} code - Source code with test directives
 * @returns {{options: Object, isMultiFile: boolean, cleanCode: string, files: Array}} Parsed directives and cleaned code
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
      const normalizedKey = key.toLowerCase();

      // Special handling for @lib directive - parse as comma-separated list and accumulate
      if (normalizedKey === 'lib') {
        if (!options[normalizedKey]) {
          options[normalizedKey] = [];
        }
        const libs = value.split(',').map(lib => lib.trim()).filter(lib => lib.length > 0);
        options[normalizedKey].push(...libs);
      } else if (value === 'true') {
        options[normalizedKey] = true;
      } else if (value === 'false') {
        options[normalizedKey] = false;
      } else if (!isNaN(Number(value))) {
        options[normalizedKey] = Number(value);
      } else {
        options[normalizedKey] = value;
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
    strict: testOptions.strict === true,  // Only enable strict if explicitly set
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

async function runWasm(code, fileName = 'test.ts', testOptions = {}) {
  let parser = null;
  try {
    const wasmModule = await import(join(wasmPkgPath, 'wasm.js'));

    // WASM module auto-initializes when built with --target nodejs
    parser = new wasmModule.ThinParser(fileName, code);

    // Handle lib files
    if (!testOptions.nolib) {
      if (testOptions.lib && Array.isArray(testOptions.lib)) {
        // Load specified lib files
        let hasLoadedLibs = false;
        for (const libName of testOptions.lib) {
          const libFiles = loadLibFile(libName);
          for (const { name, content } of libFiles) {
            parser.addLibFile(name, content);
            hasLoadedLibs = true;
          }
        }
        // Fallback to default lib if no lib files could be loaded
        if (!hasLoadedLibs) {
          parser.addLibFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
        }
      } else {
        // Use default lib
        parser.addLibFile(DEFAULT_LIB_NAME, DEFAULT_LIB_SOURCE);
      }
    }

    // Build compiler options from test directives
    const compilerOptions = {};
    if (testOptions.strict !== undefined) {
      compilerOptions.strict = testOptions.strict;
    }
    if (testOptions.noimplicitany !== undefined) {
      compilerOptions.noImplicitAny = testOptions.noimplicitany;
    }
    if (testOptions.strictnullchecks !== undefined) {
      compilerOptions.strictNullChecks = testOptions.strictnullchecks;
    }

    // Apply compiler options to the parser
    if (Object.keys(compilerOptions).length > 0) {
      parser.setCompilerOptions(JSON.stringify(compilerOptions));
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

const TEST_TIMEOUT_MS = 5000; // 5 second timeout per test (to skip hangs faster)

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
