#!/usr/bin/env node
/**
 * TSZ Emit Test Runner
 *
 * Compares tsz JavaScript/Declaration emit output against TypeScript's baselines.
 * Uses worker threads with timeout protection to prevent hangs.
 *
 * Usage:
 *   ./run.sh [options]
 *
 * Options:
 *   --max=N           Maximum number of tests to run
 *   --filter=PATTERN  Only run tests matching pattern
 *   --verbose         Show detailed output
 *   --js-only         Only test JavaScript emit
 *   --dts-only        Only test declaration emit
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { fileURLToPath } from 'url';
import { Worker } from 'worker_threads';
import { parseBaseline, getEmitDiff, getEmitDiffSummary } from './baseline-parser.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');
const TS_DIR = path.join(ROOT_DIR, 'TypeScript');
const BASELINES_DIR = path.join(TS_DIR, 'tests/baselines/reference');
const CACHE_DIR = path.join(__dirname, '../.cache');

// Configuration
const TEST_TIMEOUT_MS = 400;   // 400ms timeout per test
const WORKER_RECYCLE_AFTER = 50; // Recycle worker after N tests to prevent memory buildup

// ANSI colors
const colors = {
  reset: '\x1b[0m',
  bold: '\x1b[1m',
  dim: '\x1b[2m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
};

interface Config {
  maxTests: number;
  filter: string;
  verbose: boolean;
  jsOnly: boolean;
  dtsOnly: boolean;
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

function getCacheKey(source: string, target: number, module: number): string {
  return hashString(`${source}:${target}:${module}`);
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
  return 1; // Default ES5
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
  return 0; // Default None
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

/**
 * Parse compiler directives from test source code.
 * Uses the same pattern as the conformance runner (scripts/conformance/src/tsc-runner.ts).
 * Directives are lines like: // @target: ES5, // @module: commonjs, // @strict: true
 */
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

function findTestCases(filter: string, maxTests: number): TestCase[] {
  const testCases: TestCase[] = [];
  
  if (!fs.existsSync(BASELINES_DIR)) {
    console.error(`Baselines directory not found: ${BASELINES_DIR}`);
    process.exit(1);
  }

  const entries = fs.readdirSync(BASELINES_DIR);
  const jsFiles = entries.filter(e => e.endsWith('.js')).sort();

  for (const baselineFile of jsFiles) {
    if (testCases.length >= maxTests) break;
    if (filter && !baselineFile.toLowerCase().includes(filter.toLowerCase())) continue;

    const baselinePath = path.join(BASELINES_DIR, baselineFile);
    const baselineContent = fs.readFileSync(baselinePath, 'utf-8');
    const baseline = parseBaseline(baselineContent);

    if (!baseline.source || !baseline.js) continue;

    // Extract variant from filename (e.g., test(target=ES5,module=commonjs).js)
    const variant = extractVariantFromFilename(baselineFile);

    // Parse source directives from the original test file
    // Directives like // @target: ES5, // @strict: true are in the .ts file, not the baseline
    let directives: Record<string, unknown> = {};
    if (baseline.testPath) {
      const testFilePath = path.join(TS_DIR, baseline.testPath);
      if (fs.existsSync(testFilePath)) {
        const testFileContent = fs.readFileSync(testFilePath, 'utf-8');
        directives = parseSourceDirectives(testFileContent);
      }
    }

    // Filename variants override source directives, which override defaults
    const target = variant.target ? parseTarget(variant.target)
      : directives.target ? parseTarget(String(directives.target))
      : 1; // Default ES5
    const module = variant.module ? parseModule(variant.module)
      : directives.module ? parseModule(String(directives.module))
      : 0; // Default None

    // Detect strict mode: @strict or @alwaysStrict directives
    const alwaysStrict = directives.strict === true || directives.alwaysstrict === true;

    testCases.push({
      baselineFile,
      testPath: baseline.testPath,
      source: baseline.source,
      expectedJs: baseline.js,
      expectedDts: baseline.dts,
      target,
      module,
      alwaysStrict,
    });
  }

  return testCases;
}

// ============================================================================
// Worker Management
// ============================================================================

