/**
 * Persistent worker thread for test execution
 *
 * Loads WASM once at startup, then processes tests as messages arrive.
 * Includes crash detection and graceful error reporting.
 *
 * Uses pre-computed TSC results from cache when available.
 */

import { parentPort, workerData } from 'worker_threads';
import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';
import { hashContent, type CacheEntry } from './tsc-cache.js';
import {
  loadLibManifest,
  normalizeLibName,
  resolveLibWithDependencies,
  type LibManifest,
} from './lib-manifest.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

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
let libDir = '';
let tsLibDir = ''; // TypeScript node_modules lib directory (for es2015, dom, etc.)
let hasLibDir = false;
let workerId = -1;
let useWasm = true;
let nativeBinaryPath = '';
let nativeBinary: any = null;
let tscCacheEntries: Record<string, CacheEntry> | undefined = undefined;
let libManifest: LibManifest | null = null;

// Memory monitoring
const getWasmMemoryUsage = () => {
  try {
    return wasmModule?.memory?.buffer?.byteLength ?? 0;
  } catch {
    return 0;
  }
};
const getMemoryUsage = () => process.memoryUsage().heapUsed + getWasmMemoryUsage();

// Heartbeat to detect hangs
let lastActivity = Date.now();
const HEARTBEAT_INTERVAL = 1000;

const LIB_REFERENCE_RE = /\/\/\/\s*<reference\s+lib=["']([^"']+)["']\s*\/>/g;
const libContentCache = new Map<string, string>();
const libPathCache = new Map<string, string>();

function parseLibReferences(source: string): string[] {
  const refs: string[] = [];
  for (const match of source.matchAll(LIB_REFERENCE_RE)) {
    if (match[1]) refs.push(match[1].trim().toLowerCase());
  }
  return refs;
}

function resolveLibFilePath(libName: string): string | null {
  const normalized = libName.trim().toLowerCase();
  if (libPathCache.has(normalized)) return libPathCache.get(normalized)!;

  // First check tests/lib directory (for lib.d.ts, react.d.ts, etc.)
  if (hasLibDir && libDir) {
    const candidates = [
      path.join(libDir, `lib.${normalized}.d.ts`),
      path.join(libDir, `${normalized}.d.ts`),
      path.join(libDir, `${normalized}.generated.d.ts`),
    ];
    for (const candidate of candidates) {
      if (fs.existsSync(candidate)) {
        libPathCache.set(normalized, candidate);
        return candidate;
      }
    }
  }

  // Fallback to TypeScript node_modules lib directory (for es2015, dom, etc.)
  if (tsLibDir) {
    const candidates = [
      path.join(tsLibDir, `lib.${normalized}.d.ts`),
    ];
    for (const candidate of candidates) {
      if (fs.existsSync(candidate)) {
        libPathCache.set(normalized, candidate);
        return candidate;
      }
    }
  }

  return null;
}

function readLibContent(libName: string): string | null {
  const normalized = libName.trim().toLowerCase();
  if (libContentCache.has(normalized)) return libContentCache.get(normalized)!;
  const libFilePath = resolveLibFilePath(normalized);
  if (!libFilePath) return null;
  const content = fs.readFileSync(libFilePath, 'utf8');
  libContentCache.set(normalized, content);
  return content;
}

function loadLibRecursive(
  libName: string,
  out: Map<string, string>,
  seen: Set<string>
): void {
  const normalized = libName.trim().toLowerCase();
  if (seen.has(normalized)) return;
  seen.add(normalized);

  const content = readLibContent(normalized);
  if (!content) return;

  out.set(`lib.${normalized}.d.ts`, content);

  for (const ref of parseLibReferences(content)) {
    loadLibRecursive(ref, out, seen);
  }
}

function normalizeTargetName(target: unknown): string {
  if (typeof target === 'number') {
    const name = (ts.ScriptTarget as any)[target];
    if (typeof name === 'string') return name.toLowerCase();
  }
  return String(target ?? 'es2020').toLowerCase();
}

function defaultFullLibNameForTarget(targetName: string): string {
  switch (targetName) {
    case 'es3':
    case 'es5':
      return 'es5.full';
    case 'es6':
    case 'es2015':
      return 'es2015.full';
    case 'es2016':
    case 'es2017':
    case 'es2018':
    case 'es2019':
    case 'es2020':
    case 'es2021':
    case 'es2022':
    case 'es2023':
    case 'es2024':
      return `${targetName}.full`;
    case 'esnext':
    default:
      return 'esnext.full';
  }
}

