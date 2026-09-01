#!/usr/bin/env node
/**
 * TSZ Emit Test Runner
 *
 * Compares TSZ products with a fresh pinned TypeScript 7 process observation.
 * Runs tests in parallel with configurable concurrency and timeout.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'url';
import pc from 'picocolors';
import pLimit from 'p-limit';
import { parseBaseline, getEmitDiff, getEmitDiffSummary } from './baseline-parser.js';
import { CliTranspiler, type LinkInput } from './cli-transpiler.js';
import { parseTarget, parseModule, inferDefaultModule } from './ts-enums.js';
import { BaselineBlobReader } from './baseline-blob-reader.js';
import { buildBlob, isBlobFresh } from './build-baseline-blob.js';
import {
  authoredOptionFailureReasons,
  extractAuthoredVariantFromFilename,
  optionBoolean,
  optionLibList,
  optionString,
  parseEmbeddedCompilerOptions,
  resolveAuthoredOptions,
} from './authored-options.js';
import {
  compareCanonicalProductSets,
  type ProductSetComparison,
} from './canonical-products.js';
import { resolvePinnedOracle } from './oracle.js';
import {
  canonicalUnsupportedReasons,
  hasEmitSidecar,
  retainCanonicalInventory,
} from './canonical-support.js';
import {
  artifactCandidateTotal,
  artifactHasNonPass,
  artifactSurfaceObservation,
  artifactStatus,
  compareArtifactOutcomes as compareCompilerOutcomes,
  compilerArtifactState,
  emptyArtifactProductCounts,
  emptyArtifactStatusCounts,
  ensureMeasuredArtifact,
  recordArtifactProduct,
  recordArtifactStatus,
  selectArtifactSurfaces,
  type ArtifactProductCounts,
  type ArtifactState,
  type ArtifactStatus,
  type ArtifactStatusCounts,
} from './artifact-state.js';
import {
  parseSourceTest,
  readTypeScriptTestFile,
  selectHarnessRootFiles,
  type ParsedSourceTest,
} from './source-test.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');
const TS_DIR = path.join(ROOT_DIR, 'TypeScript');
const BASELINES_DIR = path.join(TS_DIR, 'tests/baselines/reference');
const CACHE_DIR = path.join(__dirname, '../.cache');
const DTS_DISCOVERY_CACHE = path.join(CACHE_DIR, 'dts-baseline-index.json');
const DTS_DISCOVERY_CACHE_VERSION = 4;

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
  rootFileNames: string[];
  links: LinkInput[];
  source: string;
  /** Baseline names define only the invocation's product domain, never bytes. */
  jsProductDomain: string[];
  dtsProductDomain: string[];
  target?: number;
  module?: number;
  lib?: string[];
  alwaysStrict?: boolean;
  sourceMap?: boolean;
  inlineSourceMap?: boolean;
  downlevelIteration?: boolean;
  noEmitHelpers?: boolean;
  noEmitOnError?: boolean;
  noCheck?: boolean;
  noLib?: boolean;
  noEmit?: boolean;
  declaration?: boolean;
  importHelpers?: boolean;
  esModuleInterop?: boolean;
  useDefineForClassFields?: boolean;
  experimentalDecorators?: boolean;
  emitDecoratorMetadata?: boolean;
  strict?: boolean;
  strictNullChecks?: boolean;
  exactOptionalPropertyTypes?: boolean;
  jsx?: string;
  jsxFactory?: string;
  jsxFragmentFactory?: string;
  jsxImportSource?: string;
  moduleResolution?: string;
  moduleDetection?: string;
  preserveConstEnums?: boolean;
  verbatimModuleSyntax?: boolean;
  rewriteRelativeImportExtensions?: boolean;
  isolatedModules?: boolean;
  importsNotUsedAsValues?: string;
  preserveValueImports?: boolean;
  removeComments?: boolean;
  stripInternal?: boolean;
  allowJs?: boolean;
  allowUnreachableCode?: boolean;
  checkJs?: boolean;
  noImplicitAny?: boolean;
  noUnusedLocals?: boolean;
  noUnusedParameters?: boolean;
  skipLibCheck?: boolean;
  strictPropertyInitialization?: boolean;
  baseUrl?: string;
  outFile?: string;
  outDir?: string;
  declarationDir?: string;
  rootDir?: string;
  emitDeclarationOnly?: boolean;
  declarationMap?: boolean;
  unsupportedReason?: string;
}

