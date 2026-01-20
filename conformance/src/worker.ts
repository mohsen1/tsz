/**
 * Worker thread for parallel test execution
 */

import { parentPort, workerData } from 'worker_threads';
import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

interface TestJob {
  filePath: string;
  libSource: string;
  testsBasePath: string;
  timeout?: number;  // Per-test timeout in ms (default: 5000)
}

interface TestFile {
  name: string;
  content: string;
  relativePath: string;
}

interface ParsedTestCase {
  options: Record<string, unknown>;
  isMultiFile: boolean;
  files: TestFile[];
  category: string;
  testName: string;
}

interface DiagnosticInfo {
  code: number;
  message: string;
  category: string;
  file?: string;
  start?: number;
  length?: number;
}

interface TestResult {
  diagnostics: DiagnosticInfo[];
  crashed: boolean;
  error?: string;
}

interface WorkerResult {
  filePath: string;
  relPath: string;
  category: string;
  tscCodes: number[];
  wasmCodes: number[];
  crashed: boolean;
  error?: string;
  skipped: boolean;
  timedOut?: boolean;
}

// Initialize WASM module once per worker
let wasmModule: unknown = null;

async function initWasm(wasmPkgPath: string): Promise<void> {
  if (wasmModule) return;
  const wasmPath = path.join(wasmPkgPath, 'wasm.js');
  const module = await import(wasmPath);
  if (typeof module.default === 'function') {
    await module.default();
  }
  wasmModule = module;
}

function parseTestDirectives(code: string, filePath: string): ParsedTestCase {
  const lines = code.split('\n');
  const options: Record<string, unknown> = {};
  let isMultiFile = false;
  const files: TestFile[] = [];
  let currentFileName: string | null = null;
  let currentFileLines: string[] = [];
  const cleanLines: string[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/i);
    if (filenameMatch) {
      isMultiFile = true;
      if (currentFileName) {
        files.push({ name: currentFileName, content: currentFileLines.join('\n'), relativePath: currentFileName });
      }
      currentFileName = filenameMatch[1].trim();
      currentFileLines = [];
      continue;
    }

    const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
    if (optionMatch) {
      const [, key, value] = optionMatch;
      const lowKey = key.toLowerCase();
      if (value.toLowerCase() === 'true') options[lowKey] = true;
      else if (value.toLowerCase() === 'false') options[lowKey] = false;
      else if (!isNaN(Number(value))) options[lowKey] = Number(value);
      else options[lowKey] = value;
      continue;
    }

    if (isMultiFile && currentFileName) {
      currentFileLines.push(line);
    } else {
      cleanLines.push(line);
    }
  }

  if (isMultiFile && currentFileName) {
    files.push({ name: currentFileName, content: currentFileLines.join('\n'), relativePath: currentFileName });
  }
  if (!isMultiFile) {
    const baseName = path.basename(filePath);
    files.push({ name: baseName, content: cleanLines.join('\n'), relativePath: baseName });
  }

  const relativePath = filePath.replace(/.*tests\/cases\//, '');
  const parts = relativePath.split(path.sep);
  const category = parts[0] || 'unknown';
  const testName = parts.slice(1).join('/').replace(/\.ts$/, '');

  return { options, isMultiFile, files, category, testName };
}

function toCompilerOptions(testOptions: Record<string, unknown>): ts.CompilerOptions {
  const options: ts.CompilerOptions = {
    strict: testOptions.strict !== false,
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
  };

  if (testOptions.target) {
    const targetMap: Record<string, ts.ScriptTarget> = {
      'es5': ts.ScriptTarget.ES5, 'es6': ts.ScriptTarget.ES2015, 'es2015': ts.ScriptTarget.ES2015,
      'es2016': ts.ScriptTarget.ES2016, 'es2017': ts.ScriptTarget.ES2017, 'es2018': ts.ScriptTarget.ES2018,
      'es2019': ts.ScriptTarget.ES2019, 'es2020': ts.ScriptTarget.ES2020, 'es2021': ts.ScriptTarget.ES2021,
      'es2022': ts.ScriptTarget.ES2022, 'esnext': ts.ScriptTarget.ESNext,
    };
    options.target = targetMap[String(testOptions.target).toLowerCase()] || ts.ScriptTarget.ES2020;
  }

  const booleanOptions: Array<[string, keyof ts.CompilerOptions]> = [
    ['noimplicitany', 'noImplicitAny'], ['strictnullchecks', 'strictNullChecks'],
    ['nolib', 'noLib'], ['skiplibcheck', 'skipLibCheck'],
  ];
  for (const [testKey, compilerKey] of booleanOptions) {
    if (testOptions[testKey] !== undefined) {
      (options as Record<string, unknown>)[compilerKey] = testOptions[testKey];
    }
  }

  return options;
}