function defaultLibNamesForTarget(targetName: string): string[] {
  const fullName = defaultFullLibNameForTarget(targetName);
  const fullContent = readLibContent(fullName);
  if (!fullContent) {
    return [targetName === 'es6' ? 'es2015' : targetName];
  }
  const refs = parseLibReferences(fullContent);
  return refs.length ? refs : [targetName === 'es6' ? 'es2015' : targetName];
}

function parseLibOption(libOpt: unknown): string[] {
  if (typeof libOpt === 'string') {
    return libOpt
      .split(',')
      .map(s => s.trim().toLowerCase())
      .filter(Boolean);
  }
  if (Array.isArray(libOpt)) {
    return libOpt.map(v => String(v).trim().toLowerCase()).filter(Boolean);
  }
  return [];
}

function getLibNamesForTestCase(
  opts: Record<string, unknown>,
  compilerOptionsTarget: ts.ScriptTarget | undefined
): string[] {
  if (opts.nolib) return [];
  const explicit = parseLibOption(opts.lib);
  return explicit;
}

function collectLibFiles(libNames: string[]): Map<string, string> {
  const out = new Map<string, string>();
  const seen = new Set<string>();

  // If manifest is available, use it for more accurate dependency resolution
  if (libManifest) {
    const resolvedNames = new Set<string>();
    for (const libName of libNames) {
      const deps = resolveLibWithDependencies(libName, libManifest);
      for (const dep of deps) {
        resolvedNames.add(dep);
      }
    }
    for (const libName of resolvedNames) {
      loadLibRecursive(libName, out, seen);
    }
  } else {
    // Fall back to file-based resolution
    for (const libName of libNames) {
      loadLibRecursive(libName, out, seen);
    }
  }

  return out;
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

  const mapping: Record<string, string> = {
    strict: 'strict',
    noimplicitany: 'noImplicitAny',
    strictnullchecks: 'strictNullChecks',
    strictfunctiontypes: 'strictFunctionTypes',
    strictpropertyinitialization: 'strictPropertyInitialization',
    noimplicitreturns: 'noImplicitReturns',
    noimplicitthis: 'noImplicitThis',
    target: 'target',
    module: 'module',
    nolib: 'noLib',
    lib: 'lib',
  };

  for (const [key, value] of Object.entries(opts)) {
    const mapped = mapping[key] ?? key;
    result[mapped] = value;
  }

  return result;
}

function runTsc(testCase: ParsedTestCase): number[] {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];
  const libNames = getLibNamesForTestCase(testCase.options, compilerOptions.target);
  const libFiles = libNames.length ? collectLibFiles(libNames) : new Map<string, string>();

  if (!compilerOptions.noLib && libNames.length) {
    compilerOptions.lib = libNames;
  }

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

  // Add lib files unless noLib is set
  if (!compilerOptions.noLib && libFiles.size) {
    for (const [name, content] of libFiles.entries()) {
      sourceFiles.set(
        name,
        ts.createSourceFile(
          name,
          content,
          compilerOptions.target ?? ts.ScriptTarget.ES2020,
          true
        )
      );
    }
  } else if (!compilerOptions.noLib && libSource) {
    sourceFiles.set(
      'lib.d.ts',
      ts.createSourceFile(
        'lib.d.ts',
        libSource,
        compilerOptions.target ?? ts.ScriptTarget.ES2020,
        true
      )
    );
  }

  const host = ts.createCompilerHost(compilerOptions);
  host.getSourceFile = (name, languageVersion, onError, shouldCreateNewSourceFile) => {
    return sourceFiles.get(name) ?? sourceFiles.get(path.basename(name));
  };
  host.fileExists = (name) => sourceFiles.has(name) || sourceFiles.has(path.basename(name));
  host.readFile = (name) => {
    const file = testCase.files.find(f => f.name === name);
    if (file) return file.content;
    const libName = libFiles.has(name) ? name : path.basename(name);
    if (libFiles.has(libName)) return libFiles.get(libName);
    if (libSource && libName === 'lib.d.ts') return libSource;
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
    if (sf.fileName.startsWith('lib.')) continue;
    
    for (const d of program.getSyntacticDiagnostics(sf)) diags.push(d.code);
    for (const d of program.getSemanticDiagnostics(sf)) diags.push(d.code);
  }
  
  // Also get global diagnostics
  for (const d of program.getGlobalDiagnostics()) diags.push(d.code);
  
  return diags;
}

