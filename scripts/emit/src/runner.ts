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

const DEFAULT_TIMEOUT_MS = 5000;

// ============================================================================
// Types
// ============================================================================

interface Config {
  maxTests: number;
  filter: string;
  verbose: boolean;
  jsOnly: boolean;
  dtsOnly: boolean;
  concurrency: number;
  timeoutMs: number;
}

interface TestCase {
  baselineFile: string;
  testPath: string | null;
  source: string;
  expectedJs: string | null;
  expectedDts: string | null;
  target: number;
  module: number;
  alwaysStrict: boolean;
  sourceMap: boolean;
}

interface TestResult {
  name: string;
  jsMatch: boolean | null;
  dtsMatch: boolean | null;
  jsError?: string;
  dtsError?: string;
  elapsed?: number;
  skipped?: boolean;
  timeout?: boolean;
}

interface CacheEntry {
  hash: string;
  jsOutput: string;
  dtsOutput: string | null;
}

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

function getCacheKey(source: string, target: number, module: number, alwaysStrict: boolean, declaration: boolean, sourceMap: boolean = false): string {
  return hashString(`${source}:${target}:${module}:${alwaysStrict}:${declaration}:${sourceMap}`);
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
  if (lower.includes('esnext')) return 99;
  return 1;
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
  if (lower === 'nodenext') return 199;
  return 0;
}

function extractVariantFromFilename(filename: string): { base: string; target?: string; module?: string } {
  const match = filename.match(/^(.+?)\(([^)]+)\)\.js$/);
  if (!match) {
    return { base: filename.replace('.js', '') };
  }

  const base = match[1];
  const variants = match[2].split(',').map(v => v.trim());
  const result: { base: string; target?: string; module?: string } = { base };

  for (const variant of variants) {
    const [key, value] = variant.split('=');
    if (key === 'target') result.target = value;
    if (key === 'module') result.module = value;
  }

  return result;
}

function parseSourceDirectives(source: string): Record<string, unknown> {
  const options: Record<string, unknown> = {};
  for (const line of source.split('\n')) {
    const trimmed = line.trim();
    const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
    if (optionMatch) {
      const [, key, value] = optionMatch;
      const lowKey = key.toLowerCase();
      if (value.toLowerCase() === 'true') options[lowKey] = true;
      else if (value.toLowerCase() === 'false') options[lowKey] = false;
      else options[lowKey] = value;
    }
  }
  return options;
}

async function findTestCases(filter: string, maxTests: number): Promise<TestCase[]> {
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

  // Cap to maxTests before reading (we may discard some after parsing, so read a bit extra)
  if (maxTests < Infinity) {
    jsFiles = jsFiles.slice(0, Math.min(jsFiles.length, maxTests * 2));
  }

  // Read and parse baseline files in parallel
  const readLimit = pLimit(64);
  const results = await Promise.all(jsFiles.map(baselineFile => readLimit(async () => {
    const baselinePath = path.join(BASELINES_DIR, baselineFile);
    const baselineContent = await fs.promises.readFile(baselinePath, 'utf-8');
    const baseline = parseBaseline(baselineContent);

    if (!baseline.source || !baseline.js) return null;

    const variant = extractVariantFromFilename(baselineFile);

    let directives: Record<string, unknown> = {};
    if (baseline.testPath) {
      const testFilePath = path.join(TS_DIR, baseline.testPath);
      try {
        const testFileContent = await fs.promises.readFile(testFilePath, 'utf-8');
        directives = parseSourceDirectives(testFileContent);
      } catch {}
    }

    const target = variant.target ? parseTarget(variant.target)
      : directives.target ? parseTarget(String(directives.target))
      : 1;
    const module = variant.module ? parseModule(variant.module)
      : directives.module ? parseModule(String(directives.module))
      : target >= 2 ? 5  // When target >= ES2015 and no module specified, default to ES2015 modules
      : 0;

    const alwaysStrict = directives.strict === true || directives.alwaysstrict === true;
    const sourceMap = directives.sourcemap === true;

    return {
      baselineFile,
      testPath: baseline.testPath,
      source: baseline.source,
      expectedJs: baseline.js,
      expectedDts: baseline.dts,
      target,
      module,
      alwaysStrict,
      sourceMap,
    } as TestCase;
  })));

  // Filter nulls and cap to maxTests
  return results.filter((r): r is TestCase => r !== null).slice(0, maxTests);
}

// ============================================================================
// Test Execution
// ============================================================================

