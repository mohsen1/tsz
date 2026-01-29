/**
 * Worker thread for TSC cache generation
 *
 * Runs TypeScript compiler on test files and returns diagnostic codes.
 */

import { parentPort, workerData } from 'worker_threads';
import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

interface TestFile {
  name: string;
  content: string;
}

interface ParsedTestCase {
  options: Record<string, unknown>;
  isMultiFile: boolean;
  files: TestFile[];
}

const libSource: string = workerData.libSource;
const libDir: string = workerData.libDir;
const testsBasePath: string = workerData.testsBasePath;
const hasLibDir = !!libDir && fs.existsSync(path.join(libDir, 'es5.d.ts'));

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
  if (!hasLibDir) return null;

  const candidates = [
    path.join(libDir, `${normalized}.d.ts`),
    path.join(libDir, `${normalized}.generated.d.ts`),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      libPathCache.set(normalized, candidate);
      return candidate;
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

function collectLibFiles(libNames: string[]): Map<string, string> {
  const out = new Map<string, string>();
  const seen = new Set<string>();
  for (const libName of libNames) {
    loadLibRecursive(libName, out, seen);
  }
  return out;
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

  return { options, isMultiFile, files };
}

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

function runTsc(testCase: ParsedTestCase): number[] {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];
  const libNames = getLibNamesForTestCase(testCase.options, compilerOptions.target);
  const libFiles = libNames.length ? collectLibFiles(libNames) : new Map<string, string>();

  // DON'T set compilerOptions.lib - it causes tsc to look for libs at absolute paths
  // Instead, we provide libs via getSourceFile/getDefaultLibFileName

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
  // Return the base lib file that's in sourceFiles
  // For ES5 target, this is lib.es5.d.ts; tsc will follow /// <reference lib="..." /> directives
  host.getDefaultLibFileName = () => {
    // If we have lib files loaded, return the base lib
    if (libFiles.size > 0) {
      // Find the base lib (es5 or the lowest available)
      if (sourceFiles.has('lib.es5.d.ts')) return 'lib.es5.d.ts';
      // Fallback to first lib file
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
  const diags: number[] = [];

  for (const sf of sourceFiles.values()) {
    if (sf.fileName.startsWith('lib.')) continue;
    for (const d of program.getSyntacticDiagnostics(sf)) diags.push(d.code);
    for (const d of program.getSemanticDiagnostics(sf)) diags.push(d.code);
  }

  for (const d of program.getGlobalDiagnostics()) diags.push(d.code);

  return diags;
}

parentPort!.on('message', (msg: { id: number; filePath: string }) => {
  try {
    const code = fs.readFileSync(msg.filePath, 'utf8');
    const testCase = parseTestDirectives(code, msg.filePath);
    const codes = runTsc(testCase);
    parentPort!.postMessage({ id: msg.id, codes });
  } catch (e) {
    parentPort!.postMessage({
      id: msg.id,
      codes: [],
      error: e instanceof Error ? e.message : String(e),
    });
  }
});

parentPort!.postMessage({ type: 'ready' });