async function runCompiler(testCase: ParsedTestCase): Promise<{ codes: number[]; crashed: boolean; oom: boolean; error?: string }> {
  const memBefore = getMemoryUsage();
  const wasmLibNames = getLibNamesForTestCase(testCase.options, undefined);
  const wasmLibFiles = wasmLibNames.length ? collectLibFiles(wasmLibNames) : new Map<string, string>();

  if (useWasm) {
    // WASM mode - use the loaded WASM module
    try {
      if (testCase.isMultiFile || testCase.files.length > 1) {
        const program = new wasmModule.WasmProgram();

        if (program.setCompilerOptions) {
          program.setCompilerOptions(JSON.stringify(toWasmOptions(testCase.options)));
        }

        // Add lib files unless noLib - use addLibFile for library files
        if (!testCase.options.nolib) {
          if (wasmLibFiles.size) {
            for (const [name, content] of wasmLibFiles.entries()) {
              program.addLibFile(name, content);
            }
          } else if (libSource && !wasmLibNames.length) {
            // No explicit libs - load default lib.d.ts (ES5 base)
            // Note: The WASM compiler will use embedded ES2015+ libs based on target
            program.addLibFile('lib.d.ts', libSource);
          }
        }

        for (const file of testCase.files) {
          program.addFile(file.name, file.content);
        }

        // Safely extract diagnostic codes with defensive checks
        let codes: number[] = [];
        try {
          const diagCodes = program.getAllDiagnosticCodes();
          codes = Array.isArray(diagCodes) ? diagCodes as number[] :
                   Array.from(diagCodes || []).map((c: any) => typeof c === 'number' ? c : 0);
        } catch (e) {
          codes = [];
        }

        program.free();
        return { codes, crashed: false, oom: false };
      } else {
        const file = testCase.files[0];
        const parser = new wasmModule.Parser(file.name, file.content);

        // Add lib files unless noLib
        if (!testCase.options.nolib) {
          if (wasmLibFiles.size) {
            for (const [name, content] of wasmLibFiles.entries()) {
              parser.addLibFile(name, content);
            }
          } else if (libSource && !wasmLibNames.length) {
            // No explicit libs - load default lib.d.ts (ES5 base)
            // Note: The WASM compiler will use embedded ES2015+ libs based on target
            parser.addLibFile('lib.d.ts', libSource);
          }
        }

        // Pass compiler options to WASM
        if (parser.setCompilerOptions) {
          parser.setCompilerOptions(JSON.stringify(toWasmOptions(testCase.options)));
        }

        parser.parseSourceFile();

        // Safely extract diagnostics with defensive checks
        let parseDiags: any[] = [];
        try {
          const parseResult = parser.getDiagnosticsJson();
          if (parseResult) {
            const parsed = JSON.parse(parseResult);
            parseDiags = Array.isArray(parsed) ? parsed : [];
          }
        } catch (e) {
          // If parsing fails, use empty array
          parseDiags = [];
        }

        let checkDiags: any[] = [];
        try {
          const checkResult = parser.checkSourceFile();
          if (checkResult) {
            const parsed = JSON.parse(checkResult);
            checkDiags = Array.isArray(parsed?.diagnostics) ? parsed.diagnostics : [];
          }
        } catch (e) {
          // If parsing fails, use empty array
          checkDiags = [];
        }

        // Extract diagnostic codes with null checks
        const codes = [
          ...parseDiags.filter((d: any) => d && typeof d.code === 'number').map((d: any) => d.code),
          ...checkDiags.filter((d: any) => d && typeof d.code === 'number').map((d: any) => d.code),
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

        // For native mode, write lib.d.ts to the directory (for reference)
        // but don't add it to the args list - the native CLI handles lib loading internally
        // and parsing the huge lib.d.ts file for each test is too slow
        if (!testCase.options.nolib && libSource) {
          fs.writeFileSync(path.join(tmpDir, 'lib.d.ts'), libSource);
          // Don't add to filesToCheck - native CLI handles its own lib loading
        }

        // Write test files
        for (const file of testCase.files) {
          const filePath = path.join(tmpDir, file.name);
          // Ensure parent directories exist for nested paths like node_modules/...
          const parentDir = path.dirname(filePath);
          if (parentDir !== tmpDir) {
            fs.mkdirSync(parentDir, { recursive: true });
          }
          fs.writeFileSync(filePath, file.content);
          filesToCheck.push(file.name);
        }

        // Spawn native binary
        const { spawn } = require('child_process');
        const args: string[] = [];
        
        // Add compiler options as CLI arguments
        const opts = testCase.options;
        if (opts.target) args.push('--target', String(opts.target));
        if (opts.lib) {
          const libVal = opts.lib;
          if (typeof libVal === 'string') {
            // Split comma-separated lib names
            const libNames = libVal.split(',').map(s => s.trim());
            for (const libName of libNames) {
              args.push('--lib', libName);
            }
          } else if (Array.isArray(libVal)) {
            args.push('--lib', libVal.join(','));
          }
        }
        if (opts.nolib) args.push('--noLib');
        if (opts.strict) args.push('--strict');
        if (opts.strictnullchecks !== undefined) args.push(opts.strictnullchecks ? '--strictNullChecks' : '--strictNullChecks=false');
        if (opts.strictfunctiontypes !== undefined) args.push(opts.strictfunctiontypes ? '--strictFunctionTypes' : '--strictFunctionTypes=false');
        if (opts.noimplicitany !== undefined) args.push(opts.noimplicitany ? '--noImplicitAny' : '--noImplicitAny=false');
        if (opts.noimplicitreturns) args.push('--noImplicitReturns');
        if (opts.noimplicitthis) args.push('--noImplicitThis');
        if (opts.module) args.push('--module', String(opts.module));
        if (opts.jsx) args.push('--jsx', String(opts.jsx));
        if (opts.allowjs) args.push('--allowJs');
        if (opts.checkjs) args.push('--checkJs');
        if (opts.declaration) args.push('--declaration');
        if (opts.noemit) args.push('--noEmit');
        
        // Add file paths
        args.push(...filesToCheck.map(f => path.join(tmpDir, f)));
        
        // Run from project directory so CLI can find its built-in lib files
        const projectDir = path.resolve(__dirname, '../..');
        const child = spawn(nativeBinaryPath, args, {
          cwd: projectDir,
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

          // Exit code 1 with error codes is normal (tsz reports type errors)
          // Only treat as crash if:
          // 1. Exit code is not 0 or 1, OR
          // 2. Exit code is 1 but no error codes found (unexpected error)
          const hasErrors = codes.length > 0;
          const actuallyCrashed = (code !== 0 && code !== 1) || (code === 1 && !hasErrors);

          cleanup();
          resolve({ codes, crashed: actuallyCrashed, oom: false });
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

    // Try to use cached TSC result
    let tscCodes: number[];
    const relPath = job.filePath.replace(job.testsBasePath + path.sep, '');
    const cachedEntry = tscCacheEntries?.[relPath];

    if (cachedEntry && cachedEntry.hash === hashContent(code)) {
      // Use cached result
      tscCodes = cachedEntry.codes;
    } else {
      // Run TSC (cache miss or file changed)
      tscCodes = runTsc(testCase);
    }
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
  const data = workerData as { wasmPkgPath: string; libPath: string; libDir: string; useWasm: boolean; nativeBinaryPath?: string; id: number; tscCacheEntries?: Record<string, CacheEntry> };
  workerId = data.id;
  libPath = data.libPath;
  libDir = data.libDir;
  useWasm = data.useWasm;
  nativeBinaryPath = data.nativeBinaryPath || '';
  tscCacheEntries = data.tscCacheEntries;

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

  // Load lib.d.ts once (if libPath is a file) or set libDir (if directory)
  try {
    const stat = fs.statSync(libPath);
    if (stat.isDirectory()) {
      libDir = libPath;
      hasLibDir = fs.existsSync(path.join(libDir, 'es5.d.ts'));
    } else {
      libSource = fs.readFileSync(libPath, 'utf8');
      hasLibDir = fs.existsSync(path.join(libDir, 'es5.d.ts'));
    }
  } catch {}

  // Set TypeScript lib directory (for es2015, dom, etc.)
  try {
    // Try multiple possible locations for TypeScript lib files
    const candidates = [
      path.resolve(__dirname, '../../TypeScript/node_modules/typescript/lib'),
      path.resolve(process.cwd(), 'TypeScript/node_modules/typescript/lib'),
      '/work/TypeScript/node_modules/typescript/lib',
    ];
    for (const candidate of candidates) {
      if (fs.existsSync(candidate)) {
        tsLibDir = candidate;
        break;
      }
    }
  } catch {}

  // Load lib manifest for consistent resolution (optional - falls back to file-based)
  try {
    libManifest = loadLibManifest();
  } catch {
    // Manifest not available, use file-based resolution
  }

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