interface TestResult {
  name: string;
  testPath: string | null;
  jsSelected: boolean;
  dtsSelected: boolean;
  outcomeMatch: boolean | null;
  jsMatch: boolean | null;
  dtsMatch: boolean | null;
  jsProductMatch: boolean | null;
  dtsProductMatch: boolean | null;
  artifactState: ArtifactState;
  outcomeError?: string;
  jsError?: string;
  dtsError?: string;
  jsProductError?: string;
  dtsProductError?: string;
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

interface DtsDiscoveryEntry {
  version: number;
  mtimeMs: number;
  size: number;
  hasDts: boolean;
}

type DtsDiscoveryCache = Record<string, DtsDiscoveryEntry>;

// ============================================================================
// Result Fingerprinting
// ============================================================================

// SHA-256 hex digest. Collision-free in practice; use for cache identity where
// the prior 32-bit polynomial hash had a non-trivial pairwise collision rate
// over the full emit test set.
function hashString(str: string): string {
  return createHash('sha256').update(str).digest('hex');
}

function detailRowsFingerprint(results: Array<Record<string, unknown>>): string {
  const rows = results.map(result => ({
    // Keep keys in lexical order so this byte stream agrees with Python's
    // json.dumps(..., sort_keys=True) in query-emit.py.
    artifactState: result.artifactState ?? null,
    baselineFile: result.baselineFile ?? null,
    dtsError: result.dtsError ?? null,
    dtsMatch: result.dtsMatch ?? null,
    dtsProductError: result.dtsProductError ?? null,
    dtsProductMatch: result.dtsProductMatch ?? null,
    dtsSelected: result.dtsSelected ?? null,
    dtsStatus: result.dtsStatus ?? null,
    jsError: result.jsError ?? null,
    jsMatch: result.jsMatch ?? null,
    jsProductError: result.jsProductError ?? null,
    jsProductMatch: result.jsProductMatch ?? null,
    jsSelected: result.jsSelected ?? null,
    jsStatus: result.jsStatus ?? null,
    name: result.name ?? null,
    outcomeError: result.outcomeError ?? null,
    outcomeMatch: result.outcomeMatch ?? null,
    testPath: result.testPath ?? null,
  })).sort((left, right) => {
    const leftKey = `${left.name ?? ''}\0${left.baselineFile ?? ''}\0${left.testPath ?? ''}`;
    const rightKey = `${right.name ?? ''}\0${right.baselineFile ?? ''}\0${right.testPath ?? ''}`;
    if (leftKey < rightKey) return -1;
    if (leftKey > rightKey) return 1;
    return 0;
  });
  return `sha256:${hashString(JSON.stringify(rows))}`;
}

// ============================================================================
// Test Discovery
// ============================================================================

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
    if (entry && entry.version === DTS_DISCOVERY_CACHE_VERSION && entry.mtimeMs === stat.mtimeMs && entry.size === stat.size) {
      return { file, hasDts: entry.hasDts };
    }

    const content = await readLimit(() => fs.promises.readFile(fullPath, 'utf-8'));
    const hasDts = parseBaseline(content).dtsOutputs.length > 0;
    updated[file] = { version: DTS_DISCOVERY_CACHE_VERSION, mtimeMs: stat.mtimeMs, size: stat.size, hasDts };
    return { file, hasDts };
  })));

  saveDtsDiscoveryCache(updated);
  return checks.filter(c => c.hasDts).map(c => c.file);
}

// Tries to open the baseline blob (if fresh) and falls back to per-file
// reads otherwise. The blob is a one-time `open()` + offset-sliced reads,
// vs ~13,800 individual `open()` syscalls.
//
// **Default: OFF** (opt in via TSZ_EMIT_BLOB=1).
// A/B measurement on M4 Max with warm OS file cache (3 cold-result-cache
// runs each, 13,530 tests): no-blob 19.7/22.0/22.2s vs blob
// 21.6/21.8/22.2s — statistically equivalent. The per-file path is
// already fast enough that the blob's JSON-index parse + setup cost
// erases the syscall savings. Kept as opt-in infrastructure because the
// expected win is on CI cold disk cache (untested here).
async function openBaselineSource(): Promise<BaselineBlobReader | null> {
  if (process.env.TSZ_EMIT_BLOB !== '1') return null;
  try {
    if (!await isBlobFresh(BASELINES_DIR, path.join(__dirname, '..', '.baseline-blob-cache', 'baselines.meta.json'))) {
      if (process.env.TSZ_EMIT_NO_BLOB_BUILD === '1') return null;
      console.log(pc.dim('  Building baseline blob (one-time cost)...'));
      await buildBlob(
        BASELINES_DIR,
        path.join(__dirname, '..', '.baseline-blob-cache', 'baselines.bin'),
        path.join(__dirname, '..', '.baseline-blob-cache', 'baselines.idx.json'),
        path.join(__dirname, '..', '.baseline-blob-cache', 'baselines.meta.json'),
      );
    }
    return await BaselineBlobReader.tryLoad(BASELINES_DIR);
  } catch {
    return null;
  }
}

async function readBaselineFile(
  blob: BaselineBlobReader | null,
  name: string,
): Promise<string> {
  if (blob && blob.has(name)) {
    const buf = await blob.readBaseline(name);
    if (buf) return buf.toString('utf-8');
  }
  return await fs.promises.readFile(path.join(BASELINES_DIR, name), 'utf-8');
}

