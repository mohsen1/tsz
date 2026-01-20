/**
 * Persistent worker thread for test execution
 * 
 * Loads WASM once at startup, then processes tests as messages arrive.
 * If a test hangs, the main thread can terminate this worker and spawn a new one.
 */

import { parentPort, workerData } from 'worker_threads';
import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

interface TestJob {
  id: number;
  filePath: string;
  testsBasePath: string;
}

interface TestFile {
  name: string;
  content: string;
}

interface ParsedTestCase {
  options: Record<string, unknown>;
  isMultiFile: boolean;
  files: TestFile[];
  category: string;
}

interface WorkerResult {
  id: number;
  tscCodes: number[];
  wasmCodes: number[];
  crashed: boolean;
  category: string;
  error?: string;
}

// Cached at worker startup
let wasmModule: any = null;
let libSource = '';

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
        files.push({ name: currentFileName, content: currentFileLines.join('\n') });
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
    files.push({ name: currentFileName, content: currentFileLines.join('\n') });
  }
  if (!isMultiFile) {
    files.push({ name: path.basename(filePath), content: cleanLines.join('\n') });
  }

  const relativePath = filePath.replace(/.*tests\/cases\//, '');
  const category = relativePath.split(path.sep)[0] || 'unknown';

  return { options, isMultiFile, files, category };
}

function toCompilerOptions(opts: Record<string, unknown>): ts.CompilerOptions {
  const options: ts.CompilerOptions = {
    strict: opts.strict !== false,
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    noEmit: true,
    skipLibCheck: true,
  };

  if (opts.target) {
    const map: Record<string, ts.ScriptTarget> = {
      es5: ts.ScriptTarget.ES5, es6: ts.ScriptTarget.ES2015, es2015: ts.ScriptTarget.ES2015,
      es2017: ts.ScriptTarget.ES2017, es2020: ts.ScriptTarget.ES2020, esnext: ts.ScriptTarget.ESNext,
    };
    options.target = map[String(opts.target).toLowerCase()] || ts.ScriptTarget.ES2020;
  }

  if (opts.noimplicitany !== undefined) options.noImplicitAny = opts.noimplicitany as boolean;
  if (opts.strictnullchecks !== undefined) options.strictNullChecks = opts.strictnullchecks as boolean;
  if (opts.nolib !== undefined) options.noLib = opts.nolib as boolean;

  return options;
}

function runTsc(testCase: ParsedTestCase): number[] {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];

  for (const file of testCase.files) {
    const sf = ts.createSourceFile(file.name, file.content, ts.ScriptTarget.ES2020, true);
    sourceFiles.set(file.name, sf);
    fileNames.push(file.name);
  }

  if (!testCase.options.nolib && libSource) {
    sourceFiles.set('lib.d.ts', ts.createSourceFile('lib.d.ts', libSource, ts.ScriptTarget.ES2020, true));
  }

  const host = ts.createCompilerHost(compilerOptions);
  host.getSourceFile = (name) => sourceFiles.get(name);
  host.fileExists = (name) => sourceFiles.has(name);
  host.readFile = (name) => testCase.files.find(f => f.name === name)?.content || (name === 'lib.d.ts' ? libSource : undefined);

  const program = ts.createProgram(fileNames, compilerOptions, host);
  const diags: number[] = [];
  for (const sf of sourceFiles.values()) {
    if (sf.fileName !== 'lib.d.ts') {
      for (const d of program.getSyntacticDiagnostics(sf)) diags.push(d.code);
      for (const d of program.getSemanticDiagnostics(sf)) diags.push(d.code);
    }
  }
  return diags;
}

function runWasm(testCase: ParsedTestCase): { codes: number[]; crashed: boolean; error?: string } {
  try {
    if (testCase.isMultiFile || testCase.files.length > 1) {
      const program = new wasmModule.WasmProgram();
      if (!testCase.options.nolib && libSource) program.addFile('lib.d.ts', libSource);
      for (const file of testCase.files) program.addFile(file.name, file.content);
      return { codes: Array.from(program.getAllDiagnosticCodes()), crashed: false };
    } else {
      const file = testCase.files[0];
      const parser = new wasmModule.ThinParser(file.name, file.content);
      if (!testCase.options.nolib && libSource) parser.addLibFile('lib.d.ts', libSource);
      if (parser.setCompilerOptions) parser.setCompilerOptions(JSON.stringify(testCase.options));
      parser.parseSourceFile();
      const parseDiags = JSON.parse(parser.getDiagnosticsJson());
      const checkResult = JSON.parse(parser.checkSourceFile());
      const codes = [
        ...parseDiags.map((d: any) => d.code),
        ...(checkResult.diagnostics || []).map((d: any) => d.code),
      ];
      parser.free();
      return { codes, crashed: false };
    }
  } catch (e) {
    return { codes: [], crashed: true, error: e instanceof Error ? e.message : String(e) };
  }
}

function processTest(job: TestJob): WorkerResult {
  try {
    const code = fs.readFileSync(job.filePath, 'utf8');
    const testCase = parseTestDirectives(code, job.filePath);
    const tscCodes = runTsc(testCase);
    const wasmResult = runWasm(testCase);
    return {
      id: job.id,
      tscCodes,
      wasmCodes: wasmResult.codes,
      crashed: wasmResult.crashed,
      category: testCase.category,
      error: wasmResult.error,
    };
  } catch (e) {
    return {
      id: job.id,
      tscCodes: [],
      wasmCodes: [],
      crashed: true,
      category: 'unknown',
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

// Initialize worker
(async () => {
  const { wasmPkgPath, libPath } = workerData as { wasmPkgPath: string; libPath: string };

  // Load WASM module once
  const wasmPath = path.join(wasmPkgPath, 'wasm.js');
  const module = await import(wasmPath);
  if (typeof module.default === 'function') await module.default();
  wasmModule = module;

  // Load lib.d.ts once
  try {
    libSource = fs.readFileSync(libPath, 'utf8');
  } catch {}

  // Signal ready
  parentPort!.postMessage({ type: 'ready' });

  // Process jobs
  parentPort!.on('message', (job: TestJob) => {
    const result = processTest(job);
    parentPort!.postMessage({ type: 'result', ...result });
  });
})();
