/**
 * Persistent worker thread for test execution
 * 
 * Loads WASM once at startup, then processes tests as messages arrive.
 * Includes crash detection and graceful error reporting.
 */

import { parentPort, workerData } from 'worker_threads';
import { createRequire } from 'module';
import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

const require = createRequire(import.meta.url);

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
let libPath = '';
let workerId = -1;
let useWasm = true;
let nativeBinaryPath = '';
let nativeBinary: any = null;

// Memory monitoring
const getMemoryUsage = () => process.memoryUsage().heapUsed;

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

// Target string to ScriptTarget mapping
const TARGET_MAP: Record<string, ts.ScriptTarget> = {
  es3: ts.ScriptTarget.ES3,
  es5: ts.ScriptTarget.ES5,
  es6: ts.ScriptTarget.ES2015,
  es2015: ts.ScriptTarget.ES2015,
  es2016: ts.ScriptTarget.ES2016,
  es2017: ts.ScriptTarget.ES2017,
  es2018: ts.ScriptTarget.ES2018,
  es2019: ts.ScriptTarget.ES2019,
  es2020: ts.ScriptTarget.ES2020,
  es2021: ts.ScriptTarget.ES2021,
  es2022: ts.ScriptTarget.ES2022,
  esnext: ts.ScriptTarget.ESNext,
};

// Module string to ModuleKind mapping
const MODULE_MAP: Record<string, ts.ModuleKind> = {
  none: ts.ModuleKind.None,
  commonjs: ts.ModuleKind.CommonJS,
  amd: ts.ModuleKind.AMD,
  umd: ts.ModuleKind.UMD,
  system: ts.ModuleKind.System,
  es6: ts.ModuleKind.ES2015,
  es2015: ts.ModuleKind.ES2015,
  es2020: ts.ModuleKind.ES2020,
  es2022: ts.ModuleKind.ES2022,
  esnext: ts.ModuleKind.ESNext,
  node16: ts.ModuleKind.Node16,
  nodenext: ts.ModuleKind.NodeNext,
  preserve: ts.ModuleKind.Preserve,
};

// ModuleResolution string mapping
const MODULE_RESOLUTION_MAP: Record<string, ts.ModuleResolutionKind> = {
  classic: ts.ModuleResolutionKind.Classic,
  node: ts.ModuleResolutionKind.NodeJs,
  node10: ts.ModuleResolutionKind.NodeJs,
  node16: ts.ModuleResolutionKind.Node16,
  nodenext: ts.ModuleResolutionKind.NodeNext,
  bundler: ts.ModuleResolutionKind.Bundler,
};

// JSX string mapping
const JSX_MAP: Record<string, ts.JsxEmit> = {
  none: ts.JsxEmit.None,
  preserve: ts.JsxEmit.Preserve,
  react: ts.JsxEmit.React,
  'react-native': ts.JsxEmit.ReactNative,
  'react-jsx': ts.JsxEmit.ReactJSX,
  'react-jsxdev': ts.JsxEmit.ReactJSXDev,
};