function runTsc(testCase: ParsedTestCase, libSource: string): TestResult {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];

  for (const file of testCase.files) {
    const sf = ts.createSourceFile(file.name, file.content, ts.ScriptTarget.ES2020, true);
    sourceFiles.set(file.name, sf);
    fileNames.push(file.name);
  }

  if (!testCase.options.nolib) {
    const libSf = ts.createSourceFile('lib.d.ts', libSource, ts.ScriptTarget.ES2020, true);
    sourceFiles.set('lib.d.ts', libSf);
  }

  const host = ts.createCompilerHost(compilerOptions);
  const originalGetSourceFile = host.getSourceFile;
  host.getSourceFile = (name, languageVersion, onError) => {
    if (sourceFiles.has(name)) return sourceFiles.get(name);
    return originalGetSourceFile.call(host, name, languageVersion, onError);
  };
  host.fileExists = (name) => sourceFiles.has(name) || ts.sys.fileExists(name);
  host.readFile = (name) => {
    const file = testCase.files.find(f => f.name === name);
    if (file) return file.content;
    if (name === 'lib.d.ts' && !testCase.options.nolib) return libSource;
    return ts.sys.readFile(name);
  };

  const program = ts.createProgram(fileNames, compilerOptions, host);
  const allDiagnostics: ts.Diagnostic[] = [];
  for (const sf of sourceFiles.values()) {
    if (sf.fileName !== 'lib.d.ts') {
      allDiagnostics.push(...program.getSyntacticDiagnostics(sf));
      allDiagnostics.push(...program.getSemanticDiagnostics(sf));
    }
  }

  return {
    diagnostics: allDiagnostics.map(d => ({
      code: d.code,
      message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
      category: ts.DiagnosticCategory[d.category],
    })),
    crashed: false,
  };
}

function runWasm(testCase: ParsedTestCase, libSource: string): TestResult {
  try {
    const wasm = wasmModule as {
      ThinParser: new (name: string, code: string) => {
        addLibFile(name: string, content: string): void;
        setCompilerOptions?(options: string): void;
        parseSourceFile(): number;
        getDiagnosticsJson(): string;
        checkSourceFile(): string;
        free(): void;
      };
      WasmProgram: new () => {
        addFile(name: string, content: string): void;
        getAllDiagnosticCodes(): number[];
      };
    };

    if (testCase.isMultiFile || testCase.files.length > 1) {
      const program = new wasm.WasmProgram();
      if (!testCase.options.nolib) program.addFile('lib.d.ts', libSource);
      for (const file of testCase.files) program.addFile(file.name, file.content);
      const codes = program.getAllDiagnosticCodes();
      return {
        diagnostics: Array.from(codes).map(code => ({ code, message: '', category: 'Error' })),
        crashed: false,
      };
    } else {
      const file = testCase.files[0];
      const parser = new wasm.ThinParser(file.name, file.content);
      if (!testCase.options.nolib) parser.addLibFile('lib.d.ts', libSource);
      if (parser.setCompilerOptions) parser.setCompilerOptions(JSON.stringify(testCase.options));
      parser.parseSourceFile();
      const parseDiags = JSON.parse(parser.getDiagnosticsJson());
      const checkResult = JSON.parse(parser.checkSourceFile());
      const diagnostics = [
        ...parseDiags.map((d: { code: number; message: string }) => ({ code: d.code, message: d.message, category: 'Error' })),
        ...(checkResult.diagnostics || []).map((d: { code: number }) => ({ code: d.code, message: '', category: 'Error' })),
      ];
      parser.free();
      return { diagnostics, crashed: false };
    }
  } catch (error) {
    return { diagnostics: [], crashed: true, error: error instanceof Error ? error.message : String(error) };
  }
}

async function processJob(job: TestJob): Promise<WorkerResult> {
  const { filePath, libSource, testsBasePath } = job;
  const relPath = filePath.replace(testsBasePath + path.sep, '');

  try {
    const code = fs.readFileSync(filePath, 'utf8');
    const testCase = parseTestDirectives(code, filePath);
    const tscResult = runTsc(testCase, libSource);
    const wasmResult = runWasm(testCase, libSource);

    return {
      filePath,
      relPath,
      category: testCase.category,
      tscCodes: tscResult.diagnostics.map(d => d.code),
      wasmCodes: wasmResult.diagnostics.map(d => d.code),
      crashed: wasmResult.crashed,
      error: wasmResult.error,
      skipped: false,
    };
  } catch (error) {
    return {
      filePath,
      relPath,
      category: 'unknown',
      tscCodes: [],
      wasmCodes: [],
      crashed: false,
      skipped: true,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

// Timeout wrapper
function withTimeout<T>(promise: Promise<T>, ms: number, timeoutResult: T): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((resolve) => setTimeout(() => resolve(timeoutResult), ms))
  ]);
}

// Worker main
(async () => {
  const { wasmPkgPath } = workerData as { wasmPkgPath: string };
  await initWasm(wasmPkgPath);

  parentPort!.on('message', async (job: TestJob) => {
    const timeout = job.timeout || 5000; // 5 second default per test
    const relPath = job.filePath.replace(job.testsBasePath + path.sep, '');
    
    const timeoutResult: WorkerResult = {
      filePath: job.filePath,
      relPath,
      category: 'unknown',
      tscCodes: [],
      wasmCodes: [],
      crashed: false,
      skipped: false,
      timedOut: true,
      error: `Test timed out after ${timeout}ms`,
    };

    const result = await withTimeout(processJob(job), timeout, timeoutResult);
    parentPort!.postMessage(result);
  });

  parentPort!.postMessage({ ready: true });
})();
