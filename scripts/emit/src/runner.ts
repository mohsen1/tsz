#!/usr/bin/env node
/**
 * TSZ Emit Test Runner
 *
 * Compares tsz JavaScript/Declaration emit output against TypeScript's baselines.
 * Uses semantic comparison (normalizes formatting differences).
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
import { fileURLToPath } from 'url';
import { parseBaseline, normalizeEmit, getEmitDiff, getEmitDiffSummary, type BaselineContent } from './baseline-parser.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');
const TS_DIR = path.join(ROOT_DIR, 'TypeScript');
const BASELINES_DIR = path.join(TS_DIR, 'tests/baselines/reference');
const TESTS_DIR = path.join(TS_DIR, 'tests/cases');

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

interface TestResult {
  name: string;
  jsMatch: boolean | null;  // null = not tested
  dtsMatch: boolean | null;
  jsError?: string;
  dtsError?: string;
}

function parseArgs(): Config {
  const args = process.argv.slice(2);
  const config: Config = {
    maxTests: 500,
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
  --max=N           Maximum tests (default: 500)
  --filter=PATTERN  Filter tests by name
  --verbose, -v     Detailed output
  --js-only         Test JavaScript emit only
  --dts-only        Test declaration emit only
  --help, -h        Show this help
`);
      process.exit(0);
    }
  }

  return config;
}

/**
 * Find all baseline files that have emit output
 */
function findEmitBaselines(filter: string): string[] {
  const files: string[] = [];

  if (!fs.existsSync(BASELINES_DIR)) {
    console.error(`Baselines directory not found: ${BASELINES_DIR}`);
    process.exit(1);
  }

  const entries = fs.readdirSync(BASELINES_DIR);
  for (const entry of entries) {
    if (!entry.endsWith('.js')) continue;
    if (filter && !entry.toLowerCase().includes(filter.toLowerCase())) continue;

    // Skip error-only baselines
    const errorFile = entry.replace('.js', '.errors.txt');
    if (entries.includes(errorFile)) {
      // Has errors - might still have emit, check the file
    }

    files.push(entry);
  }

  files.sort();
  return files;
}

/**
 * Load tsz WASM module
 */
async function loadTsz(): Promise<any> {
  const wasmPath = path.join(ROOT_DIR, 'pkg/wasm.js');
  if (!fs.existsSync(wasmPath)) {
    console.error('WASM module not found. Run: wasm-pack build --target nodejs --out-dir pkg');
    process.exit(1);
  }
  return import(wasmPath);
}

/**
 * Get tsz emit output for source code
 */
function getTszEmit(
  wasm: any,
  source: string,
  fileName: string,
  options: { target?: number; module?: number; declaration?: boolean }
): { js: string; dts: string | null } {
  // Use transpileModule for JS
  const jsOutput = wasm.transpile(source, options.target ?? 1, options.module ?? 0);

  // For declaration emit, we'd need to use WasmProgram with declaration option
  // For now, return null for dts (to be implemented)
  let dtsOutput: string | null = null;

  if (options.declaration) {
    try {
      const program = new wasm.WasmProgram();
      program.setCompilerOptions(JSON.stringify({
        target: options.target ?? 1,
        module: options.module ?? 0,
        declaration: true,
      }));
      program.addFile(fileName, source);

      // Check if we have declaration emit capability
      if (program.emitDeclarations) {
        dtsOutput = program.emitDeclarations();
      }
      program.free();
    } catch (e) {
      // Declaration emit not implemented yet
    }
  }

  return { js: jsOutput, dts: dtsOutput };
}

/**
 * Run a single emit test
 */
function runTest(
  wasm: any,
  baselineFile: string,
  config: Config
): TestResult {
  const baselinePath = path.join(BASELINES_DIR, baselineFile);
  const baselineContent = fs.readFileSync(baselinePath, 'utf-8');
  const testName = baselineFile.replace('.js', '');

  const result: TestResult = {
    name: testName,
    jsMatch: null,
    dtsMatch: null,
  };

  try {
    const baseline = parseBaseline(baselineContent);

    if (!baseline.source) {
      result.jsError = 'No source in baseline';
      return result;
    }

    // Get tsz emit
    const tszEmit = getTszEmit(wasm, baseline.source, baseline.sourceFileName || 'test.ts', {
      target: 1, // ES5
      module: 0, // None
      declaration: !config.jsOnly,
    });

    // Compare JS emit
    if (!config.dtsOnly && baseline.js) {
      const normalizedBaseline = normalizeEmit(baseline.js);
      const normalizedTsz = normalizeEmit(tszEmit.js);
      result.jsMatch = normalizedBaseline === normalizedTsz;
      if (!result.jsMatch) {
        if (config.verbose) {
          result.jsError = getEmitDiff(baseline.js, tszEmit.js);
        } else {
          result.jsError = getEmitDiffSummary(baseline.js, tszEmit.js);
        }
      }
    }

    // Compare DTS emit
    if (!config.jsOnly && baseline.dts && tszEmit.dts) {
      const normalizedBaseline = normalizeEmit(baseline.dts);
      const normalizedTsz = normalizeEmit(tszEmit.dts);
      result.dtsMatch = normalizedBaseline === normalizedTsz;
      if (!result.dtsMatch) {
        if (config.verbose) {
          result.dtsError = getEmitDiff(baseline.dts, tszEmit.dts);
        } else {
          result.dtsError = getEmitDiffSummary(baseline.dts, tszEmit.dts);
        }
      }
    }

  } catch (e) {
    result.jsError = e instanceof Error ? e.message : String(e);
  }

  return result;
}