function toCompilerOptions(opts: Record<string, unknown>): ts.CompilerOptions {
  const options: ts.CompilerOptions = {
    noEmit: true,
  };

  // Target
  if (opts.target !== undefined) {
    const t = String(opts.target).toLowerCase();
    options.target = TARGET_MAP[t] ?? ts.ScriptTarget.ES2020;
  } else {
    options.target = ts.ScriptTarget.ES2020;
  }

  // Module
  if (opts.module !== undefined) {
    const m = String(opts.module).toLowerCase();
    options.module = MODULE_MAP[m] ?? ts.ModuleKind.ESNext;
  } else {
    options.module = ts.ModuleKind.ESNext;
  }

  // Module resolution
  if (opts.moduleresolution !== undefined) {
    const mr = String(opts.moduleresolution).toLowerCase();
    options.moduleResolution = MODULE_RESOLUTION_MAP[mr] ?? ts.ModuleResolutionKind.NodeJs;
  }

  // JSX
  if (opts.jsx !== undefined) {
    const j = String(opts.jsx).toLowerCase();
    options.jsx = JSX_MAP[j] ?? ts.JsxEmit.None;
  }

  // Strict mode flags
  if (opts.strict !== undefined) options.strict = opts.strict as boolean;
  if (opts.noimplicitany !== undefined) options.noImplicitAny = opts.noimplicitany as boolean;
  if (opts.strictnullchecks !== undefined) options.strictNullChecks = opts.strictnullchecks as boolean;
  if (opts.strictfunctiontypes !== undefined) options.strictFunctionTypes = opts.strictfunctiontypes as boolean;
  if (opts.strictbindcallapply !== undefined) options.strictBindCallApply = opts.strictbindcallapply as boolean;
  if (opts.strictpropertyinitialization !== undefined) options.strictPropertyInitialization = opts.strictpropertyinitialization as boolean;
  if (opts.noimplicitthis !== undefined) options.noImplicitThis = opts.noimplicitthis as boolean;
  if (opts.alwaysstrict !== undefined) options.alwaysStrict = opts.alwaysstrict as boolean;

  // Lib and noLib
  if (opts.nolib !== undefined) options.noLib = opts.nolib as boolean;
  if (opts.lib !== undefined) {
    const libVal = opts.lib;
    if (typeof libVal === 'string') {
      options.lib = libVal.split(',').map(s => s.trim());
    } else if (Array.isArray(libVal)) {
      options.lib = libVal as string[];
    }
  }
  if (opts.skiplibcheck !== undefined) options.skipLibCheck = opts.skiplibcheck as boolean;

  // JavaScript support
  if (opts.allowjs !== undefined) options.allowJs = opts.allowjs as boolean;
  if (opts.checkjs !== undefined) options.checkJs = opts.checkjs as boolean;

  // Declaration emit
  if (opts.declaration !== undefined) options.declaration = opts.declaration as boolean;
  if (opts.declarationmap !== undefined) options.declarationMap = opts.declarationmap as boolean;
  if (opts.emitdeclarationonly !== undefined) options.emitDeclarationOnly = opts.emitdeclarationonly as boolean;

  // Decorators
  if (opts.experimentaldecorators !== undefined) options.experimentalDecorators = opts.experimentaldecorators as boolean;
  if (opts.emitdecoratormetadata !== undefined) options.emitDecoratorMetadata = opts.emitdecoratormetadata as boolean;

  // Class fields
  if (opts.usedefineforclassfields !== undefined) options.useDefineForClassFields = opts.usedefineforclassfields as boolean;

  // Import helpers
  if (opts.importhelpers !== undefined) options.importHelpers = opts.importhelpers as boolean;
  if (opts.downleveliteration !== undefined) options.downlevelIteration = opts.downleveliteration as boolean;

  // Module interop
  if (opts.esmoduleinterop !== undefined) options.esModuleInterop = opts.esmoduleinterop as boolean;
  if (opts.allowsyntheticdefaultimports !== undefined) options.allowSyntheticDefaultImports = opts.allowsyntheticdefaultimports as boolean;

  // Output options
  if (opts.outfile !== undefined) options.outFile = opts.outfile as string;
  if (opts.outdir !== undefined) options.outDir = opts.outdir as string;
  if (opts.rootdir !== undefined) options.rootDir = opts.rootdir as string;

  // Type checking
  if (opts.nounusedlocals !== undefined) options.noUnusedLocals = opts.nounusedlocals as boolean;
  if (opts.nounusedparameters !== undefined) options.noUnusedParameters = opts.nounusedparameters as boolean;
  if (opts.noimplicitreturns !== undefined) options.noImplicitReturns = opts.noimplicitreturns as boolean;
  if (opts.nofallthroughcasesinswitch !== undefined) options.noFallthroughCasesInSwitch = opts.nofallthroughcasesinswitch as boolean;
  if (opts.nouncheckedindexedaccess !== undefined) options.noUncheckedIndexedAccess = opts.nouncheckedindexedaccess as boolean;
  if (opts.nopropertyaccessfromindexsignature !== undefined) options.noPropertyAccessFromIndexSignature = opts.nopropertyaccessfromindexsignature as boolean;
  if (opts.exactoptionalpropertytypes !== undefined) options.exactOptionalPropertyTypes = opts.exactoptionalpropertytypes as boolean;

  // Source maps
  if (opts.sourcemap !== undefined) options.sourceMap = opts.sourcemap as boolean;
  if (opts.inlinesourcemap !== undefined) options.inlineSourceMap = opts.inlinesourcemap as boolean;
  if (opts.inlinesources !== undefined) options.inlineSources = opts.inlinesources as boolean;

  // Isolated modules
  if (opts.isolatedmodules !== undefined) options.isolatedModules = opts.isolatedmodules as boolean;

  // Resolve JSON modules
  if (opts.resolvejsonmodule !== undefined) options.resolveJsonModule = opts.resolvejsonmodule as boolean;

  // Preserve const enums
  if (opts.preserveconstenums !== undefined) options.preserveConstEnums = opts.preserveconstenums as boolean;

  // Allow unreachable/unused labels
  if (opts.allowunreachablecode !== undefined) options.allowUnreachableCode = opts.allowunreachablecode as boolean;
  if (opts.allowunusedlabels !== undefined) options.allowUnusedLabels = opts.allowunusedlabels as boolean;

  // forceConsistentCasingInFileNames
  if (opts.forceconsistentcasinginfilenames !== undefined) {
    options.forceConsistentCasingInFileNames = opts.forceconsistentcasinginfilenames as boolean;
  }

  return options;
}