class TranspileWorker {
  private worker: Worker | null = null;
  private jobId = 0;
  private pendingJobs = new Map<number, { resolve: (v: any) => void; reject: (e: any) => void; timer: NodeJS.Timeout }>();
  private testsRun = 0;
  private wasmPath: string;

  constructor(wasmPath: string) {
    this.wasmPath = wasmPath;
  }

  private async ensureWorker(): Promise<void> {
    if (this.worker && this.testsRun < WORKER_RECYCLE_AFTER) {
      return;
    }

    // Recycle worker
    if (this.worker) {
      this.worker.terminate();
      this.worker = null;
      this.testsRun = 0;
    }

    const workerPath = path.join(__dirname, 'emit-worker.js');
    this.worker = new Worker(workerPath, {
      workerData: { wasmPath: this.wasmPath },
    });

    await new Promise<void>((resolve, reject) => {
      const onMessage = (msg: any) => {
        if (msg.type === 'ready') {
          this.worker!.off('message', onMessage);
          resolve();
        } else if (msg.type === 'error') {
          reject(new Error(msg.error));
        }
      };
      this.worker!.on('message', onMessage);
      this.worker!.on('error', reject);
    });

    this.worker.on('message', (msg: any) => {
      if (msg.id !== undefined) {
        const pending = this.pendingJobs.get(msg.id);
        if (pending) {
          clearTimeout(pending.timer);
          this.pendingJobs.delete(msg.id);
          if (msg.error) {
            pending.reject(new Error(msg.error));
          } else {
            pending.resolve({ js: msg.output, dts: msg.declaration });
          }
        }
      }
    });
  }

  async transpile(source: string, target: number, module: number, declaration = false): Promise<{js: string, dts?: string | null}> {
    await this.ensureWorker();
    this.testsRun++;

    const id = this.jobId++;

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingJobs.delete(id);
        // Kill and recreate worker on timeout
        if (this.worker) {
          this.worker.terminate();
          this.worker = null;
          this.testsRun = 0;
        }
        reject(new Error('TIMEOUT'));
      }, TEST_TIMEOUT_MS);

      this.pendingJobs.set(id, { resolve, reject, timer });
      this.worker!.postMessage({ id, source, target, module, declaration });
    });
  }

  terminate(): void {
    if (this.worker) {
      for (const { timer } of this.pendingJobs.values()) {
        clearTimeout(timer);
      }
      this.pendingJobs.clear();
      this.worker.terminate();
      this.worker = null;
    }
  }
}

// ============================================================================
// Test Execution
// ============================================================================