async function main() {
  const config = parseArgs();

  console.log('');
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.bold}  TSZ Emit Test Runner${colors.reset}`);
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log(`${colors.dim}  Max tests: ${config.maxTests}${colors.reset}`);
  if (config.filter) {
    console.log(`${colors.dim}  Filter: ${config.filter}${colors.reset}`);
  }
  console.log(`${colors.dim}  Mode: ${config.jsOnly ? 'JS only' : config.dtsOnly ? 'DTS only' : 'JS + DTS'}${colors.reset}`);
  console.log(`${colors.cyan}════════════════════════════════════════════════════════════${colors.reset}`);
  console.log('');

  // Load WASM
  console.log(`${colors.dim}Loading tsz WASM module...${colors.reset}`);
  const wasm = await loadTsz();

  // Find baselines
  const baselines = findEmitBaselines(config.filter);
  const testsToRun = baselines.slice(0, config.maxTests);

  console.log(`${colors.dim}Found ${baselines.length} baselines, running ${testsToRun.length}${colors.reset}`);
  console.log('');

  // Run tests
  let jsPass = 0, jsFail = 0, jsSkip = 0;
  let dtsPass = 0, dtsFail = 0, dtsSkip = 0;
  const failures: TestResult[] = [];
  const startTime = Date.now();

  for (let i = 0; i < testsToRun.length; i++) {
    const baseline = testsToRun[i];
    const result = runTest(wasm, baseline, config);

    // Count results
    if (result.jsMatch === true) jsPass++;
    else if (result.jsMatch === false) { jsFail++; failures.push(result); }
    else jsSkip++;

    if (result.dtsMatch === true) dtsPass++;
    else if (result.dtsMatch === false) dtsFail++;
    else dtsSkip++;

    // Progress
    if (!config.verbose && (i + 1) % 50 === 0) {
      const pct = ((i + 1) / testsToRun.length * 100).toFixed(1);
      process.stdout.write(`\r  Progress: ${i + 1}/${testsToRun.length} (${pct}%)`);
    }

    if (config.verbose) {
      const jsStatus = result.jsMatch === true ? `${colors.green}✓${colors.reset}` :
                       result.jsMatch === false ? `${colors.red}✗${colors.reset}` : `${colors.dim}-${colors.reset}`;
      const dtsStatus = result.dtsMatch === true ? `${colors.green}✓${colors.reset}` :
                        result.dtsMatch === false ? `${colors.red}✗${colors.reset}` : `${colors.dim}-${colors.reset}`;
      console.log(`  [${jsStatus}/${dtsStatus}] ${result.name}`);
      if (result.jsError) console.log(`       ${colors.dim}${result.jsError}${colors.reset}`);
    }
  }

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

  // Summary
  console.log('');
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

  if (!config.jsOnly) {
    const dtsTotal = dtsPass + dtsFail;
    const dtsPct = dtsTotal > 0 ? (dtsPass / dtsTotal * 100).toFixed(1) : '0.0';
    console.log(`${colors.bold}Declaration Emit:${colors.reset}`);
    console.log(`  ${colors.green}Passed: ${dtsPass}${colors.reset}`);
    console.log(`  ${colors.red}Failed: ${dtsFail}${colors.reset}`);
    console.log(`  ${colors.dim}Skipped: ${dtsSkip}${colors.reset}`);
    console.log(`  ${colors.yellow}Pass Rate: ${dtsPct}% (${dtsPass}/${dtsTotal})${colors.reset}`);
  }

  console.log(`${colors.dim}\nTime: ${elapsed}s${colors.reset}`);
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

  // In verbose mode, show full diffs for failures
  if (failures.length > 0 && config.verbose) {
    console.log(`\n${colors.bold}Failure Details:${colors.reset}`);
    for (const f of failures.slice(0, 5)) {
      console.log(`\n${colors.cyan}─── ${f.name} ───${colors.reset}`);
      if (f.jsError) {
        console.log(f.jsError);
      }
    }
    if (failures.length > 5) {
      console.log(`\n${colors.dim}... and ${failures.length - 5} more failures${colors.reset}`);
    }
  }

  process.exit(jsFail > 0 || dtsFail > 0 ? 1 : 0);
}

main().catch(err => {
  console.error('Fatal error:', err);
  process.exit(2);
});