async function findTestCases(filter: string, maxTests: number, dtsOnly: boolean): Promise<TestCase[]> {
  if (!fs.existsSync(BASELINES_DIR)) {
    console.error(`Baselines directory not found: ${BASELINES_DIR}`);
    process.exit(1);
  }

  const blob = await openBaselineSource();
  if (blob) {
    const stats = await blob.stats();
    console.log(pc.dim(`  Baseline blob: ${stats.entries} entries (loaded in 1 open() call)`));
  }

  const submoduleEntries = fs.readdirSync(BASELINES_DIR);
  const submoduleNames = new Set(submoduleEntries);
  const entries = submoduleEntries;
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

  // Capability/parser outcomes never change inventory membership. Apply the
  // requested cap once, before reading, and retain every selected candidate.
  jsFiles = retainCanonicalInventory(jsFiles, maxTests);

  // Read and parse baseline files in parallel
  const readLimit = pLimit(64);
  const parsedSourceCache = new Map<string, ParsedSourceTest>();
  const sourceReadFailures = new Set<string>();
  const results = await Promise.all(jsFiles.map(baselineFile => readLimit(async () => {
    const baselineContent = await readBaselineFile(blob, baselineFile);
    const baseline = parseBaseline(baselineContent);

    const variant = extractAuthoredVariantFromFilename(baselineFile);

    let directives: Record<string, unknown> = {};
    let sourceFiles = baseline.sourceFiles;
    let source = baseline.source;
    let sourceFileName = baseline.sourceFileName;
    let links: LinkInput[] = [];
    let sourceReadFailed = false;
    if (baseline.testPath) {
      const cached = parsedSourceCache.get(baseline.testPath);
      if (cached) {
        sourceReadFailed = sourceReadFailures.has(baseline.testPath);
        directives = cached.options;
        links = cached.links;
        if (cached.sourceFiles.length > 0) {
          sourceFiles = cached.sourceFiles;
          source = cached.source ?? source;
          sourceFileName = cached.sourceFileName ?? sourceFileName;
        }
      } else {
        try {
          const testFileContent = await readTypeScriptTestFile(TS_DIR, baseline.testPath);
          const parsedSource = parseSourceTest(testFileContent, path.basename(baseline.testPath));
          directives = parsedSource.options;
          if (parsedSource.sourceFiles.length > 0) {
            sourceFiles = parsedSource.sourceFiles;
            source = parsedSource.source ?? source;
            sourceFileName = parsedSource.sourceFileName ?? sourceFileName;
          }
          links = parsedSource.links;
          parsedSourceCache.set(baseline.testPath, parsedSource);
        } catch {
          sourceReadFailed = true;
          sourceReadFailures.add(baseline.testPath);
          parsedSourceCache.set(baseline.testPath, {
            options: directives,
            source: null,
            sourceFileName: null,
            sourceFiles: [],
            links: [],
          });
        }
      }
    }

    const embeddedConfig = parseEmbeddedCompilerOptions(sourceFiles);
    const tsconfigOptions = embeddedConfig.compilerOptions;
    const authoredOptions = resolveAuthoredOptions({
      variant,
      directives,
      embeddedConfig: tsconfigOptions,
    });
    const targetText = optionString(authoredOptions, 'target');
    const target = targetText === undefined ? undefined : parseTarget(targetText);
    const effectiveTarget = target ?? 12;
    const moduleText = optionString(authoredOptions, 'module');
    const module = moduleText === undefined ? undefined : parseModule(moduleText);
    const effectiveModule = module ?? inferDefaultModule(effectiveTarget);
    const moduleResolution = optionString(authoredOptions, 'moduleResolution');
    const normalizedModuleResolution = moduleResolution?.toLowerCase() ?? '';
    const lib = optionLibList(authoredOptions);
    const alwaysStrict = optionBoolean(authoredOptions, 'alwaysStrict');
    const sourceMap = optionBoolean(authoredOptions, 'sourceMap');
    const inlineSourceMap = optionBoolean(authoredOptions, 'inlineSourceMap');
    const declarationMap = optionBoolean(authoredOptions, 'declarationMap');
    const downlevelIteration = optionBoolean(authoredOptions, 'downlevelIteration');
    const noEmitHelpers = optionBoolean(authoredOptions, 'noEmitHelpers');
    const noEmitOnError = optionBoolean(authoredOptions, 'noEmitOnError');
    const noCheck = optionBoolean(authoredOptions, 'noCheck');
    const noLib = optionBoolean(authoredOptions, 'noLib');
    const noEmit = optionBoolean(authoredOptions, 'noEmit');
    const declaration = optionBoolean(authoredOptions, 'declaration');
    const emitDeclarationOnly = optionBoolean(authoredOptions, 'emitDeclarationOnly');
    const importHelpers = optionBoolean(authoredOptions, 'importHelpers');
    const esModuleInterop = optionBoolean(authoredOptions, 'esModuleInterop');
    const useDefineForClassFields = optionBoolean(authoredOptions, 'useDefineForClassFields');
    const experimentalDecorators = optionBoolean(authoredOptions, 'experimentalDecorators');
    const emitDecoratorMetadata = optionBoolean(authoredOptions, 'emitDecoratorMetadata');
    const strict = optionBoolean(authoredOptions, 'strict');
    // Strict is forwarded as one authored option. Never approximate it by
    // synthesizing strictNullChecks or any other strict-family suboption.
    const strictNullChecks = optionBoolean(authoredOptions, 'strictNullChecks');
    const exactOptionalPropertyTypes = optionBoolean(authoredOptions, 'exactOptionalPropertyTypes');
    const jsx = optionString(authoredOptions, 'jsx');
    const jsxFactory = optionString(authoredOptions, 'jsxFactory');
    const jsxFragmentFactory = optionString(authoredOptions, 'jsxFragmentFactory');
    const jsxImportSource = optionString(authoredOptions, 'jsxImportSource');
    const moduleDetection = optionString(authoredOptions, 'moduleDetection');
    const preserveConstEnums = optionBoolean(authoredOptions, 'preserveConstEnums');
    const verbatimModuleSyntax = optionBoolean(authoredOptions, 'verbatimModuleSyntax');
    const rewriteRelativeImportExtensions = optionBoolean(authoredOptions, 'rewriteRelativeImportExtensions');
    const isolatedModules = optionBoolean(authoredOptions, 'isolatedModules');
    const importsNotUsedAsValues = optionString(authoredOptions, 'importsNotUsedAsValues');
    const preserveValueImports = optionBoolean(authoredOptions, 'preserveValueImports');
    const removeComments = optionBoolean(authoredOptions, 'removeComments');
    const stripInternal = optionBoolean(authoredOptions, 'stripInternal');
    const allowJs = optionBoolean(authoredOptions, 'allowJs');
    const allowUnreachableCode = optionBoolean(authoredOptions, 'allowUnreachableCode');
    const checkJs = optionBoolean(authoredOptions, 'checkJs');
    const noImplicitAny = optionBoolean(authoredOptions, 'noImplicitAny');
    const noUnusedLocals = optionBoolean(authoredOptions, 'noUnusedLocals');
    const noUnusedParameters = optionBoolean(authoredOptions, 'noUnusedParameters');
    const skipLibCheck = optionBoolean(authoredOptions, 'skipLibCheck');
    const strictPropertyInitialization = optionBoolean(authoredOptions, 'strictPropertyInitialization');
    const noImplicitReferences = optionBoolean(authoredOptions, 'noImplicitReferences');
    const baseUrl = optionString(authoredOptions, 'baseUrl');
    const outFile = optionString(authoredOptions, 'outFile');
    const outDir = optionString(authoredOptions, 'outDir');
    const declarationDir = optionString(authoredOptions, 'declarationDir');
    const rootDir = optionString(authoredOptions, 'rootDir');
    const hasMapOption = [
      'sourcemap',
      'inlinesourcemap',
      'declarationmap',
      'maproot',
      'sourceroot',
      'inlinesources',
    ].some(key => authoredOptions.has(key));
    const rootSelection = selectHarnessRootFiles(sourceFiles, noImplicitReferences);
    const unsupportedReasons = canonicalUnsupportedReasons({
      parserHasSource: sourceFiles.length > 0,
      parserHasJs: baseline.jsOutputs.length > 0 || emitDeclarationOnly === true || noEmit === true,
      parserHasDts: baseline.dtsOutputs.length > 0 || noEmit === true,
      dtsOnly,
      sourceReadFailed,
      target: effectiveTarget,
      module: effectiveModule,
      moduleResolution: normalizedModuleResolution,
      alwaysStrict,
      hasDownlevelIterationOption: authoredOptions.has('downleveliteration'),
      esModuleInteropDisabled: esModuleInterop === false,
      hasOutFile: outFile !== undefined,
      hasBaseUrl: baseUrl !== undefined,
      hasMapOption,
      hasBaselineSidecar: hasEmitSidecar(baselineFile, submoduleNames),
    });
    if (
      baseline.dtsOutputs.length > 0 &&
      declaration !== true &&
      emitDeclarationOnly !== true &&
      noEmit !== true
    ) {
      unsupportedReasons.push('declaration-product-domain-without-authored-declaration');
    }
    unsupportedReasons.push(...embeddedConfig.reasons);
    if (rootSelection.unsupportedReason) {
      unsupportedReasons.push(rootSelection.unsupportedReason);
    }
    unsupportedReasons.push(...authoredOptionFailureReasons(authoredOptions));

    return {
      baselineFile,
      testPath: baseline.testPath,
      sourceFileName,
      sourceFiles,
      rootFileNames: rootSelection.rootFileNames,
      links,
      source: source ?? baseline.source ?? '',
      jsProductDomain: baseline.jsOutputs.map(product => product.name),
      dtsProductDomain: baseline.dtsOutputs.map(product => product.name),
      target,
      module,
      lib,
      alwaysStrict,
      sourceMap,
      inlineSourceMap,
      downlevelIteration,
      noEmitHelpers,
      noEmitOnError,
      noCheck,
      noLib,
      noEmit,
      declaration,
      importHelpers,
      esModuleInterop,
      useDefineForClassFields,
      experimentalDecorators,
      emitDecoratorMetadata,
      strict,
      strictNullChecks,
      exactOptionalPropertyTypes,
      jsx,
      jsxFactory,
      jsxFragmentFactory,
      jsxImportSource,
      moduleResolution,
      moduleDetection,
      preserveConstEnums,
      verbatimModuleSyntax,
      rewriteRelativeImportExtensions,
      isolatedModules,
      importsNotUsedAsValues,
      preserveValueImports,
      removeComments,
      stripInternal,
      allowJs,
      allowUnreachableCode,
      checkJs,
      noImplicitAny,
      noUnusedLocals,
      noUnusedParameters,
      skipLibCheck,
      strictPropertyInitialization,
      baseUrl,
      outFile,
      outDir,
      declarationDir,
      rootDir,
      emitDeclarationOnly,
      declarationMap,
      unsupportedReason: unsupportedReasons.length > 0 ? unsupportedReasons.join(', ') : undefined,
    } as TestCase;
  })));

  // Release the blob's file handle now that discovery is done. Node 25+
  // makes implicit FH-on-GC closure a hard error; we own the lifetime.
  if (blob) await blob.close();

  return results;
}

