#!/usr/bin/env node
/**
 * TSZ Emit Test Runner
 *
 * Compares tsz JavaScript/Declaration emit output against TypeScript's baselines.
 * Runs tests in parallel with configurable concurrency and timeout.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { fileURLToPath } from 'url';
import pc from 'picocolors';
import pLimit from 'p-limit';
import { parseBaseline, getEmitDiff, getEmitDiffSummary } from './baseline-parser.js';
import { CliTranspiler } from './cli-transpiler.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');
const TS_DIR = path.join(ROOT_DIR, 'TypeScript');
const BASELINES_DIR = path.join(TS_DIR, 'tests/baselines/reference');
const CACHE_DIR = path.join(__dirname, '../.cache');
const DTS_DISCOVERY_CACHE = path.join(CACHE_DIR, 'dts-baseline-index.json');

const DEFAULT_TIMEOUT_MS = 5000;

// ============================================================================
// Types
// ============================================================================

interface Config {
  maxTests: number;
  offset: number;
  filter: string;
  verbose: boolean;
  jsOnly: boolean;
  dtsOnly: boolean;
  concurrency: number;
  timeoutMs: number;
  jsonOut: string | null;
}

interface TestCase {
  baselineFile: string;
  testPath: string | null;
  sourceFileName: string | null;
  sourceFiles: Array<{ name: string; content: string }>;
  source: string;
  expectedJs: string | null;
  expectedJsFileName: string | null;
  expectedDts: string | null;
  expectedDtsFileName: string | null;
  target: number;
  module: number;
  alwaysStrict: boolean;
  sourceMap: boolean;
  inlineSourceMap: boolean;
  downlevelIteration: boolean;
  noEmitHelpers: boolean;
  noEmitOnError: boolean;
  importHelpers: boolean;
  esModuleInterop: boolean;
  useDefineForClassFields?: boolean;
  experimentalDecorators: boolean;
  emitDecoratorMetadata: boolean;
  strictNullChecks?: boolean;
  jsx?: string;
  jsxFactory?: string;
  jsxFragmentFactory?: string;
  jsxImportSource?: string;
  moduleDetection?: string;
  preserveConstEnums: boolean;
  verbatimModuleSyntax: boolean;
  rewriteRelativeImportExtensions: boolean;
  isolatedModules: boolean;
  importsNotUsedAsValues?: string;
  preserveValueImports: boolean;
  removeComments: boolean;
  stripInternal: boolean;
  outFile?: string;
  emitDeclarationOnly: boolean;
  declarationMap: boolean;
}

interface TestResult {
  name: string;
  testPath: string | null;
  jsMatch: boolean | null;
  dtsMatch: boolean | null;
  jsError?: string;
  dtsError?: string;
  elapsed?: number;
  skipped?: boolean;
  timeout?: boolean;
}

function summarizeErrorMessage(message: string): string {
  const normalized = message.replace(/\r\n/g, '\n');
  const lines = normalized.split('\n').map(l => l.trim()).filter(Boolean);
  if (lines.length === 0) return 'Unknown error';
  const tsDiag = lines.find(l => /\bTS\d{4}\b/.test(l));
  if (tsDiag) return tsDiag;
  const commandFailed = lines.find(l => l.startsWith('Command failed:'));
  if (commandFailed) return commandFailed;
  return lines[0];
}

interface CacheEntry {
  hash: string;
  jsOutput: string;
  dtsOutput: string | null;
}

interface DtsDiscoveryEntry {
  mtimeMs: number;
  size: number;
  hasDts: boolean;
}

type DtsDiscoveryCache = Record<string, DtsDiscoveryEntry>;

// ============================================================================
// Cache Management
// ============================================================================

function hashString(str: string): string {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash;
  }
  return hash.toString(36);
}

function getCacheKey(
  sourceKey: string,
  target: number,
  module: number,
  alwaysStrict: boolean,
  declaration: boolean,
  sourceMap: boolean = false,
  inlineSourceMap: boolean = false,
  downlevelIteration: boolean = false,
  noEmitHelpers: boolean = false,
  noEmitOnError: boolean = false,
  importHelpers: boolean = false,
  esModuleInterop: boolean = false,
  useDefineForClassFields: string = '',
  experimentalDecorators: boolean = false,
  emitDecoratorMetadata: boolean = false,
  strictNullChecks: string = '',
  jsx: string = '',
  jsxFactory: string = '',
  jsxFragmentFactory: string = '',
  jsxImportSource: string = '',
  moduleDetection: string = '',
  preserveConstEnums: boolean = false,
  verbatimModuleSyntax: boolean = false,
  rewriteRelativeImportExtensions: boolean = false,
  isolatedModules: boolean = false,
  importsNotUsedAsValues: string = '',
  preserveValueImports: boolean = false,
  removeComments: boolean = false,
  stripInternal: boolean = false,
  outFile: string = '',
  declarationMap: boolean = false,
): string {
  const tszBin = process.env.TSZ_BIN;
  let engineSalt = '';
  if (tszBin) {
    try {
      const st = fs.statSync(tszBin);
      engineSalt = `${tszBin}:${st.size}:${st.mtimeMs}`;
    } catch {
      engineSalt = tszBin;
    }
  }
  let runnerSalt = '';
  try {
    const runnerStat = fs.statSync(fileURLToPath(import.meta.url));
    runnerSalt = `${runnerStat.size}:${runnerStat.mtimeMs}`;
  } catch {
    runnerSalt = 'runner-unknown';
  }
  return hashString(`${sourceKey}:${target}:${module}:${alwaysStrict}:${declaration}:${sourceMap}:${inlineSourceMap}:${downlevelIteration}:${noEmitHelpers}:${noEmitOnError}:${importHelpers}:${esModuleInterop}:${useDefineForClassFields}:${experimentalDecorators}:${emitDecoratorMetadata}:${strictNullChecks}:${jsx}:${jsxFactory}:${jsxFragmentFactory}:${jsxImportSource}:${moduleDetection}:${preserveConstEnums}:${verbatimModuleSyntax}:${rewriteRelativeImportExtensions}:${isolatedModules}:${importsNotUsedAsValues}:${preserveValueImports}:${removeComments}:${stripInternal}:${outFile}:${declarationMap}:${engineSalt}:${runnerSalt}`);
}

let cache: Map<string, CacheEntry> = new Map();
let cacheLoaded = false;

function loadCache(): void {
  if (cacheLoaded) return;
  cacheLoaded = true;

  const cachePath = path.join(CACHE_DIR, 'emit-cache.json');
  if (fs.existsSync(cachePath)) {
    try {
      const data = JSON.parse(fs.readFileSync(cachePath, 'utf-8'));
      cache = new Map(Object.entries(data));
    } catch {
      cache = new Map();
    }
  }
}

function buildSourceKey(sourceFiles: Array<{ name: string; content: string }>): string {
  return sourceFiles.map(f => `${f.name}\n${f.content}`).join('\n////\n');
}

function saveCache(): void {
  if (!fs.existsSync(CACHE_DIR)) {
    fs.mkdirSync(CACHE_DIR, { recursive: true });
  }
  const cachePath = path.join(CACHE_DIR, 'emit-cache.json');
  const obj: Record<string, CacheEntry> = {};
  for (const [k, v] of cache) {
    obj[k] = v;
  }
  fs.writeFileSync(cachePath, JSON.stringify(obj));
}

// ============================================================================
// Test Discovery
// ============================================================================

function parseTarget(targetStr: string): number {
  const lower = targetStr.toLowerCase();
  if (lower.includes('es3')) return 0;
  if (lower.includes('es5')) return 1;
  if (lower.includes('es2015') || lower === 'es6') return 2;
  if (lower.includes('es2016')) return 3;
  if (lower.includes('es2017')) return 4;
  if (lower.includes('es2018')) return 5;
  if (lower.includes('es2019')) return 6;
  if (lower.includes('es2020')) return 7;
  if (lower.includes('es2021')) return 8;
  if (lower.includes('es2022')) return 9;
  if (lower.includes('es2023')) return 10;
  if (lower.includes('es2024')) return 11;
  if (lower.includes('es2025')) return 12;
  if (lower.includes('esnext')) return 99;
  return 12;  // TS6 default: ES2025
}

function parseModule(moduleStr: string): number {
  const lower = moduleStr.toLowerCase();
  if (lower === 'none') return 0;
  if (lower === 'commonjs') return 1;
  if (lower === 'amd') return 2;
  if (lower === 'umd') return 3;
  if (lower === 'system') return 4;
  if (lower === 'es2015' || lower === 'es6') return 5;
  if (lower === 'es2020') return 6;
  if (lower === 'es2022') return 7;
  if (lower === 'esnext') return 99;
  if (lower === 'node16') return 100;
  if (lower === 'node18') return 101;
  if (lower === 'node20') return 102;
  if (lower === 'nodenext') return 199;
  if (lower === 'preserve') return 200;
  return 0;
}

/**
 * Infer default module kind from target, matching TS6's computed module defaults:
 * - ESNext (99) → ESNext module (99)
 * - >= ES2022 (9) → ES2022 module (7)
 * - >= ES2020 (7) → ES2020 module (6)
 * - >= ES2015 (2) → ES2015 module (5)
 * - else → CommonJS (1)
 */
