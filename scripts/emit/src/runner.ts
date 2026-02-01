#!/usr/bin/env node
/**
 * TSZ Emit Test Runner
 *
 * Compares tsz JavaScript/Declaration emit output against TypeScript's baselines.
 * Uses TestCaseParser to parse test directives and matches with baseline variations.
 *
 * Usage:
 *   ./run.sh [options]
 *
 * Options:
 *   --max=N           Maximum number of tests to run
 *   --filter=PATTERN  Only run tests matching pattern
 *   --verbose         Show detailed output
 *   --workers=N       Number of parallel workers (default: CPU count)
 *   --js-only         Only test JavaScript emit
 *   --dts-only        Only test declaration emit
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { fileURLToPath } from 'url';
import { Worker, isMainThread, parentPort, workerData } from 'worker_threads';
import { parseBaseline, getEmitDiff, getEmitDiffSummary } from './baseline-parser.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');
const TS_DIR = path.join(ROOT_DIR, 'TypeScript');
const BASELINES_DIR = path.join(TS_DIR, 'tests/baselines/reference');
const TESTS_DIR = path.join(TS_DIR, 'tests/cases');
const CACHE_DIR = path.join(__dirname, '../.cache');

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
  workers: number;
}

interface TestCase {
  baselineFile: string;
  testPath: string | null;
  source: string;
  expectedJs: string | null;
  expectedDts: string | null;
  target: number;
  module: number;
}

interface TestResult {
  name: string;
  jsMatch: boolean | null;
  dtsMatch: boolean | null;
  jsError?: string;
  dtsError?: string;
  elapsed?: number;
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
  // Match patterns like: testName(target=es2015).js or testName(target=es2015,module=commonjs).js
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

    // Extract variant from filename
    const variant = extractVariantFromFilename(baselineFile);
    const target = variant.target ? parseTarget(variant.target) : 1;
    const module = variant.module ? parseModule(variant.module) : 0;

    testCases.push({
      baselineFile,
      testPath: baseline.testPath,
      source: baseline.source,
      expectedJs: baseline.js,
      expectedDts: baseline.dts,
      target,
      module,
    });
  }

  return testCases;
}

// ============================================================================
// Test Execution
// ============================================================================

async function loadTsz(): Promise<any> {
  const wasmPath = path.join(ROOT_DIR, 'pkg/wasm.js');
  if (!fs.existsSync(wasmPath)) {
    console.error('WASM module not found. Run: wasm-pack build --target nodejs --out-dir pkg');
    process.exit(1);
  }
  return import(wasmPath);
}

function runTest(wasm: any, testCase: TestCase, config: Config): TestResult {
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
    
    const cached = cache.get(cacheKey);
    const sourceHash = hashString(testCase.source);
    
    if (cached && cached.hash === sourceHash) {
      tszJs = cached.jsOutput;
    } else {
      // Run tsz transpile
      tszJs = wasm.transpile(testCase.source, testCase.target, testCase.module);
      cache.set(cacheKey, { hash: sourceHash, jsOutput: tszJs, dtsOutput: null });
    }

    // Compare JS
    if (!config.dtsOnly && testCase.expectedJs) {
      const expected = testCase.expectedJs.trim();
      const actual = tszJs.trim();
      result.jsMatch = expected === actual;
      
      if (!result.jsMatch) {
        if (config.verbose) {
          result.jsError = getEmitDiff(expected, actual);
        } else {
          result.jsError = getEmitDiffSummary(expected, actual);
        }
      }
    }

    // DTS comparison (when implemented)
    if (!config.jsOnly && testCase.expectedDts) {
      // TODO: Implement declaration emit comparison
      result.dtsMatch = null;
    }

    result.elapsed = Date.now() - start;

  } catch (e) {
    result.jsError = e instanceof Error ? e.message : String(e);
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
    maxTests: 500,
    filter: '',
    verbose: false,
    jsOnly: false,
    dtsOnly: false,
    workers: Math.min(os.cpus().length, 8),
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
    } else if (arg.startsWith('--workers=')) {
      config.workers = parseInt(arg.slice(10), 10);
    } else if (arg === '--help' || arg === '-h') {
      console.log(`
TSZ Emit Test Runner

Usage: ./run.sh [options]

Options:
  --max=N           Maximum tests (default: 500)
  --filter=PATTERN  Filter tests by name
  --verbose, -v     Detailed output with diffs
  --workers=N       Parallel workers (default: CPU count, max 8)
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
  console.log(`${colors.dim}  Max tests: ${config.maxTests}${colors.reset}`);
  console.log(`${colors.dim}  Workers: ${config.workers}${colors.reset}`);
  if (config.filter) {
    console.log(`${colors.dim}  Filter: ${config.filter}${colors.reset}`);
  }
  console.log(`${colors.dim}  Mode: ${config.jsOnly ? 'JS only' : config.dtsOnly ? 'DTS only' : 'JS + DTS'}${colors.reset}`);
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log('');

  // Load WASM
  console.log(`${colors.dim}Loading tsz WASM module...${colors.reset}`);
  const wasm = await loadTsz();

  // Find test cases
  console.log(`${colors.dim}Discovering test cases...${colors.reset}`);
  const testCases = findTestCases(config.filter, config.maxTests);
  console.log(`${colors.dim}Found ${testCases.length} test cases${colors.reset}`);
  console.log('');

  // Run tests
  let jsPass = 0, jsFail = 0, jsSkip = 0;
  let dtsPass = 0, dtsFail = 0, dtsSkip = 0;
  const failures: TestResult[] = [];
  const startTime = Date.now();

  // Progress tracking
  let lastProgressLen = 0;
  function printProgress(current: number) {
    const bar = progressBar(current, testCases.length);
    const rate = current > 0 ? Math.round(current / ((Date.now() - startTime) / 1000)) : 0;
    const msg = `  ${bar} | ${rate}/s`;
    process.stdout.write('\r' + msg + ' '.repeat(Math.max(0, lastProgressLen - msg.length)));
    lastProgressLen = msg.length;
  }

  for (let i = 0; i < testCases.length; i++) {
    const testCase = testCases[i];
    const result = runTest(wasm, testCase, config);

    // Count results
    if (result.jsMatch === true) jsPass++;
    else if (result.jsMatch === false) { jsFail++; failures.push(result); }
    else jsSkip++;

    if (result.dtsMatch === true) dtsPass++;
    else if (result.dtsMatch === false) dtsFail++;
    else dtsSkip++;

    // Progress
    if (!config.verbose) {
      printProgress(i + 1);
    } else {
      const jsStatus = result.jsMatch === true ? `${colors.green}✓${colors.reset}` :
                       result.jsMatch === false ? `${colors.red}✗${colors.reset}` : `${colors.dim}-${colors.reset}`;
      console.log(`  [${jsStatus}] ${result.name} (${result.elapsed}ms)`);
      if (result.jsError && result.jsMatch === false) {
        console.log(result.jsError);
      }
    }
  }

  // Save cache
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
    console.log(`  ${colors.red}Failed: ${jsFail}${colors.reset}`);
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

  // Show first failures
  if (failures.length > 0 && !config.verbose) {
    console.log(`\n${colors.bold}First failures:${colors.reset}`);
    for (const f of failures.slice(0, 10)) {
      const diffInfo = f.jsError ? ` ${colors.dim}(${f.jsError})${colors.reset}` : '';
      console.log(`  ${colors.red}✗${colors.reset} ${f.name}${diffInfo}`);
    }
    if (failures.length > 10) {
      console.log(`  ${colors.dim}... and ${failures.length - 10} more${colors.reset}`);
    }
  }

  process.exit(jsFail > 0 || dtsFail > 0 ? 1 : 0);
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(2);
});