// ============================================================================
// Test Execution
// ============================================================================

function formatProductComparison(comparison: ProductSetComparison, verbose: boolean): string {
  const first = comparison.mismatches[0];
  if (!first) return '';
  const suffix = comparison.mismatches.length > 1
    ? ` (${comparison.mismatches.length} product mismatches total)`
    : '';
  if (first.kind === 'content' && first.expected !== undefined && first.actual !== undefined) {
    const diff = verbose
      ? getEmitDiff(first.expected, first.actual)
      : getEmitDiffSummary(first.expected, first.actual);
    return `Content mismatch at ${first.path}: ${diff}${suffix}`;
  }
  if (first.kind === 'missing') return `Missing emit product: ${first.path}${suffix}`;
  if (first.kind === 'extra') return `Unexpected emit product: ${first.path}${suffix}`;
  if (first.kind === 'duplicate-oracle') return `Duplicate TypeScript 7 product path: ${first.path}${suffix}`;
  return `Duplicate TSZ product path: ${first.path}${suffix}`;
}

async function runTest(
  oracleTranspiler: CliTranspiler,
  tszTranspiler: CliTranspiler,
  testCase: TestCase,
  config: Config,
): Promise<TestResult> {
  const start = Date.now();
  const testName = testCase.baselineFile.replace('.js', '');
  const emitDeclarations = testCase.declaration === true || testCase.emitDeclarationOnly === true;
  const selected = selectArtifactSurfaces(config, emitDeclarations);

  const result: TestResult = {
    name: testName,
    testPath: testCase.testPath,
    jsSelected: selected.js,
    dtsSelected: selected.dts,
    outcomeMatch: null,
    jsMatch: null,
    dtsMatch: null,
    jsProductMatch: null,
    dtsProductMatch: null,
    artifactState: 'incomplete',
  };

  try {
    if (testCase.unsupportedReason) {
      result.artifactState = 'unsupported';
      const message = `UNSUPPORTED_CANONICAL_EMIT: ${testCase.unsupportedReason}`;
      const jsObservation = artifactSurfaceObservation('unsupported', selected.js, null, null);
      const dtsObservation = artifactSurfaceObservation('unsupported', selected.dts, null, null);
      result.jsMatch = jsObservation.match;
      result.dtsMatch = dtsObservation.match;
      if (selected.js) {
        result.jsError = message;
      }
      if (selected.dts) {
        result.dtsError = message;
      }
      result.elapsed = Date.now() - start;
      return result;
    }

    const transpileOptions = {
      sourceFileName: testCase.sourceFileName ?? undefined,
      declaration: testCase.declaration,
      emitDeclarationOnly: testCase.emitDeclarationOnly,
      noCheck: testCase.noCheck,
      noLib: testCase.noLib,
      noEmit: testCase.noEmit,
      lib: testCase.lib,
      alwaysStrict: testCase.alwaysStrict,
      sourceMap: testCase.sourceMap,
      inlineSourceMap: testCase.inlineSourceMap,
      downlevelIteration: testCase.downlevelIteration,
      noEmitHelpers: testCase.noEmitHelpers,
      noEmitOnError: testCase.noEmitOnError,
      strict: testCase.strict,
      allowJs: testCase.allowJs,
      allowUnreachableCode: testCase.allowUnreachableCode,
      checkJs: testCase.checkJs,
      noImplicitAny: testCase.noImplicitAny,
      noUnusedLocals: testCase.noUnusedLocals,
      noUnusedParameters: testCase.noUnusedParameters,
      skipLibCheck: testCase.skipLibCheck,
      strictPropertyInitialization: testCase.strictPropertyInitialization,
      importHelpers: testCase.importHelpers,
      esModuleInterop: testCase.esModuleInterop,
      useDefineForClassFields: testCase.useDefineForClassFields,
      experimentalDecorators: testCase.experimentalDecorators,
      emitDecoratorMetadata: testCase.emitDecoratorMetadata,
      strictNullChecks: testCase.strictNullChecks,
      exactOptionalPropertyTypes: testCase.exactOptionalPropertyTypes,
      jsx: testCase.jsx,
      jsxFactory: testCase.jsxFactory,
      jsxFragmentFactory: testCase.jsxFragmentFactory,
      jsxImportSource: testCase.jsxImportSource,
      moduleResolution: testCase.moduleResolution,
      moduleDetection: testCase.moduleDetection,
      preserveConstEnums: testCase.preserveConstEnums,
      verbatimModuleSyntax: testCase.verbatimModuleSyntax,
      rewriteRelativeImportExtensions: testCase.rewriteRelativeImportExtensions,
      isolatedModules: testCase.isolatedModules,
      importsNotUsedAsValues: testCase.importsNotUsedAsValues,
      preserveValueImports: testCase.preserveValueImports,
      removeComments: testCase.removeComments,
      stripInternal: testCase.stripInternal,
      baseUrl: testCase.baseUrl,
      outFile: testCase.outFile,
      outDir: testCase.outDir,
      declarationDir: testCase.declarationDir,
      rootDir: testCase.rootDir,
      declarationMap: testCase.declarationMap,
      sourceFiles: testCase.sourceFiles,
      rootFileNames: testCase.rootFileNames,
      links: testCase.links,
    };

    // The oracle result never feeds TSZ arguments or output selection. Both
    // external processes receive the same authored options and independently
    // staged source graph, then their complete product maps are compared.
    const [oracleResult, tszResult] = await Promise.all([
      oracleTranspiler.transpile(testCase.source, testCase.target, testCase.module, transpileOptions),
      tszTranspiler.transpile(testCase.source, testCase.target, testCase.module, transpileOptions),
    ]);

    const outcomeComparison = compareCompilerOutcomes(oracleResult.outcome, tszResult.outcome);
    result.artifactState = compilerArtifactState(oracleResult.outcome, tszResult.outcome);
    result.outcomeMatch = outcomeComparison.match;
    result.outcomeError = outcomeComparison.error;
    const jsComparison = compareCanonicalProductSets(oracleResult.jsProducts, tszResult.jsProducts);
    const dtsComparison = compareCanonicalProductSets(oracleResult.dtsProducts, tszResult.dtsProducts);
    const jsObservation = artifactSurfaceObservation(
      result.artifactState,
      selected.js,
      outcomeComparison.match,
      jsComparison.match,
    );
    const dtsObservation = artifactSurfaceObservation(
      result.artifactState,
      selected.dts,
      outcomeComparison.match,
      dtsComparison.match,
    );

    result.jsMatch = jsObservation.match;
    result.dtsMatch = dtsObservation.match;
    result.jsProductMatch = jsObservation.productMatch;
    result.dtsProductMatch = dtsObservation.productMatch;
    if (selected.js && !jsComparison.match) {
      result.jsProductError = formatProductComparison(jsComparison, config.verbose);
    }
    if (selected.dts && !dtsComparison.match) {
      result.dtsProductError = formatProductComparison(dtsComparison, config.verbose);
    }
    if (selected.js && !jsObservation.match) {
      result.jsError = outcomeComparison.error ?? result.jsProductError;
    }
    if (selected.dts && !dtsObservation.match) {
      result.dtsError = outcomeComparison.error ?? result.dtsProductError;
    }

    result.elapsed = Date.now() - start;
  } catch (e) {
    const errorMsg = e instanceof Error ? e.message : String(e);
    const summarized = summarizeErrorMessage(errorMsg);
    result.timeout = errorMsg.startsWith('TIMEOUT:');
    result.artifactState = result.timeout ? 'timeout' : 'crash';
    const jsObservation = artifactSurfaceObservation(result.artifactState, selected.js, null, null);
    const dtsObservation = artifactSurfaceObservation(result.artifactState, selected.dts, null, null);
    result.jsMatch = jsObservation.match;
    result.dtsMatch = dtsObservation.match;
    if (selected.js) {
      result.jsError = result.timeout ? 'TIMEOUT' : summarized;
    }
    if (selected.dts) {
      result.dtsError = result.timeout ? 'TIMEOUT' : summarized;
    }
    result.elapsed = Date.now() - start;
  }

  return result;
}