function inferDefaultModule(target: number): number {
  if (target === 99) return 99;  // ESNext → ESNext module
  if (target >= 9) return 7;     // >= ES2022 → ES2022 module
  if (target >= 7) return 6;     // >= ES2020 → ES2020 module
  if (target >= 2) return 5;     // >= ES2015 → ES2015 module
  return 1;                      // ES3/ES5 → CommonJS
}

function extractVariantFromFilename(
  filename: string,
): { base: string } & Record<string, string | undefined> {
  const match = filename.match(/^(.+?)\(([^)]+)\)\.js$/);
  if (!match) {
    return { base: filename.replace('.js', '') };
  }

  const base = match[1];
  const variants = match[2].split(',').map(v => v.trim());
  const result: { base: string } & Record<string, string | undefined> = { base };

  for (const variant of variants) {
    const [key, value] = variant.split('=');
    result[key] = value;
  }

  return result;
}

interface ParsedSourceTest {
  options: Record<string, unknown>;
  source: string | null;
  sourceFileName: string | null;
  sourceFiles: Array<{ name: string; content: string }>;
}

function parseSourceTest(content: string): ParsedSourceTest {
  const options: Record<string, unknown> = {};
  const sourceFiles: Array<{ name: string; content: string }> = [];
  const stripped = content.replace(/^\uFEFF/, '');
  const lines = stripped.split('\n');
  let currentFileName: string | null = null;
  let currentContent: string[] = [];

  const flushCurrentFile = () => {
    if (!currentFileName) return;
    sourceFiles.push({
      name: currentFileName,
      content: currentContent.join('\n'),
    });
    currentFileName = null;
    currentContent = [];
  };

  for (const line of lines) {
    const trimmed = line.trim();
    const optionMatch = trimmed.match(/^\/\/\s*@(\w+)\s*:\s*([^\r\n]*)$/i);
    if (optionMatch) {
      const [, key, rawValue] = optionMatch;
      const value = rawValue.trim();
      const lowKey = key.toLowerCase();
      if (lowKey === 'filename') {
        flushCurrentFile();
        currentFileName = value;
        continue;
      }
      if (value.toLowerCase() === 'true') options[lowKey] = true;
      else if (value.toLowerCase() === 'false') options[lowKey] = false;
      else options[lowKey] = value;
      continue;
    }

    const tsDirectiveMatch = trimmed.match(/^\/\/\s*@([\w-]+)\s*$/i);
    if (tsDirectiveMatch) {
      const lowKey = tsDirectiveMatch[1].toLowerCase();
      if (lowKey === 'ts-check') {
        options.checkjs = true;
        // @ts-check inside a @filename block is real source content (a comment
        // that tsc preserves in JS output), not a test-runner directive.
        if (currentFileName) {
          currentContent.push(line);
        }
      } else if (lowKey === 'ts-nocheck') {
        options.checkjs = false;
        if (currentFileName) {
          currentContent.push(line);
        }
      } else if (currentFileName) {
        currentContent.push(line);
      }
      continue;
    }

    if (currentFileName) {
      currentContent.push(line);
    }
  }

  flushCurrentFile();

  const entrySourceFile = sourceFiles.find(file => {
    return (
      file.content.length > 0 &&
      !file.name.endsWith('.d.ts') &&
      !file.name.endsWith('package.json') &&
      !file.name.endsWith('tsconfig.json')
    );
  });

  return {
    options,
    source: entrySourceFile?.content ?? null,
    sourceFileName: entrySourceFile?.name ?? null,
    sourceFiles,
  };
}