async function runTest(worker: TranspileWorker, testCase: TestCase, config: Config): Promise<TestResult> {
  const start = Date.now();
  const testName = testCase.baselineFile.replace('.js', '');

  const result: TestResult = {
    name: testName,
    jsMatch: null,
    dtsMatch: null,
  };

  try {
    // Check cache
    loadCache();
    const cacheKey = getCacheKey(testCase.source, testCase.target, testCase.module);
    let tszJs: string;
    let tszDts: string | null = null;

    const cached = cache.get(cacheKey);
    const sourceHash = hashString(testCase.source);

    if (cached && cached.hash === sourceHash) {
      tszJs = cached.jsOutput;
      tszDts = cached.dtsOutput;
    } else {
      // Run tsz transpile via worker
      const transpileResult = await worker.transpile(testCase.source, testCase.target, testCase.module, config.dtsOnly);
      tszJs = transpileResult.js;
      tszDts = transpileResult.dts || null;
      cache.set(cacheKey, { hash: sourceHash, jsOutput: tszJs, dtsOutput: tszDts });
    }

    // Prepend "use strict" prologue when source has @strict or @alwaysStrict directive
    if (testCase.alwaysStrict && !tszJs.trimStart().startsWith('"use strict"')) {
      tszJs = '"use strict";\n' + tszJs;
    }

    // Compare JS
    if (!config.dtsOnly && testCase.expectedJs) {
      const expected = testCase.expectedJs.replace(/\r\n/g, '\n').trim();
      const actual = tszJs.replace(/\r\n/g, '\n').trim();
      result.jsMatch = expected === actual;
      
      if (!result.jsMatch) {
        if (config.verbose) {
          result.jsError = getEmitDiff(expected, actual);
        } else {
          result.jsError = getEmitDiffSummary(expected, actual);
        }
      }
    }

    // DTS comparison
    if (!config.jsOnly && testCase.expectedDts) {
      if (tszDts !== null) {
        const expected = testCase.expectedDts.replace(/\r\n/g, '\n').trim();
        const actual = tszDts.replace(/\r\n/g, '\n').trim();
        result.dtsMatch = expected === actual;
      } else {
        result.dtsMatch = null; // No DTS output generated
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
// Progress Bar
// ============================================================================

function progressBar(current: number, total: number, width: number = 30): string {
  const pct = total > 0 ? current / total : 0;
  const filled = Math.round(pct * width);
  const empty = width - filled;
  const bar = '\x1b[32m' + '█'.repeat(filled) + '\x1b[2m' + '░'.repeat(empty) + '\x1b[0m';
  return `${bar} ${(pct * 100).toFixed(1)}% | ${current.toLocaleString()}/${total.toLocaleString()}`;
}

// ============================================================================
// Main
// ============================================================================

function parseArgs(): Config {
  const args = process.argv.slice(2);
  const config: Config = {
    maxTests: Infinity,
    filter: '',
    verbose: false,
    jsOnly: false,
    dtsOnly: false,
  };

  for (const arg of args) {
    if (arg.startsWith('--max=')) {
      config.maxTests = parseInt(arg.slice(6), 10);
    } else if (arg.startsWith('--filter=')) {
      config.filter = arg.slice(9);
    } else if (arg === '--verbose' || arg === '-v') {
      config.verbose = true;
    } else if (arg === '--js-only') {
      config.jsOnly = true;
    } else if (arg === '--dts-only') {
      config.dtsOnly = true;
    } else if (arg === '--help' || arg === '-h') {
      console.log(`
TSZ Emit Test Runner

Usage: ./run.sh [options]

Options:
  --max=N           Maximum tests (default: all)
  --filter=PATTERN  Filter tests by name
  --verbose, -v     Detailed output with diffs
  --js-only         Test JavaScript emit only
  --dts-only        Test declaration emit only
  --help, -h        Show this help
`);
      process.exit(0);
    }
  }

  return config;
}

async function main() {
  const config = parseArgs();

  console.log('');
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.bold}  TSZ Emit Test Runner${colors.reset}`);
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.dim}  Max tests: ${config.maxTests === Infinity ? 'all' : config.maxTests}${colors.reset}`);
  console.log(`${colors.dim}  Timeout: ${TEST_TIMEOUT_MS}ms per test${colors.reset}`);
  if (config.filter) {
    console.log(`${colors.dim}  Filter: ${config.filter}${colors.reset}`);
  }
  console.log(`${colors.dim}  Mode: ${config.jsOnly ? 'JS only' : config.dtsOnly ? 'DTS only' : 'JS + DTS'}${colors.reset}`);
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log('');

  // Check WASM module exists
  const wasmPath = path.join(ROOT_DIR, 'pkg/wasm.js');
  if (!fs.existsSync(wasmPath)) {
    console.error('WASM module not found. Run: wasm-pack build --target nodejs --out-dir pkg');
    process.exit(1);
  }

  // Find test cases
  console.log(`${colors.dim}Discovering test cases...${colors.reset}`);
  const testCases = findTestCases(config.filter, config.maxTests);
  console.log(`${colors.dim}Found ${testCases.length} test cases${colors.reset}`);
  console.log('');

  // Create worker
  const worker = new TranspileWorker(wasmPath);

  // Run tests
  let jsPass = 0, jsFail = 0, jsSkip = 0, jsTimeout = 0;
  let dtsPass = 0, dtsFail = 0, dtsSkip = 0;
  const failures: TestResult[] = [];
  const startTime = Date.now();

  // Progress tracking
  let lastProgressLen = 0;
  function printProgress(current: number) {
    const bar = progressBar(current, testCases.length);
    const elapsed = (Date.now() - startTime) / 1000;
    const rate = current > 0 ? Math.round(current / elapsed) : 0;
    const msg = `  ${bar} | ${rate}/s`;
    process.stdout.write('\r' + msg + ' '.repeat(Math.max(0, lastProgressLen - msg.length)));
    lastProgressLen = msg.length;
  }

  for (let i = 0; i < testCases.length; i++) {
    const testCase = testCases[i];
    const result = await runTest(worker, testCase, config);

    // Count results
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

    // Progress
    if (!config.verbose) {
      printProgress(i + 1);
    } else {
      const jsStatus = result.timeout ? `${colors.yellow}T${colors.reset}` :
                       result.skipped ? `${colors.dim}S${colors.reset}` :
                       result.jsMatch === true ? `${colors.green}✓${colors.reset}` :
                       result.jsMatch === false ? `${colors.red}✗${colors.reset}` : `${colors.dim}-${colors.reset}`;
      console.log(`  [${jsStatus}] ${result.name} (${result.elapsed}ms)`);
      if (result.jsError && result.jsMatch === false) {
        console.log(result.jsError);
      }
    }
  }

  // Cleanup
  worker.terminate();
  saveCache();

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

  // Summary
  console.log('\n');
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.bold}EMIT TEST RESULTS${colors.reset}`);
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);

  if (!config.dtsOnly) {
    const jsTotal = jsPass + jsFail;
    const jsPct = jsTotal > 0 ? (jsPass / jsTotal * 100).toFixed(1) : '0.0';
    console.log(`${colors.bold}JavaScript Emit:${colors.reset}`);
    console.log(`  ${colors.green}Passed: ${jsPass}${colors.reset}`);
    console.log(`  ${colors.red}Failed: ${jsFail}${colors.reset}${jsTimeout > 0 ? ` (${jsTimeout} timeouts)` : ''}`);
    console.log(`  ${colors.dim}Skipped: ${jsSkip}${colors.reset}`);
    console.log(`  ${colors.yellow}Pass Rate: ${jsPct}% (${jsPass}/${jsTotal})${colors.reset}`);
  }

  if (!config.jsOnly && (dtsPass + dtsFail) > 0) {
    const dtsTotal = dtsPass + dtsFail;
    const dtsPct = dtsTotal > 0 ? (dtsPass / dtsTotal * 100).toFixed(1) : '0.0';
    console.log(`${colors.bold}Declaration Emit:${colors.reset}`);
    console.log(`  ${colors.green}Passed: ${dtsPass}${colors.reset}`);
    console.log(`  ${colors.red}Failed: ${dtsFail}${colors.reset}`);
    console.log(`  ${colors.dim}Skipped: ${dtsSkip}${colors.reset}`);
    console.log(`  ${colors.yellow}Pass Rate: ${dtsPct}% (${dtsPass}/${dtsTotal})${colors.reset}`);
  }

  const totalTests = testCases.length;
  const rate = totalTests > 0 ? Math.round(totalTests / parseFloat(elapsed)) : 0;
  console.log(`${colors.dim}\nTime: ${elapsed}s (${rate} tests/sec)${colors.reset}`);
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);

  // Show first failures (excluding timeouts)
  const realFailures = failures.filter(f => !f.timeout);
  if (realFailures.length > 0 && !config.verbose) {
    console.log(`\n${colors.bold}First failures:${colors.reset}`);
    for (const f of realFailures.slice(0, 10)) {
      const diffInfo = f.jsError ? ` ${colors.dim}(${f.jsError})${colors.reset}` : '';
      console.log(`  ${colors.red}✗${colors.reset} ${f.name}${diffInfo}`);
    }
    if (realFailures.length > 10) {
      console.log(`  ${colors.dim}... and ${realFailures.length - 10} more${colors.reset}`);
    }
  }

  // Show timeouts
  const timeouts = failures.filter(f => f.timeout);
  if (timeouts.length > 0 && !config.verbose) {
    console.log(`\n${colors.bold}Timeouts (${timeouts.length}):${colors.reset}`);
    for (const f of timeouts.slice(0, 5)) {
      console.log(`  ${colors.yellow}T${colors.reset} ${f.name}`);
    }
    if (timeouts.length > 5) {
      console.log(`  ${colors.dim}... and ${timeouts.length - 5} more${colors.reset}`);
    }
  }

  process.exit(jsFail > 0 || dtsFail > 0 ? 1 : 0);
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(2);
});