// ============================================================================
// Display Helpers
// ============================================================================

function resultStatusIcon(result: TestResult, config: Config): string {
  if (result.skipped) return pc.dim('S');
  const statuses = config.dtsOnly
    ? [artifactStatus(result.artifactState, result.dtsMatch)]
    : [
        artifactStatus(result.artifactState, result.jsMatch),
        ...(!config.jsOnly && result.dtsMatch !== null
          ? [artifactStatus(result.artifactState, result.dtsMatch)]
          : []),
      ];
  const status = (['fail', 'crash', 'timeout', 'incomplete', 'unsupported', 'pass', 'skip'] as const)
    .find(candidate => statuses.includes(candidate)) ?? 'skip';
  switch (status) {
    case 'pass': return pc.green('✓');
    case 'fail': return pc.red('✗');
    case 'timeout': return pc.yellow('T');
    case 'crash': return pc.red('C');
    case 'incomplete': return pc.yellow('I');
    case 'unsupported': return pc.yellow('U');
    case 'skip': return pc.dim('-');
  }
}

function printVerboseResult(result: TestResult, config: Config) {
  console.log(`  [${resultStatusIcon(result, config)}] ${result.name} (${result.elapsed}ms)`);
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

function printSurfaceSummary(
  label: string,
  counts: ArtifactStatusCounts,
  products: ArtifactProductCounts,
): void {
  const total = artifactCandidateTotal(counts);
  const pct = total > 0 ? (counts.pass / total * 100).toFixed(1) : '0.0';
  console.log(pc.bold(`${label}:`));
  console.log(`  ${pc.green(`Passed: ${counts.pass}`)}`);
  console.log(`  ${pc.red(`Failed: ${total - counts.pass}`)}`);
  console.log(`  ${pc.red(`Product mismatches: ${products.mismatch}`)}`);
  console.log(`  ${pc.dim(`Product comparisons unavailable: ${products.unmeasured}`)}`);
  console.log(`  ${pc.yellow(`Incomplete: ${counts.incomplete}`)}`);
  console.log(`  ${pc.yellow(`Unsupported: ${counts.unsupported}`)}`);
  console.log(`  ${pc.red(`Crashed: ${counts.crash}`)}`);
  console.log(`  ${pc.yellow(`Timed out: ${counts.timeout}`)}`);
  console.log(`  ${pc.dim(`Skipped: ${counts.skip}`)}`);
  console.log(`  ${pc.yellow(`Pass Rate: ${pct}% (${counts.pass}/${total})`)}`);
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

  if (config.jsOnly && config.dtsOnly) {
    throw new Error('--js-only and --dts-only are mutually exclusive');
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
  console.log(pc.dim('  Engine: TSZ CLI against pinned TypeScript 7 external-process oracle'));
  console.log(sep);
  console.log('');

  const oracle = resolvePinnedOracle(ROOT_DIR);
  console.log(pc.dim(`  Oracle: TypeScript ${oracle.provenance.version} (${oracle.provenance.fingerprint})`));

  console.log(pc.dim('Discovering test cases...'));
  // Discover more tests than needed when offset is used, then slice
  const discoveredLimit = config.offset > 0 ? config.maxTests + config.offset : config.maxTests;
  let testCases = await findTestCases(config.filter, discoveredLimit, config.dtsOnly);
  if (config.offset > 0) {
    testCases = testCases.slice(config.offset, config.offset + config.maxTests);
  }
  console.log(pc.dim(`Found ${testCases.length} test cases`));
  if (testCases.length === 0) {
    throw new Error(
      `No canonical emit test cases selected (filter=${config.filter || '<none>'}, offset=${config.offset})`,
    );
  }
  console.log('');

  const oracleTranspiler = new CliTranspiler(config.timeoutMs, {
    binaryPath: oracle.binaryPath,
    label: 'typescript-7-oracle',
    diagnosticWitnessProvider: 'typescript-7-api',
  });
  const tszTranspiler = new CliTranspiler(config.timeoutMs);

  // Ensure child processes are killed on unexpected exit.
  const cleanup = () => {
    oracleTranspiler.terminate();
    tszTranspiler.terminate();
  };
  process.on('SIGINT', () => { cleanup(); process.exit(130); });
  process.on('SIGTERM', () => { cleanup(); process.exit(143); });

  // Per-status counters preserve the candidate domain without calling typed
  // terminal observations product mismatches.
  const jsCounts = emptyArtifactStatusCounts();
  const dtsCounts = emptyArtifactStatusCounts();
  const jsProducts = emptyArtifactProductCounts();
  const dtsProducts = emptyArtifactProductCounts();
  const failures: TestResult[] = [];
  const allTestResults: TestResult[] = [];
  const startTime = Date.now();
  let completed = 0;

  function recordResult(result: TestResult) {
    ensureMeasuredArtifact(result, { js: result.jsSelected, dts: result.dtsSelected });
    completed++;
    allTestResults.push(result);
    const jsStatus = artifactStatus(result.artifactState, result.jsMatch);
    const dtsStatus = artifactStatus(result.artifactState, result.dtsMatch);
    recordArtifactStatus(jsCounts, jsStatus);
    recordArtifactStatus(dtsCounts, dtsStatus);
    recordArtifactProduct(jsProducts, result.jsSelected, result.jsProductMatch);
    recordArtifactProduct(dtsProducts, result.dtsSelected, result.dtsProductMatch);
    if (![jsStatus, dtsStatus].every(status => status === 'pass' || status === 'skip')) {
      failures.push(result);
    }
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
      const result = await runTest(oracleTranspiler, tszTranspiler, tc, config);
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
      const result = await runTest(oracleTranspiler, tszTranspiler, tc, config);
      recordResult(result);
      printProgress();
    })));
  }

  // Cleanup
  cleanup();

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

  // Summary
  console.log('\n');
  console.log(sep);
  console.log(pc.bold('EMIT TEST RESULTS'));
  console.log(sep);

  if (!config.dtsOnly) {
    printSurfaceSummary('JavaScript Emit', jsCounts, jsProducts);
  }

  if (!config.jsOnly && artifactCandidateTotal(dtsCounts) > 0) {
    printSurfaceSummary('Declaration Emit', dtsCounts, dtsProducts);
  }

  const totalTests = testCases.length;
  const rate = totalTests > 0 ? Math.round(totalTests / parseFloat(elapsed)) : 0;
  console.log(pc.dim(`\nTime: ${elapsed}s (${rate} tests/sec)`));
  console.log(sep);

  // Show first non-passes (excluding timeouts)
  const realFailures = failures.filter(f => !f.timeout);
  if (realFailures.length > 0 && !config.verbose) {
    console.log(`\n${pc.bold('First non-passes:')}`);
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
      artifactState: ArtifactState;
      jsSelected: boolean;
      dtsSelected: boolean;
      outcomeMatch: boolean | null;
      jsMatch: boolean | null;
      dtsMatch: boolean | null;
      jsProductMatch: boolean | null;
      dtsProductMatch: boolean | null;
      jsStatus: ArtifactStatus;
      dtsStatus: ArtifactStatus;
      outcomeError?: string;
      jsError?: string;
      dtsError?: string;
      jsProductError?: string;
      dtsProductError?: string;
      elapsed?: number;
    }> = [];

    for (const r of allTestResults) {
      const jsStatus = artifactStatus(r.artifactState, r.jsMatch);
      const dtsStatus = artifactStatus(r.artifactState, r.dtsMatch);

      const record: any = {
        name: r.name,
        baselineFile: r.name + '.js',
        testPath: r.testPath,
        artifactState: r.artifactState,
        jsSelected: r.jsSelected,
        dtsSelected: r.dtsSelected,
        outcomeMatch: r.outcomeMatch,
        jsMatch: r.jsMatch,
        dtsMatch: r.dtsMatch,
        jsProductMatch: r.jsProductMatch,
        dtsProductMatch: r.dtsProductMatch,
        jsStatus,
        dtsStatus,
      };
      if (r.outcomeError) record.outcomeError = r.outcomeError;
      if (r.jsError) record.jsError = r.jsError;
      if (r.dtsError) record.dtsError = r.dtsError;
      if (r.jsProductError) record.jsProductError = r.jsProductError;
      if (r.dtsProductError) record.dtsProductError = r.dtsProductError;
      if (r.elapsed !== undefined) record.elapsed = r.elapsed;
      allResults.push(record);
    }

    const jsTotal = artifactCandidateTotal(jsCounts);
    const dtsTotal = artifactCandidateTotal(dtsCounts);
    // Stamp the measured tree so observational artifacts retain provenance.
    // Degrades to undefined off a git checkout rather than failing the emit run.
    let gitSha: string | undefined;
    try {
      gitSha = execFileSync('git', ['rev-parse', 'HEAD'], {
        cwd: ROOT_DIR,
        encoding: 'utf8',
      }).trim() || undefined;
    } catch {
      gitSha = undefined;
    }
    const detail = {
      detailSchemaVersion: 2,
      timestamp: new Date().toISOString(),
      ...(gitSha ? { git_sha: gitSha } : {}),
      oracle: oracle.provenance,
      detailFingerprint: detailRowsFingerprint(allResults),
      detailResultCount: allResults.length,
      summary: {
        jsTotal,
        jsPass: jsCounts.pass,
        jsFail: jsTotal - jsCounts.pass,
        jsSkip: jsCounts.skip,
        jsCompleteMismatch: jsCounts.fail,
        jsUnsupported: jsCounts.unsupported,
        jsTimeout: jsCounts.timeout,
        jsCrash: jsCounts.crash,
        jsIncomplete: jsCounts.incomplete,
        jsProductMatch: jsProducts.match,
        jsProductMismatch: jsProducts.mismatch,
        jsProductUnmeasured: jsProducts.unmeasured,
        jsPassRate: jsTotal > 0 ? Math.round(jsCounts.pass / jsTotal * 1000) / 10 : 0,
        dtsTotal,
        dtsPass: dtsCounts.pass,
        dtsFail: dtsTotal - dtsCounts.pass,
        dtsSkip: dtsCounts.skip,
        dtsCompleteMismatch: dtsCounts.fail,
        dtsUnsupported: dtsCounts.unsupported,
        dtsTimeout: dtsCounts.timeout,
        dtsCrash: dtsCounts.crash,
        dtsIncomplete: dtsCounts.incomplete,
        dtsProductMatch: dtsProducts.match,
        dtsProductMismatch: dtsProducts.mismatch,
        dtsProductUnmeasured: dtsProducts.unmeasured,
        dtsPassRate: dtsTotal > 0 ? Math.round(dtsCounts.pass / dtsTotal * 1000) / 10 : 0,
      },
      results: allResults,
    };

    const outPath = path.resolve(config.jsonOut);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(detail, null, 2));
    console.log(pc.dim(`\nJSON results written to ${outPath}`));
  }

  process.exit(artifactHasNonPass(jsCounts) || artifactHasNonPass(dtsCounts) ? 1 : 0);
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