function loadDtsDiscoveryCache(): DtsDiscoveryCache {
  if (!fs.existsSync(DTS_DISCOVERY_CACHE)) return {};
  try {
    const parsed = JSON.parse(fs.readFileSync(DTS_DISCOVERY_CACHE, 'utf-8'));
    if (parsed && typeof parsed === 'object') return parsed as DtsDiscoveryCache;
  } catch {}
  return {};
}

function saveDtsDiscoveryCache(cacheData: DtsDiscoveryCache): void {
  if (!fs.existsSync(CACHE_DIR)) {
    fs.mkdirSync(CACHE_DIR, { recursive: true });
  }
  fs.writeFileSync(DTS_DISCOVERY_CACHE, JSON.stringify(cacheData));
}

async function filterToDtsBaselines(jsFiles: string[]): Promise<string[]> {
  const cached = loadDtsDiscoveryCache();
  const updated: DtsDiscoveryCache = { ...cached };
  const statLimit = pLimit(128);
  const readLimit = pLimit(64);

  const checks = await Promise.all(jsFiles.map(file => statLimit(async () => {
    const fullPath = path.join(BASELINES_DIR, file);
    const stat = await fs.promises.stat(fullPath);
    const entry = cached[file];
    if (entry && entry.mtimeMs === stat.mtimeMs && entry.size === stat.size) {
      return { file, hasDts: entry.hasDts };
    }

    const content = await readLimit(() => fs.promises.readFile(fullPath, 'utf-8'));
    const hasDts = /\[\s*[^\]]+\.d\.ts\s*]/i.test(content);
    updated[file] = { mtimeMs: stat.mtimeMs, size: stat.size, hasDts };
    return { file, hasDts };
  })));

  saveDtsDiscoveryCache(updated);
  return checks.filter(c => c.hasDts).map(c => c.file);
}

