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
const testsBasePath: string = workerData.testsBasePath;

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

  if (!compilerOptions.noLib && libSource) {
    sourceFiles.set('lib.d.ts', ts.createSourceFile(
      'lib.d.ts',
      libSource,
      compilerOptions.target ?? ts.ScriptTarget.ES2020,
      true
    ));
  }

  const host = ts.createCompilerHost(compilerOptions);
  host.getSourceFile = (name) => sourceFiles.get(name);
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
