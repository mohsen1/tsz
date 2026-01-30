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
  if (!isMultiFile) {
    files.push({ name: path.basename(filePath), content: cleanLines.join('\n') });
  }

  return { options, isMultiFile, files };
}

// ============================================================================
// Compiler Options
// ============================================================================

function toCompilerOptions(opts: Record<string, unknown>): ts.CompilerOptions {
  const options: ts.CompilerOptions = { noEmit: true };

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

  if (opts.strict !== undefined) options.strict = opts.strict as boolean;
  if (opts.noimplicitany !== undefined) options.noImplicitAny = opts.noimplicitany as boolean;
  if (opts.strictnullchecks !== undefined) options.strictNullChecks = opts.strictnullchecks as boolean;
  if (opts.nolib !== undefined) options.noLib = opts.nolib as boolean;

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
  includeMessages: boolean = false
): TscResult {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];
  const libNames = getLibNamesForTestCase(testCase.options, compilerOptions.target);
  const libFiles = libNames.length ? collectLibFiles(libNames, libDir) : new Map<string, string>();

  for (const file of testCase.files) {
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
    const file = testCase.files.find(f => f.name === name);
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
  host.getCurrentDirectory = () => '/';
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
  options: { target?: string; strict?: boolean; strictNullChecks?: boolean; noLib?: boolean; lib?: string[] },
  libDir: string,
  includeMessages: boolean = false
): TscResult {
  // Convert to ParsedTestCase format
  const testFiles: TestFile[] = Object.entries(files).map(([name, content]) => ({ name, content }));
  const testOptions: Record<string, unknown> = {};
  
  if (options.target) testOptions.target = options.target;
  if (options.strict) testOptions.strict = options.strict;
  if (options.strictNullChecks !== undefined) testOptions.strictnullchecks = options.strictNullChecks;
  if (options.noLib) testOptions.nolib = options.noLib;
  if (options.lib) testOptions.lib = options.lib;

  const testCase: ParsedTestCase = {
    options: testOptions,
    isMultiFile: testFiles.length > 1,
    files: testFiles,
  };

  return runTsc(testCase, libDir, '', includeMessages);
}