async function findTestCases(filter: string, maxTests: number, dtsOnly: boolean): Promise<TestCase[]> {
  if (!fs.existsSync(BASELINES_DIR)) {
    console.error(`Baselines directory not found: ${BASELINES_DIR}`);
    process.exit(1);
  }

  const entries = fs.readdirSync(BASELINES_DIR);
  let jsFiles = entries.filter(e => e.endsWith('.js')).sort();

  // Apply filter before reading any files
  if (filter) {
    const lowerFilter = filter.toLowerCase();
    jsFiles = jsFiles.filter(f => f.toLowerCase().includes(lowerFilter));
  }

  // For declaration-only mode, avoid parsing baselines that don't emit .d.ts outputs.
  if (dtsOnly) {
    jsFiles = await filterToDtsBaselines(jsFiles);
  }

  // Cap to maxTests before reading (we may discard some after parsing, so read a bit extra)
  if (maxTests < Infinity) {
    jsFiles = jsFiles.slice(0, Math.min(jsFiles.length, maxTests * 2));
  }

  // Read and parse baseline files in parallel
  const readLimit = pLimit(64);
  const parsedSourceCache = new Map<string, ParsedSourceTest>();
  const results = await Promise.all(jsFiles.map(baselineFile => readLimit(async () => {
    const baselinePath = path.join(BASELINES_DIR, baselineFile);
    const baselineContent = await fs.promises.readFile(baselinePath, 'utf-8');
    const baseline = parseBaseline(baselineContent);

    if (!baseline.source || !baseline.js) return null;
    if (dtsOnly && !baseline.dts) return null;

    const variant = extractVariantFromFilename(baselineFile);

    let directives: Record<string, unknown> = {};
    let sourceFiles = baseline.sourceFiles;
    let source = baseline.source;
    let sourceFileName = baseline.sourceFileName;
    if (baseline.testPath) {
      const cached = parsedSourceCache.get(baseline.testPath);
      if (cached) {
        directives = cached.options;
        if (cached.sourceFiles.length > 0) {
          sourceFiles = cached.sourceFiles;
          source = cached.source ?? source;
          sourceFileName = cached.sourceFileName ?? sourceFileName;
        }
      } else {
        const testFilePath = path.join(TS_DIR, baseline.testPath);
        try {
          const testFileContent = await fs.promises.readFile(testFilePath, 'utf-8');
          const parsedSource = parseSourceTest(testFileContent);
          directives = parsedSource.options;
          if (parsedSource.sourceFiles.length > 0) {
            sourceFiles = parsedSource.sourceFiles;
            source = parsedSource.source ?? source;
            sourceFileName = parsedSource.sourceFileName ?? sourceFileName;
          }
          parsedSourceCache.set(baseline.testPath, parsedSource);
        } catch {
          parsedSourceCache.set(baseline.testPath, {
            options: directives,
            source: null,
            sourceFileName: null,
            sourceFiles: [],
          });
        }
      }
    }

    const target = variant.target ? parseTarget(variant.target)
      : directives.target ? parseTarget(String(directives.target))
      : 12;  // TS6 default: ES2025 (LatestStandard)
    // Also check tsconfig.json files embedded in sourceFiles for compiler options
    const tsconfigOptions: Record<string, unknown> = {};
    for (const sf of sourceFiles) {
      if (sf.name.endsWith('tsconfig.json')) {
        try {
          const parsed = JSON.parse(sf.content);
          if (parsed?.compilerOptions) {
            Object.assign(tsconfigOptions, parsed.compilerOptions);
          }
        } catch { /* ignore parse errors */ }
      }
    }
    const tsconfigModule = tsconfigOptions.module
      ? parseModule(String(tsconfigOptions.module))
      : undefined;
    const module = variant.module ? parseModule(variant.module)
      : directives.module ? parseModule(String(directives.module))
      : tsconfigModule !== undefined ? tsconfigModule
      : inferDefaultModule(target);  // Match TSC's default: commonjs for es3/es5, es2015 for es2015+

    // TS6: alwaysStrict defaults to true unless explicitly set to false.
    // Note: @strict: false does NOT affect alwaysStrict in TS6 — they are independent.
    const alwaysStrict = variant.alwaysstrict !== undefined
      ? variant.alwaysstrict === 'true'
      : directives.alwaysstrict !== false;
    const sourceMap = directives.sourcemap === true || directives.inlinesourcemap === true;
    const inlineSourceMap = directives.inlinesourcemap === true;
    const downlevelIteration = variant.downleveliteration !== undefined
      ? variant.downleveliteration === 'true'
      : directives.downleveliteration === true;
    const noEmitHelpers = directives.noemithelpers === true;
    const noEmitOnError = directives.noemitonerror === true;
    const importHelpers = variant.importhelpers !== undefined
      ? variant.importhelpers === 'true'
      : directives.importhelpers === true;
    const esModuleInterop = variant.esmoduleinterop !== undefined
      ? variant.esmoduleinterop === 'true'
      : directives.esmoduleinterop !== false;
    const useDefineForClassFields = variant.usedefineforclassfields !== undefined
      ? variant.usedefineforclassfields === 'true'
      : typeof directives.usedefineforclassfields === 'boolean'
        ? directives.usedefineforclassfields
        : undefined;
    const experimentalDecorators = variant.experimentaldecorators !== undefined
      ? variant.experimentaldecorators === 'true'
      : directives.experimentaldecorators === true;
    const emitDecoratorMetadata = directives.emitdecoratormetadata === true;
    const strictNullChecks = variant.strictnullchecks !== undefined
      ? variant.strictnullchecks === 'true'
      : typeof directives.strictnullchecks === 'boolean'
        ? directives.strictnullchecks
        // When @strict: false, derive strictNullChecks as false (matches tsc)
        : directives.strict === false
          ? false
          : undefined;
    const jsx = variant.jsx ?? (typeof directives.jsx === 'string' ? directives.jsx : undefined);
    const moduleDetection =
      variant.moduledetection ?? (typeof directives.moduledetection === 'string' ? directives.moduledetection : undefined);
    // @reactNamespace: X maps to jsxFactory: X.createElement, jsxFragmentFactory: X.Fragment
    const reactNamespace = typeof directives.reactnamespace === 'string' ? directives.reactnamespace : undefined;
    const jsxFactory = typeof directives.jsxfactory === 'string' ? directives.jsxfactory
      : reactNamespace ? `${reactNamespace}.createElement` : undefined;
    const jsxFragmentFactory =
      typeof directives.jsxfragmentfactory === 'string' ? directives.jsxfragmentfactory
      : reactNamespace ? `${reactNamespace}.Fragment` : undefined;
    const jsxImportSource = typeof directives.jsximportsource === 'string' ? directives.jsximportsource : undefined;
    const preserveConstEnums = variant.preserveconstenums !== undefined
      ? variant.preserveconstenums === 'true'
      : directives.preserveconstenums === true;
    const verbatimModuleSyntax = variant.verbatimmodulesyntax !== undefined
      ? variant.verbatimmodulesyntax === 'true'
      : directives.verbatimmodulesyntax === true;
    const rewriteRelativeImportExtensions = variant.rewriterelativeimportextensions !== undefined
      ? variant.rewriterelativeimportextensions === 'true'
      : directives.rewriterelativeimportextensions === true;
    const isolatedModules = variant.isolatedmodules !== undefined
      ? variant.isolatedmodules === 'true'
      : directives.isolatedmodules === true;
    const importsNotUsedAsValues = typeof directives.importsnotusedasvalues === 'string'
      ? directives.importsnotusedasvalues : undefined;
    const preserveValueImports = directives.preservevalueimports === true;
    const removeComments = directives.removecomments === true;
    const stripInternal = directives.stripinternal === true;
    const emitDeclarationOnly = directives.emitdeclarationonly === true;
    const declarationMap = directives.declarationmap === true;

    // Fix up outFile baseline parsing: when @outFile is specified, the baseline
    // may contain both JS input files and the bundled output file. The parser
    // only handles `out.js` by default, so we fix up the expected output for
    // custom outFile names (e.g., dummy.js, output.js).
    const outFile = typeof directives.outfile === 'string' ? directives.outfile : undefined;
    if (outFile && outFile !== 'out.js' && baseline.files.has(outFile)) {
      baseline.js = baseline.files.get(outFile) ?? baseline.js;
      baseline.jsFileName = outFile;
    }
    const outDtsFile = outFile?.replace(/\.js$/, '.d.ts');
    if (outDtsFile && outDtsFile !== 'out.d.ts' && baseline.files.has(outDtsFile)) {
      baseline.dts = baseline.files.get(outDtsFile) ?? baseline.dts;
      baseline.dtsFileName = outDtsFile;
    }

    return {
      baselineFile,
      testPath: baseline.testPath,
      sourceFileName,
      sourceFiles,
      source: source ?? baseline.source!,
      expectedJs: baseline.js,
      expectedJsFileName: baseline.jsFileName,
      expectedDts: baseline.dts,
      expectedDtsFileName: baseline.dtsFileName,
      target,
      module,
      alwaysStrict,
      sourceMap,
      inlineSourceMap,
      downlevelIteration,
      noEmitHelpers,
      noEmitOnError,
      importHelpers,
      esModuleInterop,
      useDefineForClassFields,
      experimentalDecorators,
      emitDecoratorMetadata,
      strictNullChecks,
      jsx,
      jsxFactory,
      jsxFragmentFactory,
      jsxImportSource,
      moduleDetection,
      preserveConstEnums,
      verbatimModuleSyntax,
      rewriteRelativeImportExtensions,
      isolatedModules,
      importsNotUsedAsValues,
      preserveValueImports,
      removeComments,
      stripInternal,
      outFile,
      emitDeclarationOnly,
      declarationMap,
    } as TestCase;
  })));

  // Filter nulls and cap to maxTests
  return results.filter((r): r is TestCase => r !== null).slice(0, maxTests);
}

