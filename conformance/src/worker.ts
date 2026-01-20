/**
 * Persistent worker thread for test execution
 * 
 * Loads WASM once at startup, then processes tests as messages arrive.
 * Includes crash detection and graceful error reporting.
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
  type: 'result';
  id: number;
  tscCodes: number[];
  wasmCodes: number[];
  crashed: boolean;
  oom: boolean;
  category: string;
  error?: string;
  memoryUsed?: number;
}

// Cached at worker startup
let wasmModule: any = null;
let libSource = '';
let workerId = -1;

// Memory monitoring
const getMemoryUsage = () => process.memoryUsage().heapUsed;
const formatBytes = (b: number) => `${(b / 1024 / 1024).toFixed(1)}MB`;

// Heartbeat to detect hangs
let lastActivity = Date.now();
const HEARTBEAT_INTERVAL = 1000;

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

function runWasm(testCase: ParsedTestCase): { codes: number[]; crashed: boolean; oom: boolean; error?: string } {
  const memBefore = getMemoryUsage();
  
  try {
    if (testCase.isMultiFile || testCase.files.length > 1) {
      const program = new wasmModule.WasmProgram();
      if (!testCase.options.nolib && libSource) program.addFile('lib.d.ts', libSource);
      for (const file of testCase.files) program.addFile(file.name, file.content);
      const codes = Array.from(program.getAllDiagnosticCodes()) as number[];
      program.free();
      return { codes, crashed: false, oom: false };
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
      return { codes, crashed: false, oom: false };
    }
  } catch (e) {
    const memAfter = getMemoryUsage();
    const memGrowth = memAfter - memBefore;
    const isOom = memGrowth > 100 * 1024 * 1024 || // 100MB growth suggests OOM
                  (e instanceof Error && (
                    e.message.includes('memory') ||
                    e.message.includes('allocation') ||
                    e.message.includes('heap') ||
                    e.message.includes('out of') ||
                    e.message.includes('RuntimeError')
                  ));
    
    return { 
      codes: [], 
      crashed: true, 
      oom: isOom,
      error: e instanceof Error ? e.message : String(e) 
    };
  }
}

function processTest(job: TestJob): WorkerResult {
  lastActivity = Date.now();
  const memBefore = getMemoryUsage();
  
  try {
    const code = fs.readFileSync(job.filePath, 'utf8');
    const testCase = parseTestDirectives(code, job.filePath);
    
    // Run TSC first (more stable)
    const tscCodes = runTsc(testCase);
    lastActivity = Date.now();
    
    // Run WASM (may crash)
    const wasmResult = runWasm(testCase);
    lastActivity = Date.now();
    
    const memAfter = getMemoryUsage();
    
    return {
      type: 'result',
      id: job.id,
      tscCodes,
      wasmCodes: wasmResult.codes,
      crashed: wasmResult.crashed,
      oom: wasmResult.oom,
      category: testCase.category,
      error: wasmResult.error,
      memoryUsed: memAfter - memBefore,
    };
  } catch (e) {
    const memAfter = getMemoryUsage();
    const isOom = (memAfter - memBefore) > 100 * 1024 * 1024;
    
    return {
      type: 'result',
      id: job.id,
      tscCodes: [],
      wasmCodes: [],
      crashed: true,
      oom: isOom,
      category: 'unknown',
      error: e instanceof Error ? `${e.name}: ${e.message}` : String(e),
      memoryUsed: memAfter - memBefore,
    };
  }
}

// Uncaught exception handler - report and exit
process.on('uncaughtException', (err) => {
  try {
    parentPort?.postMessage({
      type: 'crash',
      workerId,
      error: `Uncaught: ${err.message}`,
      stack: err.stack,
    });
  } catch {
    // Can't send message, just exit
  }
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  try {
    parentPort?.postMessage({
      type: 'crash',
      workerId,
      error: `Unhandled rejection: ${reason}`,
    });
  } catch {
    // Can't send message, just exit
  }
  process.exit(1);
});

// Initialize worker
(async () => {
  const { wasmPkgPath, libPath, id } = workerData as { wasmPkgPath: string; libPath: string; id: number };
  workerId = id;

  // Load WASM module once
  try {
    const wasmPath = path.join(wasmPkgPath, 'wasm.js');
    const module = await import(wasmPath);
    if (typeof module.default === 'function') await module.default();
    wasmModule = module;
  } catch (e) {
    parentPort?.postMessage({
      type: 'error',
      workerId,
      error: `Failed to load WASM: ${e instanceof Error ? e.message : e}`,
    });
    process.exit(1);
  }

  // Load lib.d.ts once
  try {
    libSource = fs.readFileSync(libPath, 'utf8');
  } catch {}

  // Signal ready with memory info
  parentPort!.postMessage({ 
    type: 'ready', 
    workerId,
    memoryUsed: getMemoryUsage(),
  });

  // Process jobs
  parentPort!.on('message', (job: TestJob) => {
    const result = processTest(job);
    parentPort!.postMessage(result);
    
    // Force GC if available and memory is high
    if (global.gc && getMemoryUsage() > 500 * 1024 * 1024) {
      global.gc();
    }
  });

  // Heartbeat - detect if we're hung
  setInterval(() => {
    const sinceLastActivity = Date.now() - lastActivity;
    if (sinceLastActivity > 30000) {
      // No activity for 30s - we might be stuck
      parentPort?.postMessage({
        type: 'heartbeat',
        workerId,
        sinceLastActivity,
        memoryUsed: getMemoryUsage(),
      });
    }
  }, HEARTBEAT_INTERVAL);
})();
