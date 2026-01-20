/**
 * Single test executor - runs in isolated child process
 * 
 * This file is spawned as a child process for each test.
 * If it hangs, the parent can kill it without affecting other tests.
 */

import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';

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

interface TestOutput {
  tscCodes: number[];
  wasmCodes: number[];
  crashed: boolean;
  error?: string;
  category: string;
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

function runTsc(testCase: ParsedTestCase, libSource: string): number[] {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];

  for (const file of testCase.files) {
    const sf = ts.createSourceFile(file.name, file.content, ts.ScriptTarget.ES2020, true);
    sourceFiles.set(file.name, sf);
    fileNames.push(file.name);
  }

  if (!testCase.options.nolib && libSource) {
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

  return allDiagnostics.map(d => d.code);
}

async function runWasm(testCase: ParsedTestCase, libSource: string, wasmPkgPath: string): Promise<{ codes: number[]; crashed: boolean; error?: string }> {
  try {
    const wasmPath = path.join(wasmPkgPath, 'wasm.js');
    const module = await import(wasmPath);
    if (typeof module.default === 'function') {
      await module.default();
    }

    const wasm = module as {
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
      if (!testCase.options.nolib && libSource) program.addFile('lib.d.ts', libSource);
      for (const file of testCase.files) program.addFile(file.name, file.content);
      const codes = program.getAllDiagnosticCodes();
      return { codes: Array.from(codes), crashed: false };
    } else {
      const file = testCase.files[0];
      const parser = new wasm.ThinParser(file.name, file.content);
      if (!testCase.options.nolib && libSource) parser.addLibFile('lib.d.ts', libSource);
      if (parser.setCompilerOptions) parser.setCompilerOptions(JSON.stringify(testCase.options));
      parser.parseSourceFile();
      const parseDiags = JSON.parse(parser.getDiagnosticsJson());
      const checkResult = JSON.parse(parser.checkSourceFile());
      const codes = [
        ...parseDiags.map((d: { code: number }) => d.code),
        ...(checkResult.diagnostics || []).map((d: { code: number }) => d.code),
      ];
      parser.free();
      return { codes, crashed: false };
    }
  } catch (error) {
    return { codes: [], crashed: true, error: error instanceof Error ? error.message : String(error) };
  }
}

async function main() {
  const args = process.argv.slice(2);
  const filePath = args[0];
  const wasmPkgPath = args[1];
  const libPath = args[2];

  if (!filePath || !wasmPkgPath) {
    console.error(JSON.stringify({ error: 'Missing arguments' }));
    process.exit(1);
  }

  try {
    const code = fs.readFileSync(filePath, 'utf8');
    const libSource = libPath && fs.existsSync(libPath) ? fs.readFileSync(libPath, 'utf8') : '';
    const testCase = parseTestDirectives(code, filePath);

    const tscCodes = runTsc(testCase, libSource);
    const wasmResult = await runWasm(testCase, libSource, wasmPkgPath);

    const output: TestOutput = {
      tscCodes,
      wasmCodes: wasmResult.codes,
      crashed: wasmResult.crashed,
      error: wasmResult.error,
      category: testCase.category,
    };

    console.log(JSON.stringify(output));
    process.exit(0);
  } catch (error) {
    console.log(JSON.stringify({
      tscCodes: [],
      wasmCodes: [],
      crashed: true,
      error: error instanceof Error ? error.message : String(error),
      category: 'unknown',
    }));
    process.exit(1);
  }
}

main();