// ============================================================================
// Comment Normalization
// ============================================================================

/**
 * Normalize comments for comparison: strip inline/trailing comments, remove
 * comment-only and blank lines, collapse whitespace gaps left by removal.
 * This allows matching emit output that differs only in comment placement.
 */
function normalizeComments(s: string): string {
  return s.split('\n')
    .map(l => {
      // Strip trailing single-line comments (not triple-slash, sourcemap, or in strings)
      let code = l.replace(/\s*\/\/(?![\/#])(?![^"]*"[^"]*$).*$/, '');
      // Strip inline block comments (/* ... */) that don't span lines
      code = code.replace(/\s*\/\*[^*]*\*+(?:[^/*][^*]*\*+)*\//g, (match, offset, str) => {
        const before = str.substring(0, offset);
        const after = str.substring(offset + match.length);
        if (before.trim() === '' && after.trim() === '') return match; // comment-only line
        return '';
      });
      // Collapse runs of multiple spaces to single space
      code = code.replace(/  +/g, ' ');
      return code;
    })
    // Remove lines that are ONLY comments or whitespace-only
    .filter(l => {
      const t = l.trim();
      if (t === '') return false;
      if (t.startsWith('//')) return false;
      if (t.startsWith('/*') && t.endsWith('*/')) return false;
      if (t.startsWith('*')) return false;
      return true;
    })
    .join('\n')
    .trim();
}

/**
 * Normalize whitespace for comparison: collapse all whitespace sequences to
 * single space, normalize line breaks. This catches tab-vs-space indentation
 * differences and minor formatting differences.
 */
function normalizeWhitespace(s: string): string {
  // Collapse all whitespace (including newlines) to single space, then compare.
  // This catches tab-vs-space, line-break, and indentation differences.
  return s.replace(/\s+/g, ' ').trim();
}

// ============================================================================
// Test Execution
// ============================================================================

async function runTest(transpiler: CliTranspiler, testCase: TestCase, config: Config): Promise<TestResult> {
  const start = Date.now();
  const testName = testCase.baselineFile.replace('.js', '');

  const result: TestResult = {
    name: testName,
    testPath: testCase.testPath,
    jsMatch: null,
    dtsMatch: null,
  };

  try {
    loadCache();
    const emitDeclarations = !config.jsOnly && testCase.expectedDts !== null;
    const sourceKey = buildSourceKey(testCase.sourceFiles);
    const cacheKey = getCacheKey(
      sourceKey,
      testCase.target,
      testCase.module,
      testCase.alwaysStrict,
      emitDeclarations,
      testCase.sourceMap,
      testCase.inlineSourceMap,
      testCase.downlevelIteration,
      testCase.noEmitHelpers,
      testCase.noEmitOnError,
      testCase.importHelpers,
      testCase.esModuleInterop,
      testCase.useDefineForClassFields === undefined ? '' : String(testCase.useDefineForClassFields),
      testCase.experimentalDecorators,
      testCase.emitDecoratorMetadata,
      testCase.strictNullChecks === undefined ? '' : String(testCase.strictNullChecks),
      testCase.jsx ?? '',
      testCase.jsxFactory ?? '',
      testCase.jsxFragmentFactory ?? '',
      testCase.jsxImportSource ?? '',
      testCase.moduleDetection ?? '',
      testCase.preserveConstEnums,
      testCase.verbatimModuleSyntax,
      testCase.rewriteRelativeImportExtensions,
      testCase.isolatedModules,
      testCase.importsNotUsedAsValues ?? '',
      testCase.preserveValueImports,
      testCase.removeComments,
      testCase.stripInternal,
      testCase.outFile ?? '',
      testCase.declarationMap,
    );
    let tszJs: string;
    let tszDts: string | null = null;

    const cached = cache.get(cacheKey);
    const sourceHash = hashString(sourceKey);

    if (cached && cached.hash === sourceHash) {
      tszJs = cached.jsOutput;
      tszDts = cached.dtsOutput;
    } else {
      const transpileResult = await transpiler.transpile(testCase.source, testCase.target, testCase.module, {
        sourceFileName: testCase.sourceFileName ?? undefined,
        declaration: emitDeclarations,
        alwaysStrict: testCase.alwaysStrict,
        sourceMap: testCase.sourceMap,
        inlineSourceMap: testCase.inlineSourceMap,
        downlevelIteration: testCase.downlevelIteration,
        noEmitHelpers: testCase.noEmitHelpers,
        noEmitOnError: testCase.noEmitOnError,
        importHelpers: testCase.importHelpers,
        esModuleInterop: testCase.esModuleInterop,
        useDefineForClassFields: testCase.useDefineForClassFields,
        experimentalDecorators: testCase.experimentalDecorators,
        emitDecoratorMetadata: testCase.emitDecoratorMetadata,
        strictNullChecks: testCase.strictNullChecks,
        jsx: testCase.jsx,
        jsxFactory: testCase.jsxFactory,
        jsxFragmentFactory: testCase.jsxFragmentFactory,
        jsxImportSource: testCase.jsxImportSource,
        moduleDetection: testCase.moduleDetection,
        preserveConstEnums: testCase.preserveConstEnums,
        verbatimModuleSyntax: testCase.verbatimModuleSyntax,
        rewriteRelativeImportExtensions: testCase.rewriteRelativeImportExtensions,
        isolatedModules: testCase.isolatedModules,
        importsNotUsedAsValues: testCase.importsNotUsedAsValues,
        preserveValueImports: testCase.preserveValueImports,
        removeComments: testCase.removeComments,
        stripInternal: testCase.stripInternal,
        outFile: testCase.outFile,
        declarationMap: testCase.declarationMap,
        sourceFiles: testCase.sourceFiles,
        expectedJsFileName: testCase.expectedJsFileName ?? undefined,
        expectedDtsFileName: testCase.expectedDtsFileName ?? undefined,
        expectedJsContent: testCase.expectedJs,
        expectedDtsContent: testCase.expectedDts,
      });
      tszJs = transpileResult.js;
      tszDts = transpileResult.dts || null;
      cache.set(cacheKey, { hash: sourceHash, jsOutput: tszJs, dtsOutput: tszDts });
    }

    if (!config.dtsOnly && testCase.expectedJs !== null && !testCase.emitDeclarationOnly) {
      // Strip sourceMappingURL lines entirely: our CLI may append its own
      // sourceMappingURL while tsc baselines use inline data URLs or different
      // filenames, causing line-count mismatches. Since we test code emission
      // correctness (not source-map generation), stripping is safe.
      const stripSourceMapUrl = (s: string) =>
        s.split('\n').filter(l => !l.trimStart().startsWith('//# sourceMappingURL=')).join('\n');
      // Normalize duplicate "use strict" in preamble: tsc emits double "use strict"
      // when the source already has one and alwaysStrict is enabled. Our emitter may
      // emit only one. Normalize both sides to single "use strict" for comparison.
      const dedupeUseStrict = (s: string): string => {
        const lines = s.split('\n');
        const out: string[] = [];
        let seen = false;
        let preambleDone = false;
        for (const line of lines) {
          const t = line.trim();
          const isUS = t === '"use strict";' || t === "'use strict';";
          if (!preambleDone && isUS) {
            if (!seen) { out.push(line); seen = true; }
            continue;
          }
          if (t !== '') preambleDone = true;
          out.push(line);
        }
        return out.join('\n');
      };
      const expected = dedupeUseStrict(stripSourceMapUrl(testCase.expectedJs.replace(/\r\n/g, '\n').trim()));
      const actual = dedupeUseStrict(stripSourceMapUrl(tszJs.replace(/\r\n/g, '\n').trim()));
      result.jsMatch = expected === actual;

      if (!result.jsMatch) {
        // Fallback 1: normalize comments and compare again.
        if (normalizeComments(expected) === normalizeComments(actual)) {
          result.jsMatch = true; // comment-only difference
        }
        // Fallback 2: normalize comments + whitespace (tabs vs spaces, line breaks)
        else if (normalizeWhitespace(normalizeComments(expected)) === normalizeWhitespace(normalizeComments(actual))) {
          result.jsMatch = true; // whitespace-only difference
        }
        else {
          result.jsError = config.verbose ? getEmitDiff(expected, actual) : getEmitDiffSummary(expected, actual);
        }
      }
    }

    if (!config.jsOnly && testCase.expectedDts !== null) {
      if (tszDts !== null) {
        const expected = testCase.expectedDts.replace(/\r\n/g, '\n').trim();
        const actual = tszDts.replace(/\r\n/g, '\n').trim();
        result.dtsMatch = expected === actual;

        if (!result.dtsMatch) {
          if (normalizeComments(expected) === normalizeComments(actual)) {
            result.dtsMatch = true;
          } else if (normalizeWhitespace(normalizeComments(expected)) === normalizeWhitespace(normalizeComments(actual))) {
            result.dtsMatch = true;
          } else {
            result.dtsError = config.verbose ? getEmitDiff(expected, actual) : getEmitDiffSummary(expected, actual);
          }
        }
      } else {
        result.dtsMatch = null;
      }
    }

    result.elapsed = Date.now() - start;
  } catch (e) {
    const errorMsg = e instanceof Error ? e.message : String(e);
    const summarized = summarizeErrorMessage(errorMsg);
    result.timeout = errorMsg === 'TIMEOUT';
    if (!config.dtsOnly) {
      result.jsMatch = false;
      result.jsError = result.timeout ? 'TIMEOUT' : summarized;
    }
    if (!config.jsOnly) {
      result.dtsMatch = false;
      result.dtsError = result.timeout ? 'TIMEOUT' : summarized;
    }
    result.elapsed = Date.now() - start;
  }

  return result;
}

// ============================================================================
// Display Helpers
// ============================================================================

function resultStatusIcon(result: TestResult, dtsOnly: boolean): string {
  if (result.timeout) return pc.yellow('T');
  if (result.skipped) return pc.dim('S');
  if (dtsOnly) {
    if (result.dtsMatch === true) return pc.green('✓');
    if (result.dtsMatch === false) return pc.red('✗');
    return pc.dim('-');
  }
  if (result.jsMatch === false || result.dtsMatch === false) return pc.red('✗');
  if (result.jsMatch === true || result.dtsMatch === true) return pc.green('✓');
  return pc.dim('-');
}

function printVerboseResult(result: TestResult, config: Config) {
  console.log(`  [${resultStatusIcon(result, config.dtsOnly)}] ${result.name} (${result.elapsed}ms)`);
  if (config.dtsOnly && result.dtsError && result.dtsMatch === false) {
    console.log(result.dtsError);
  } else if (result.jsError && result.jsMatch === false) {
    console.log(result.jsError);
  }
}

function progressBar(current: number, total: number, width: number = 30): string {
  const pct = total > 0 ? current / total : 0;
  const filled = Math.round(pct * width);
  const empty = width - filled;
  const bar = pc.green('█'.repeat(filled)) + pc.dim('░'.repeat(empty));
  return `${bar} ${(pct * 100).toFixed(1)}% | ${current.toLocaleString()}/${total.toLocaleString()}`;
}

// ============================================================================
// CLI
// ============================================================================

function parseArgs(): Config {
  const args = process.argv.slice(2);
  const config: Config = {
    maxTests: Infinity,
    offset: 0,
    filter: '',
    verbose: false,
    jsOnly: false,
    dtsOnly: false,
    concurrency: Math.max(1, os.cpus().length),
    timeoutMs: DEFAULT_TIMEOUT_MS,
    jsonOut: null,
  };

  for (const arg of args) {
    if (arg.startsWith('--max=')) {
      const rawMax = arg.slice(6);
      if (rawMax === 'all' || rawMax === 'All' || rawMax === 'ALL' || rawMax === '') {
        config.maxTests = Infinity;
      } else {
        const parsed = parseInt(rawMax, 10);
        if (Number.isNaN(parsed) || parsed <= 0) {
          throw new Error(`Invalid --max value: ${rawMax}. Use a positive integer or all.`);
        }
        config.maxTests = parsed;
      }
    } else if (arg.startsWith('--offset=')) {
      config.offset = Math.max(0, parseInt(arg.slice(9), 10));
    } else if (arg.startsWith('--filter=')) {
      config.filter = arg.slice(9);
    } else if (arg.startsWith('--concurrency=') || arg.startsWith('-j')) {
      const val = arg.startsWith('-j') ? arg.slice(2) : arg.slice(14);
      config.concurrency = Math.max(1, parseInt(val, 10));
    } else if (arg.startsWith('--timeout=')) {
      config.timeoutMs = Math.max(500, parseInt(arg.slice(10), 10));
    } else if (arg === '--verbose' || arg === '-v') {
      config.verbose = true;
    } else if (arg === '--js-only') {
      config.jsOnly = true;
    } else if (arg === '--dts-only') {
      config.dtsOnly = true;
    } else if (arg.startsWith('--json-out=')) {
      config.jsonOut = arg.slice(11);
    } else if (arg === '--json-out') {
      config.jsonOut = path.join(__dirname, '../emit-detail.json');
    } else if (arg === '--help' || arg === '-h') {
      console.log(`
TSZ Emit Test Runner

Usage: ./scripts/emit/run.sh [options]

Options:
  --max=N               Maximum tests (default: all)
  --filter=PATTERN      Filter tests by name
  --concurrency=N, -jN  Parallel workers (default: CPU count)
  --timeout=MS          Per-test timeout in ms (default: ${DEFAULT_TIMEOUT_MS})
  --verbose, -v         Detailed output with diffs
  --js-only             Test JavaScript emit only
  --dts-only            Test declaration emit only
  --json-out[=PATH]     Write machine-readable results JSON (default: emit-detail.json)
  --help, -h            Show this help
`);
      process.exit(0);
    }
  }

  return config;
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  const config = parseArgs();
  const sep = pc.cyan('════════════════════════════════════════════════════════════');

  console.log('');
  console.log(sep);
  console.log(pc.bold('  TSZ Emit Test Runner'));
  console.log(sep);
  console.log(pc.dim(`  Max tests: ${config.maxTests === Infinity ? 'all' : config.maxTests}`));
  console.log(pc.dim(`  Timeout: ${config.timeoutMs}ms per test`));
  if (config.filter) console.log(pc.dim(`  Filter: ${config.filter}`));
  console.log(pc.dim(`  Mode: ${config.jsOnly ? 'JS only' : config.dtsOnly ? 'DTS only' : 'JS + DTS'}`));
  console.log(pc.dim(`  Workers: ${config.concurrency} parallel`));
  console.log(pc.dim(`  Engine: Native CLI (${config.jsOnly ? 'emit-only, --noCheck --noLib' : 'with type checking'})`));
  console.log(sep);
  console.log('');

  const transpiler = new CliTranspiler(config.timeoutMs);

  // Ensure child processes are killed on unexpected exit
  const cleanup = () => { transpiler.terminate(); };
  process.on('SIGINT', () => { cleanup(); process.exit(130); });
  process.on('SIGTERM', () => { cleanup(); process.exit(143); });

  console.log(pc.dim('Discovering test cases...'));
  // Discover more tests than needed when offset is used, then slice
  const discoveredLimit = config.offset > 0 ? config.maxTests + config.offset : config.maxTests;
  let testCases = await findTestCases(config.filter, discoveredLimit, config.dtsOnly);
  if (config.offset > 0) {
    testCases = testCases.slice(config.offset, config.offset + config.maxTests);
  }
  console.log(pc.dim(`Found ${testCases.length} test cases`));
  console.log('');

  // Counters
  let jsPass = 0, jsFail = 0, jsSkip = 0, jsTimeout = 0;
  let dtsPass = 0, dtsFail = 0, dtsSkip = 0;
  const failures: TestResult[] = [];
  const allTestResults: TestResult[] = [];
  const startTime = Date.now();
  let completed = 0;

  function recordResult(result: TestResult) {
    completed++;
    allTestResults.push(result);
    if (result.skipped) {
      jsSkip++;
    } else if (result.timeout) {
      jsTimeout++;
      jsFail++;
      failures.push(result);
    } else if (result.jsMatch === true) {
      jsPass++;
    } else if (result.jsMatch === false) {
      jsFail++;
      failures.push(result);
    } else {
      jsSkip++;
    }

    if (result.dtsMatch === true) dtsPass++;
    else if (result.dtsMatch === false) {
      dtsFail++;
      if (result.jsMatch !== false && !result.timeout) {
        failures.push(result);
      }
    }
    else dtsSkip++;
  }

  // Progress bar (non-verbose)
  let lastProgressLen = 0;
  function printProgress() {
    const bar = progressBar(completed, testCases.length);
    const elapsed = (Date.now() - startTime) / 1000;
    const rate = completed > 0 ? Math.round(completed / elapsed) : 0;
    const msg = `  ${bar} | ${rate}/s`;
    process.stdout.write('\r' + msg + ' '.repeat(Math.max(0, lastProgressLen - msg.length)));
    lastProgressLen = msg.length;
  }

  // Run tests in parallel using p-limit
  const limit = pLimit(config.concurrency);

  if (config.verbose) {
    // Verbose: collect results and flush in order as they complete
    const results = new Array<TestResult | null>(testCases.length).fill(null);
    let printedUpTo = 0;

    await Promise.all(testCases.map((tc, i) => limit(async () => {
      const result = await runTest(transpiler, tc, config);
      results[i] = result;
      recordResult(result);
      // Flush contiguously completed results in order
      while (printedUpTo < testCases.length && results[printedUpTo] !== null) {
        printVerboseResult(results[printedUpTo]!, config);
        printedUpTo++;
      }
    })));
  } else {
    // Non-verbose: parallel with progress bar
    await Promise.all(testCases.map(tc => limit(async () => {
      const result = await runTest(transpiler, tc, config);
      recordResult(result);
      printProgress();
    })));
  }

  // Cleanup
  transpiler.terminate();
  saveCache();

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

  // Summary
  console.log('\n');
  console.log(sep);
  console.log(pc.bold('EMIT TEST RESULTS'));
  console.log(sep);

  if (!config.dtsOnly) {
    const jsTotal = jsPass + jsFail;
    const jsPct = jsTotal > 0 ? (jsPass / jsTotal * 100).toFixed(1) : '0.0';
    console.log(pc.bold('JavaScript Emit:'));
    console.log(`  ${pc.green(`Passed: ${jsPass}`)}`);
    console.log(`  ${pc.red(`Failed: ${jsFail}`)}${jsTimeout > 0 ? ` (${jsTimeout} timeouts)` : ''}`);
    console.log(`  ${pc.dim(`Skipped: ${jsSkip}`)}`);
    console.log(`  ${pc.yellow(`Pass Rate: ${jsPct}% (${jsPass}/${jsTotal})`)}`);
  }

  if (!config.jsOnly && (dtsPass + dtsFail) > 0) {
    const dtsTotal = dtsPass + dtsFail;
    const dtsPct = dtsTotal > 0 ? (dtsPass / dtsTotal * 100).toFixed(1) : '0.0';
    console.log(pc.bold('Declaration Emit:'));
    console.log(`  ${pc.green(`Passed: ${dtsPass}`)}`);
    console.log(`  ${pc.red(`Failed: ${dtsFail}`)}`);
    console.log(`  ${pc.dim(`Skipped: ${dtsSkip}`)}`);
    console.log(`  ${pc.yellow(`Pass Rate: ${dtsPct}% (${dtsPass}/${dtsTotal})`)}`);
  }

  const totalTests = testCases.length;
  const rate = totalTests > 0 ? Math.round(totalTests / parseFloat(elapsed)) : 0;
  console.log(pc.dim(`\nTime: ${elapsed}s (${rate} tests/sec)`));
  console.log(sep);

  // Show first failures (excluding timeouts)
  const realFailures = failures.filter(f => !f.timeout);
  if (realFailures.length > 0 && !config.verbose) {
    console.log(`\n${pc.bold('First failures:')}`);
    for (const f of realFailures) {
      const diffInfo = f.jsError ? ` ${pc.dim(`(${f.jsError})`)}` : '';
      console.log(`  ${pc.red('✗')} ${f.name}${diffInfo}`);
    }
  }

  // Show timeouts
  const timeouts = failures.filter(f => f.timeout);
  if (timeouts.length > 0 && !config.verbose) {
    console.log(`\n${pc.bold(`Timeouts (${timeouts.length}):`)}`);
    for (const f of timeouts.slice(0, 5)) {
      console.log(`  ${pc.yellow('T')} ${f.name}`);
    }
    if (timeouts.length > 5) {
      console.log(`  ${pc.dim(`... and ${timeouts.length - 5} more`)}`);
    }
  }

  // Write machine-readable JSON if requested
  if (config.jsonOut) {
    const allResults: Array<{
      name: string;
      baselineFile: string;
      testPath: string | null;
      jsStatus: 'pass' | 'fail' | 'skip' | 'timeout';
      dtsStatus: 'pass' | 'fail' | 'skip' | 'timeout';
      jsError?: string;
      dtsError?: string;
      elapsed?: number;
    }> = [];

    for (const r of allTestResults) {
      let jsStatus: 'pass' | 'fail' | 'skip' | 'timeout' = 'skip';
      if (r.timeout) jsStatus = 'timeout';
      else if (r.jsMatch === true) jsStatus = 'pass';
      else if (r.jsMatch === false) jsStatus = 'fail';

      let dtsStatus: 'pass' | 'fail' | 'skip' | 'timeout' = 'skip';
      if (r.timeout) dtsStatus = 'timeout';
      else if (r.dtsMatch === true) dtsStatus = 'pass';
      else if (r.dtsMatch === false) dtsStatus = 'fail';

      const record: any = {
        name: r.name,
        baselineFile: r.name + '.js',
        testPath: r.testPath,
        jsStatus,
        dtsStatus,
      };
      if (r.jsError) record.jsError = r.jsError;
      if (r.dtsError) record.dtsError = r.dtsError;
      if (r.elapsed !== undefined) record.elapsed = r.elapsed;
      allResults.push(record);
    }

    const jsTotal = jsPass + jsFail;
    const dtsTotal = dtsPass + dtsFail;
    const detail = {
      timestamp: new Date().toISOString(),
      summary: {
        jsTotal,
        jsPass,
        jsFail,
        jsSkip,
        jsTimeout,
        jsPassRate: jsTotal > 0 ? Math.round(jsPass / jsTotal * 1000) / 10 : 0,
        dtsTotal,
        dtsPass,
        dtsFail,
        dtsSkip,
        dtsPassRate: dtsTotal > 0 ? Math.round(dtsPass / dtsTotal * 1000) / 10 : 0,
      },
      results: allResults,
    };

    const outPath = path.resolve(config.jsonOut);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(detail, null, 2));
    console.log(pc.dim(`\nJSON results written to ${outPath}`));
  }

  process.exit(jsFail > 0 || dtsFail > 0 ? 1 : 0);
}

main().catch(err => {
  console.error('Fatal error:', err);
  // main() installs its own cleanup; if we got here the transpiler
  // may still have in-flight children — but it's scoped inside main().
  // The SIGINT/SIGTERM handlers above cover signal-based exits.
  // For uncaught promise rejections the process is about to die anyway
  // and the OS will reap the children since they share the process group.
  process.exit(2);
});