async function runTest(transpiler: CliTranspiler, testCase: TestCase, config: Config): Promise<TestResult> {
  const start = Date.now();
  const testName = testCase.baselineFile.replace('.js', '');

  const result: TestResult = {
    name: testName,
    jsMatch: null,
    dtsMatch: null,
  };

  try {
    loadCache();
    const cacheKey = getCacheKey(testCase.source, testCase.target, testCase.module, testCase.alwaysStrict, config.dtsOnly, testCase.sourceMap);
    let tszJs: string;
    let tszDts: string | null = null;

    const cached = cache.get(cacheKey);
    const sourceHash = hashString(testCase.source);

    if (cached && cached.hash === sourceHash) {
      tszJs = cached.jsOutput;
      tszDts = cached.dtsOutput;
    } else {
      const transpileResult = await transpiler.transpile(testCase.source, testCase.target, testCase.module, {
        declaration: config.dtsOnly,
        alwaysStrict: testCase.alwaysStrict,
        sourceMap: testCase.sourceMap,
      });
      tszJs = transpileResult.js;
      tszDts = transpileResult.dts || null;
      cache.set(cacheKey, { hash: sourceHash, jsOutput: tszJs, dtsOutput: tszDts });
    }

    if (!config.dtsOnly && testCase.expectedJs) {
      // Normalize sourceMappingURL filenames since we use temp file names
      const normalizeSourceMapUrl = (s: string) =>
        s.replace(/\/\/# sourceMappingURL=\S+/g, '//# sourceMappingURL=output.js.map');
      const expected = normalizeSourceMapUrl(testCase.expectedJs.replace(/\r\n/g, '\n').trim());
      const actual = normalizeSourceMapUrl(tszJs.replace(/\r\n/g, '\n').trim());
      result.jsMatch = expected === actual;

      if (!result.jsMatch) {
        result.jsError = config.verbose ? getEmitDiff(expected, actual) : getEmitDiffSummary(expected, actual);
      }
    }

    if (!config.jsOnly && testCase.expectedDts) {
      if (tszDts !== null) {
        const expected = testCase.expectedDts.replace(/\r\n/g, '\n').trim();
        const actual = tszDts.replace(/\r\n/g, '\n').trim();
        result.dtsMatch = expected === actual;

        if (!result.dtsMatch) {
          result.dtsError = config.verbose ? getEmitDiff(expected, actual) : getEmitDiffSummary(expected, actual);
        }
      } else {
        result.dtsMatch = null;
      }
    }

    result.elapsed = Date.now() - start;
  } catch (e) {
    const errorMsg = e instanceof Error ? e.message : String(e);
    result.timeout = errorMsg === 'TIMEOUT';
    result.jsError = result.timeout ? 'TIMEOUT' : errorMsg;
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
  const match = dtsOnly ? result.dtsMatch : result.jsMatch;
  if (match === true) return pc.green('✓');
  if (match === false) return pc.red('✗');
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
    filter: '',
    verbose: false,
    jsOnly: false,
    dtsOnly: false,
    concurrency: Math.max(1, os.cpus().length),
    timeoutMs: DEFAULT_TIMEOUT_MS,
  };

  for (const arg of args) {
    if (arg.startsWith('--max=')) {
      config.maxTests = parseInt(arg.slice(6), 10);
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
  console.log(pc.dim(`  Engine: Native CLI (${config.dtsOnly ? 'with type checking' : 'emit-only, --noCheck --noLib'})`));
  console.log(sep);
  console.log('');

  const transpiler = new CliTranspiler(config.timeoutMs);

  // Ensure child processes are killed on unexpected exit
  const cleanup = () => { transpiler.terminate(); };
  process.on('SIGINT', () => { cleanup(); process.exit(130); });
  process.on('SIGTERM', () => { cleanup(); process.exit(143); });

  console.log(pc.dim('Discovering test cases...'));
  const testCases = await findTestCases(config.filter, config.maxTests);
  console.log(pc.dim(`Found ${testCases.length} test cases`));
  console.log('');

  // Counters
  let jsPass = 0, jsFail = 0, jsSkip = 0, jsTimeout = 0;
  let dtsPass = 0, dtsFail = 0, dtsSkip = 0;
  const failures: TestResult[] = [];
  const startTime = Date.now();
  let completed = 0;

  function recordResult(result: TestResult) {
    completed++;
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
    else if (result.dtsMatch === false) dtsFail++;
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
    for (const f of realFailures.slice(0, 10)) {
      const diffInfo = f.jsError ? ` ${pc.dim(`(${f.jsError})`)}` : '';
      console.log(`  ${pc.red('✗')} ${f.name}${diffInfo}`);
    }
    if (realFailures.length > 10) {
      console.log(`  ${pc.dim(`... and ${realFailures.length - 10} more`)}`);
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