// Convert options to JSON for WASM (snake_case keys expected)
function toWasmOptions(opts: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  
  // Map all options to what WASM expects
  for (const [key, value] of Object.entries(opts)) {
    result[key] = value;
  }
  
  return result;
}

function runTsc(testCase: ParsedTestCase): number[] {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];

  for (const file of testCase.files) {
    // Determine script kind based on file extension
    let scriptKind = ts.ScriptKind.TS;
    if (file.name.endsWith('.js')) scriptKind = ts.ScriptKind.JS;
    else if (file.name.endsWith('.jsx')) scriptKind = ts.ScriptKind.JSX;
    else if (file.name.endsWith('.tsx')) scriptKind = ts.ScriptKind.TSX;
    else if (file.name.endsWith('.json')) scriptKind = ts.ScriptKind.JSON;
    
    const sf = ts.createSourceFile(
      file.name, 
      file.content, 
      compilerOptions.target ?? ts.ScriptTarget.ES2020, 
      true,
      scriptKind
    );
    sourceFiles.set(file.name, sf);
    fileNames.push(file.name);
  }

  // Add lib.d.ts unless noLib is set
  if (!compilerOptions.noLib && libSource) {
    sourceFiles.set('lib.d.ts', ts.createSourceFile(
      'lib.d.ts', 
      libSource, 
      compilerOptions.target ?? ts.ScriptTarget.ES2020, 
      true
    ));
  }

  const host = ts.createCompilerHost(compilerOptions);
  host.getSourceFile = (name, languageVersion, onError, shouldCreateNewSourceFile) => {
    return sourceFiles.get(name);
  };
  host.fileExists = (name) => sourceFiles.has(name);
  host.readFile = (name) => {
    const file = testCase.files.find(f => f.name === name);
    if (file) return file.content;
    if (name === 'lib.d.ts') return libSource;
    return undefined;
  };
  host.getDefaultLibFileName = () => 'lib.d.ts';
  host.getCurrentDirectory = () => '/';
  host.getCanonicalFileName = (name) => name;
  host.useCaseSensitiveFileNames = () => true;
  host.getNewLine = () => '\n';
  host.writeFile = () => {};

  const program = ts.createProgram(fileNames, compilerOptions, host);
  const diags: number[] = [];
  
  for (const sf of sourceFiles.values()) {
    if (sf.fileName === 'lib.d.ts') continue;
    
    for (const d of program.getSyntacticDiagnostics(sf)) diags.push(d.code);
    for (const d of program.getSemanticDiagnostics(sf)) diags.push(d.code);
  }
  
  // Also get global diagnostics
  for (const d of program.getGlobalDiagnostics()) diags.push(d.code);
  
  return diags;
}

