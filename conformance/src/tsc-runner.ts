/**
 * Shared TSC runner for conformance testing.
 * 
 * Extracted from cache-worker.ts for reuse in:
 * - cache-worker.ts (generates TSC cache)
 * - runner-server.ts (--print-test mode)
 */

import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

// ============================================================================
// Types
// ============================================================================

export interface TestFile {
  name: string;
  content: string;
}

export interface ParsedTestCase {
  options: Record<string, unknown>;
  isMultiFile: boolean;
  files: TestFile[];
}

export interface DiagnosticInfo {
  code: number;
  message: string;
  file?: string;
  line?: number;
  column?: number;
}

export interface TscResult {
  codes: number[];
  diagnostics: DiagnosticInfo[];
}

// ============================================================================
// Lib Loading
// ============================================================================

const LIB_REFERENCE_RE = /\/\/\/\s*<reference\s+lib=["']([^"']+)["']\s*\/>/g;
const libContentCache = new Map<string, string>();
const libPathCache = new Map<string, string>();

export function parseLibReferences(source: string): string[] {
  const refs: string[] = [];
  for (const match of source.matchAll(LIB_REFERENCE_RE)) {
    if (match[1]) refs.push(match[1].trim().toLowerCase());
  }
  return refs;
}

export function resolveLibFilePath(libName: string, libDir: string): string | null {
  const normalized = libName.trim().toLowerCase();
  const cacheKey = `${libDir}:${normalized}`;
  if (libPathCache.has(cacheKey)) return libPathCache.get(cacheKey)!;
  if (!libDir || !fs.existsSync(path.join(libDir, 'es5.d.ts'))) return null;

  const candidates = [
    path.join(libDir, `${normalized}.d.ts`),
    path.join(libDir, `lib.${normalized}.d.ts`),
    path.join(libDir, `${normalized}.generated.d.ts`),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      libPathCache.set(cacheKey, candidate);
      return candidate;
    }
  }
  return null;
}

export function readLibContent(libName: string, libDir: string): string | null {
  const normalized = libName.trim().toLowerCase();
  const cacheKey = `${libDir}:${normalized}`;
  if (libContentCache.has(cacheKey)) return libContentCache.get(cacheKey)!;
  const libFilePath = resolveLibFilePath(normalized, libDir);
  if (!libFilePath) return null;
  const content = fs.readFileSync(libFilePath, 'utf8');
  libContentCache.set(cacheKey, content);
  return content;
}

function loadLibRecursive(
  libName: string,
  libDir: string,
  out: Map<string, string>,
  seen: Set<string>
): void {
  const normalized = libName.trim().toLowerCase();
  if (seen.has(normalized)) return;
  seen.add(normalized);

  const content = readLibContent(normalized, libDir);
  if (!content) return;

  out.set(`lib.${normalized}.d.ts`, content);

  for (const ref of parseLibReferences(content)) {
    loadLibRecursive(ref, libDir, out, seen);
  }
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

function normalizeTargetName(target: unknown): string {
  if (typeof target === 'number') {
    const name = (ts.ScriptTarget as Record<number, string>)[target];
    if (typeof name === 'string') return name.toLowerCase();
  }
  return String(target ?? 'es2020').toLowerCase();
}

function defaultCoreLibNameForTarget(targetName: string): string {
  switch (targetName) {
    case 'es3':
    case 'es5':
      return 'es5';
    case 'es6':
    case 'es2015':
      return 'es2015';
    case 'es2016':
      return 'es2016';
    case 'es2017':
      return 'es2017';
    case 'es2018':
      return 'es2018';
    case 'es2019':
      return 'es2019';
    case 'es2020':
      return 'es2020';
    case 'es2021':
      return 'es2021';
    case 'es2022':
      return 'es2022';
    case 'es2023':
      return 'es2023';
    case 'es2024':
      return 'es2024';
    case 'esnext':
      return 'esnext';
    default:
      return 'es5';
  }
}

function getLibNamesForTestCase(
  opts: Record<string, unknown>,
  compilerOptionsTarget: ts.ScriptTarget | undefined
): string[] {
  if (opts.nolib) return [];
  const explicit = parseLibOption(opts.lib);
  if (explicit.length > 0) return explicit;

  // No explicit @lib - return default libs based on target
  const targetName = normalizeTargetName(compilerOptionsTarget ?? opts.target);
  return [defaultCoreLibNameForTarget(targetName)];
}

export function collectLibFiles(libNames: string[], libDir: string): Map<string, string> {
  const out = new Map<string, string>();
  const seen = new Set<string>();
  for (const libName of libNames) {
    loadLibRecursive(libName, libDir, out, seen);
  }
  return out;
}

// ============================================================================
// Target/Module Mapping
// ============================================================================

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

// ============================================================================
// Test Parsing
// ============================================================================

export function parseTestDirectives(code: string, filePath: string): ParsedTestCase {
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

  // For multi-file tests, also include the main test file if it's not already included
  // This is needed so TypeScript can use the main file path as the root for resolution
  if (isMultiFile) {
    const mainFileName = path.basename(filePath);
    if (!files.some(f => f.name === mainFileName)) {
      files.push({ name: mainFileName, content: cleanLines.join('\n') });
    }
  } else {
    files.push({ name: path.basename(filePath), content: cleanLines.join('\n') });
  }

  return { options, isMultiFile, files };
}

// ============================================================================
// Compiler Options
// ============================================================================

function toCompilerOptions(opts: Record<string, unknown>): ts.CompilerOptions {
  const options: ts.CompilerOptions = {};

  // noEmit - respect the directive, default to true
  if (opts.noemit !== undefined) {
    options.noEmit = opts.noemit as boolean;
  } else {
    options.noEmit = true;
  }

  if (opts.target !== undefined) {
    const t = String(opts.target).toLowerCase();
    options.target = TARGET_MAP[t] ?? ts.ScriptTarget.ES2020;
  } else {
    options.target = ts.ScriptTarget.ES2020;
  }

  if (opts.module !== undefined) {
    const m = String(opts.module).toLowerCase();
    options.module = MODULE_MAP[m] ?? ts.ModuleKind.ESNext;
  } else {
    options.module = ts.ModuleKind.ESNext;
  }

  // Strict mode flags
  if (opts.strict !== undefined) options.strict = opts.strict as boolean;
  if (opts.noimplicitany !== undefined) options.noImplicitAny = opts.noimplicitany as boolean;
  if (opts.strictnullchecks !== undefined) options.strictNullChecks = opts.strictnullchecks as boolean;
  if (opts.strictfunctiontypes !== undefined) options.strictFunctionTypes = opts.strictfunctiontypes as boolean;
  if (opts.strictbindcallapply !== undefined) options.strictBindCallApply = opts.strictbindcallapply as boolean;
  if (opts.strictpropertyinitialization !== undefined) options.strictPropertyInitialization = opts.strictpropertyinitialization as boolean;
  if (opts.strictbuiltiniteratorreturn !== undefined) options.strictBuiltinIteratorReturn = opts.strictbuiltiniteratorreturn as boolean;
  if (opts.noimplicitthis !== undefined) options.noImplicitThis = opts.noimplicitthis as boolean;
  if (opts.useunknownincatchvariables !== undefined) options.useUnknownInCatchVariables = opts.useunknownincatchvariables as boolean;
  if (opts.alwaysstrict !== undefined) options.alwaysStrict = opts.alwaysstrict as boolean;
  if (opts.nolib !== undefined) options.noLib = opts.nolib as boolean;

  // Additional checks
  if (opts.nopropertyaccessfromindexsignature !== undefined) {
    options.noPropertyAccessFromIndexSignature = opts.nopropertyaccessfromindexsignature as boolean;
  }
  if (opts.nouncheckedindexedaccess !== undefined) {
    options.noUncheckedIndexedAccess = opts.nouncheckedindexedaccess as boolean;
  }
  if (opts.exactoptionalpropertytypes !== undefined) {
    options.exactOptionalPropertyTypes = opts.exactoptionalpropertytypes as boolean;
  }
  if (opts.noimplicitreturns !== undefined) {
    options.noImplicitReturns = opts.noimplicitreturns as boolean;
  }
  if (opts.nofallthroughcasesinswitch !== undefined) {
    options.noFallthroughCasesInSwitch = opts.nofallthroughcasesinswitch as boolean;
  }
  if (opts.nounusedlocals !== undefined) {
    options.noUnusedLocals = opts.nounusedlocals as boolean;
  }
  if (opts.nounusedparameters !== undefined) {
    options.noUnusedParameters = opts.nounusedparameters as boolean;
  }
  if (opts.allowunusedlabels !== undefined) {
    options.allowUnusedLabels = opts.allowunusedlabels as boolean;
  }
  if (opts.allowunreachablecode !== undefined) {
    options.allowUnreachableCode = opts.allowunreachablecode as boolean;
  }
  if (opts.noimplicitoverride !== undefined) {
    options.noImplicitOverride = opts.noimplicitoverride as boolean;
  }

  // JavaScript support (needed for @allowJs and @checkJs tests)
  if (opts.allowjs !== undefined) options.allowJs = opts.allowjs as boolean;
  if (opts.checkjs !== undefined) options.checkJs = opts.checkjs as boolean;

  // JSX options
  if (opts.jsx !== undefined) {
    const jsx = String(opts.jsx).toLowerCase();
    const jsxMap: Record<string, ts.JsxEmit> = {
      'preserve': ts.JsxEmit.Preserve,
      'react': ts.JsxEmit.React,
      'react-native': ts.JsxEmit.ReactNative,
      'react-jsx': ts.JsxEmit.ReactJSX,
      'react-jsxdev': ts.JsxEmit.ReactJSXDev,
    };
    options.jsx = jsxMap[jsx] ?? ts.JsxEmit.React;
  }
  if (opts.jsxfactory !== undefined) {
    options.jsxFactory = opts.jsxfactory as string;
  }
  if (opts.jsxfragmentfactory !== undefined) {
    options.jsxFragmentFactory = opts.jsxfragmentfactory as string;
  }
  if (opts.jsximportsource !== undefined) {
    options.jsxImportSource = opts.jsximportsource as string;
  }

  // Class fields and decorators
  if (opts.usedefineforclassfields !== undefined) {
    options.useDefineForClassFields = opts.usedefineforclassfields as boolean;
  }
  if (opts.experimentaldecorators !== undefined) {
    options.experimentalDecorators = opts.experimentaldecorators as boolean;
  }
  if (opts.emitdecoratormetadata !== undefined) {
    options.emitDecoratorMetadata = opts.emitdecoratormetadata as boolean;
  }

  // Module options
  if (opts.allowsyntheticdefaultimports !== undefined) {
    options.allowSyntheticDefaultImports = opts.allowsyntheticdefaultimports as boolean;
  }
  if (opts.esmoduleinterop !== undefined) {
    options.esModuleInterop = opts.esmoduleinterop as boolean;
  }
  if (opts.preservesymlinks !== undefined) {
    options.preserveSymlinks = opts.preservesymlinks as boolean;
  }
  if (opts.allowumdglobalaccess !== undefined) {
    options.allowUmdGlobalAccess = opts.allowumdglobalaccess as boolean;
  }
  if (opts.allowimportingtsextensions !== undefined) {
    options.allowImportingTsExtensions = opts.allowimportingtsextensions as boolean;
  }
  if (opts.resolvejsonmodule !== undefined) {
    options.resolveJsonModule = opts.resolvejsonmodule as boolean;
  }
  if (opts.noresolve !== undefined) {
    options.noResolve = opts.noresolve as boolean;
  }

  // Interop constraints
  if (opts.isolatedmodules !== undefined) {
    options.isolatedModules = opts.isolatedmodules as boolean;
  }
  if (opts.verbatimmodulesyntax !== undefined) {
    options.verbatimModuleSyntax = opts.verbatimmodulesyntax as boolean;
  }

  // Declaration
  if (opts.declaration !== undefined) {
    options.declaration = opts.declaration as boolean;
  }

  // Skip lib check
  if (opts.skiplibcheck !== undefined) {
    options.skipLibCheck = opts.skiplibcheck as boolean;
  }
  if (opts.skipdefaultlibcheck !== undefined) {
    options.skipDefaultLibCheck = opts.skipdefaultlibcheck as boolean;
  }

  return options;
}

// ============================================================================
// Main TSC Runner
// ============================================================================

/**
 * Run TSC on a test case and return results.
 * 
 * @param testCase Parsed test case with files and options
 * @param libDir Directory containing lib.*.d.ts files
 * @param libSource Optional fallback lib.d.ts content
 * @param includeMessages If true, include full diagnostic messages (slower)
 */
export function runTsc(
  testCase: ParsedTestCase,
  libDir: string,
  libSource: string = '',
  includeMessages: boolean = false,
  rootFilePath: string = ''
): TscResult {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];
  const libNames = getLibNamesForTestCase(testCase.options, compilerOptions.target);
  const libFiles = libNames.length ? collectLibFiles(libNames, libDir) : new Map<string, string>();

  // Use rootFilePath to resolve relative file names
  const rootDir = rootFilePath ? path.dirname(rootFilePath) : '.';

  for (const file of testCase.files) {
    let scriptKind = ts.ScriptKind.TS;
    if (file.name.endsWith('.js')) scriptKind = ts.ScriptKind.JS;
    else if (file.name.endsWith('.jsx')) scriptKind = ts.ScriptKind.JSX;
    else if (file.name.endsWith('.tsx')) scriptKind = ts.ScriptKind.TSX;
    else if (file.name.endsWith('.json')) scriptKind = ts.ScriptKind.JSON;

    // Resolve file name relative to root directory
    const resolvedFileName = path.resolve(rootDir, file.name);
    const sf = ts.createSourceFile(
      resolvedFileName,
      file.content,
      compilerOptions.target ?? ts.ScriptTarget.ES2020,
      true,
      scriptKind
    );
    sourceFiles.set(resolvedFileName, sf);
    fileNames.push(resolvedFileName);
  }

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
    sourceFiles.set('lib.d.ts', ts.createSourceFile(
      'lib.d.ts',
      libSource,
      compilerOptions.target ?? ts.ScriptTarget.ES2020,
      true
    ));
  }

  const host = ts.createCompilerHost(compilerOptions);
  host.getSourceFile = (name) => sourceFiles.get(name) ?? sourceFiles.get(path.basename(name));
  host.fileExists = (name) => sourceFiles.has(name) || sourceFiles.has(path.basename(name));
  host.readFile = (name) => {
    const sf = sourceFiles.get(name);
    if (sf) return sf.getFullText();

    // Try to find the file by name in testCase.files
    const file = testCase.files.find(f => {
      const resolved = path.resolve(rootDir, f.name);
      return resolved === name || path.basename(name) === name;
    });
    if (file) return file.content;

    const libName = libFiles.has(name) ? name : path.basename(name);
    if (libFiles.has(libName)) return libFiles.get(libName);
    if (libSource && libName === 'lib.d.ts') return libSource;
    return undefined;
  };
  host.getDefaultLibFileName = () => {
    if (libFiles.size > 0) {
      if (sourceFiles.has('lib.es5.d.ts')) return 'lib.es5.d.ts';
      for (const name of sourceFiles.keys()) {
        if (name.startsWith('lib.') && name.endsWith('.d.ts')) {
          return name;
        }
      }
    }
    return 'lib.d.ts';
  };
  host.getCurrentDirectory = () => rootFilePath ? path.dirname(rootFilePath) : '/';
  host.getCanonicalFileName = (name) => name;
  host.useCaseSensitiveFileNames = () => true;
  host.getNewLine = () => '\n';
  host.writeFile = () => {};

  const program = ts.createProgram(fileNames, compilerOptions, host);
  const codes: number[] = [];
  const diagnostics: DiagnosticInfo[] = [];

  const processDiagnostic = (d: ts.Diagnostic) => {
    codes.push(d.code);
    if (includeMessages) {
      const message = ts.flattenDiagnosticMessageText(d.messageText, '\n');
      const info: DiagnosticInfo = { code: d.code, message };
      if (d.file && d.start !== undefined) {
        const { line, character } = d.file.getLineAndCharacterOfPosition(d.start);
        info.file = d.file.fileName;
        info.line = line + 1;
        info.column = character + 1;
      }
      diagnostics.push(info);
    }
  };

  for (const sf of sourceFiles.values()) {
    if (sf.fileName.startsWith('lib.')) continue;
    for (const d of program.getSyntacticDiagnostics(sf)) processDiagnostic(d);
    for (const d of program.getSemanticDiagnostics(sf)) processDiagnostic(d);
  }

  for (const d of program.getGlobalDiagnostics()) processDiagnostic(d);

  return { codes, diagnostics };
}

/**
 * Run TSC on files (map of filename -> content) and return results.
 * Convenience wrapper for use with CheckOptions-style input.
 */
export function runTscOnFiles(
  files: Record<string, string>,
  options: Record<string, unknown>,
  libDir: string,
  includeMessages: boolean = false
): TscResult {
  // Convert to ParsedTestCase format
  const testFiles: TestFile[] = Object.entries(files).map(([name, content]) => ({ name, content }));
  
  // Normalize option keys to lowercase for toCompilerOptions
  const testOptions: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(options)) {
    testOptions[key.toLowerCase()] = value;
  }

  const testCase: ParsedTestCase = {
    options: testOptions,
    isMultiFile: testFiles.length > 1,
    files: testFiles,
  };

  return runTsc(testCase, libDir, '', includeMessages);
}