async function runCompiler(testCase: ParsedTestCase): Promise<{ codes: number[]; crashed: boolean; oom: boolean; error?: string }> {
  const memBefore = getMemoryUsage();

  if (useWasm) {
    // WASM mode - use the loaded WASM module
    try {
      if (testCase.isMultiFile || testCase.files.length > 1) {
        const program = new wasmModule.WasmProgram();

        // Add lib.d.ts unless noLib - use addLibFile for library files
        if (!testCase.options.nolib && libSource) {
          program.addLibFile('lib.d.ts', libSource);
        }

        for (const file of testCase.files) {
          program.addFile(file.name, file.content);
        }

        const codes = Array.from(program.getAllDiagnosticCodes()) as number[];
        program.free();
        return { codes, crashed: false, oom: false };
      } else {
        const file = testCase.files[0];
        const parser = new wasmModule.Parser(file.name, file.content);

        // Add lib.d.ts unless noLib
        if (!testCase.options.nolib && libSource) {
          parser.addLibFile('lib.d.ts', libSource);
        }

        // Pass compiler options to WASM
        if (parser.setCompilerOptions) {
          parser.setCompilerOptions(JSON.stringify(toWasmOptions(testCase.options)));
        }

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
      const isOom = memGrowth > 100 * 1024 * 1024 ||
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
  } else {
    // Native mode - spawn the binary as a child process
    return new Promise((resolve) => {
      const tmpDir = fs.mkdtempSync('/tmp/tsz-test-');
      const cleanup = () => {
        try {
          fs.rmSync(tmpDir, { recursive: true, force: true });
        } catch {
          // Ignore cleanup errors
        }
      };

      try {
        // Write test files to temp directory
        const filesToCheck: string[] = [];

        // Add lib.d.ts unless noLib
        if (!testCase.options.nolib) {
          fs.writeFileSync(path.join(tmpDir, 'lib.d.ts'), libSource);
          filesToCheck.push('lib.d.ts');
        }

        // Write test files
        for (const file of testCase.files) {
          fs.writeFileSync(path.join(tmpDir, file.name), file.content);
          filesToCheck.push(file.name);
        }

        // Spawn native binary
        const { spawn } = require('child_process');
        const args = filesToCheck.map(f => path.join(tmpDir, f));
        const child = spawn(nativeBinaryPath, args, {
          cwd: tmpDir,
          stdio: ['ignore', 'pipe', 'pipe'],
        });

        let stderr = '';
        const codes: number[] = [];

        child.stderr?.on('data', (data: Buffer) => {
          stderr += data.toString();
        });

        child.on('close', (code: number | null) => {
          // Parse error codes from stderr (tsz outputs to stderr)
          const errorMatches = stderr.match(/TS(\d+)/g);
          if (errorMatches) {
            for (const match of errorMatches) {
              codes.push(parseInt(match.substring(2), 10));
            }
          }

          cleanup();
          resolve({ codes, crashed: false, oom: false });
        });

        child.on('error', (err: Error) => {
          cleanup();
          resolve({ codes: [], crashed: true, oom: false, error: err.message });
        });

        // Timeout after 10 seconds
        setTimeout(() => {
          child.kill();
          cleanup();
          resolve({ codes: [], crashed: true, oom: false, error: 'Timeout' });
        }, 10000);
      } catch (err) {
        cleanup();
        resolve({ codes: [], crashed: true, oom: false, error: String(err) });
      }
    });
  }
}

async function processTest(job: TestJob): Promise<WorkerResult> {
  lastActivity = Date.now();
  const memBefore = getMemoryUsage();

  try {
    const code = fs.readFileSync(job.filePath, 'utf8');
    const testCase = parseTestDirectives(code, job.filePath);

    // Run TSC first (more stable)
    const tscCodes = runTsc(testCase);
    lastActivity = Date.now();

    // Run compiler (WASM or native, may crash)
    const compilerResult = await runCompiler(testCase);
    lastActivity = Date.now();

    const memAfter = getMemoryUsage();

    // Try to run garbage collection if available (Node.js with --expose-gc)
    if (global.gc) {
      try {
        global.gc();
      } catch {
        // Ignore GC errors
      }
    }

    return {
      type: 'result',
      id: job.id,
      tscCodes,
      wasmCodes: compilerResult.codes,
      crashed: compilerResult.crashed,
      oom: compilerResult.oom,
      category: testCase.category,
      error: compilerResult.error,
      memoryUsed: memAfter - memBefore,
    };
  } catch (e) {
    const memAfter = getMemoryUsage();
    const isOom = (memAfter - memBefore) > 100 * 1024 * 1024;

    // Try to run garbage collection if available
    if (global.gc) {
      try {
        global.gc();
      } catch {
        // Ignore GC errors
      }
    }

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
  const data = workerData as { wasmPkgPath: string; libPath: string; useWasm: boolean; nativeBinaryPath?: string; id: number };
  workerId = data.id;
  libPath = data.libPath;
  useWasm = data.useWasm;
  nativeBinaryPath = data.nativeBinaryPath || '';

  // Load WASM module once (if using WASM)
  if (useWasm) {
    try {
      const wasmPath = path.join(data.wasmPkgPath, 'wasm.js');
      // For --target nodejs, we use require() instead of dynamic import
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      wasmModule = require(wasmPath);
    } catch (e) {
      parentPort?.postMessage({
        type: 'error',
        workerId,
        error: `Failed to load WASM: ${e instanceof Error ? e.message : e}`,
      });
      process.exit(1);
    }
  } else {
    // Verify native binary exists
    if (!fs.existsSync(nativeBinaryPath)) {
      parentPort?.postMessage({
        type: 'error',
        workerId,
        error: `Native binary not found: ${nativeBinaryPath}`,
      });
      process.exit(1);
    }
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
  parentPort!.on('message', async (job: TestJob) => {
    const result = await processTest(job);
    parentPort!.postMessage(result);
  });

  // Heartbeat - detect if we're hung
  setInterval(() => {
    const sinceLastActivity = Date.now() - lastActivity;
    if (sinceLastActivity > 30000) {
      parentPort?.postMessage({
        type: 'heartbeat',
        workerId,
        sinceLastActivity,
        memoryUsed: getMemoryUsage(),
      });
    }
  }, HEARTBEAT_INTERVAL);
})();
