#!/usr/bin/env node
/**
 * Parallel Conformance Test Runner
 *
 * Runs TypeScript conformance tests using worker threads for high parallelism.
 */

import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { Worker } from 'worker_threads';
import { fileURLToPath } from 'url';

// Configuration
interface RunnerConfig {
  wasmPkgPath: string;
  testsBasePath: string;
  libPath: string;
  maxTests: number;
  verbose: boolean;
  categories: string[];
  workers: number;
  testTimeout: number;  // Per-test timeout in ms
}

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const DEFAULT_CONFIG: RunnerConfig = {
  wasmPkgPath: path.resolve(__dirname, '../../pkg'),
  testsBasePath: path.resolve(__dirname, '../../TypeScript/tests/cases'),
  libPath: path.resolve(__dirname, '../../TypeScript/tests/lib/lib.d.ts'),
  maxTests: 500,
  verbose: false,
  categories: ['conformance', 'compiler'],
  workers: Math.max(1, os.cpus().length - 1),
  testTimeout: 5000,  // 5 seconds per test
};

interface WorkerResult {
  filePath: string;
  relPath: string;
  category: string;
  tscCodes: number[];
  wasmCodes: number[];
  crashed: boolean;
  error?: string;
  skipped: boolean;
  timedOut?: boolean;
}

interface TestStats {
  total: number;
  passed: number;
  failed: number;
  crashed: number;
  skipped: number;
  timedOut: number;
  byCategory: Record<string, { total: number; passed: number }>;
  missingCodes: Map<number, number>;
  extraCodes: Map<number, number>;
}

const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
};

function log(msg: string, color = ''): void {
  console.log(`${color}${msg}${colors.reset}`);
}

function collectTestFiles(dir: string, maxFiles: number): string[] {
  const files: string[] = [];
  function walk(currentDir: string): void {
    if (files.length >= maxFiles) return;
    let entries: string[];
    try { entries = fs.readdirSync(currentDir); } catch { return; }
    for (const entry of entries) {
      if (files.length >= maxFiles) break;
      const fullPath = path.join(currentDir, entry);
      let stat: fs.Stats;
      try { stat = fs.statSync(fullPath); } catch { continue; }
      if (stat.isDirectory()) walk(fullPath);
      else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) files.push(fullPath);
    }
  }
  walk(dir);
  return files;
}

function compareResults(tscCodes: number[], wasmCodes: number[]): { exactMatch: boolean; missing: number[]; extra: number[] } {
  const tscSet = new Map<number, number>();
  const wasmSet = new Map<number, number>();
  for (const c of tscCodes) tscSet.set(c, (tscSet.get(c) || 0) + 1);
  for (const c of wasmCodes) wasmSet.set(c, (wasmSet.get(c) || 0) + 1);

  const missing: number[] = [];
  const extra: number[] = [];

  for (const [code, count] of tscSet) {
    const wasmCount = wasmSet.get(code) || 0;
    for (let i = 0; i < count - wasmCount; i++) missing.push(code);
  }
  for (const [code, count] of wasmSet) {
    const tscCount = tscSet.get(code) || 0;
    for (let i = 0; i < count - tscCount; i++) extra.push(code);
  }

  return { exactMatch: missing.length === 0 && extra.length === 0, missing, extra };
}

class WorkerPool {
  private workers: Worker[] = [];
  private available: Worker[] = [];
  private pending: Map<Worker, (result: WorkerResult) => void> = new Map();
  private jobQueue: Array<{ job: unknown; resolve: (result: WorkerResult) => void }> = [];
  private readyCount = 0;
  private readyPromise: Promise<void>;
  private readyResolve!: () => void;

  constructor(private workerPath: string, private workerData: unknown, count: number) {
    this.readyPromise = new Promise(resolve => { this.readyResolve = resolve; });
    for (let i = 0; i < count; i++) {
      const worker = new Worker(workerPath, { workerData });
      worker.on('message', (msg) => this.handleMessage(worker, msg));
      worker.on('error', (err) => console.error('Worker error:', err));
      this.workers.push(worker);
    }
  }

  private handleMessage(worker: Worker, msg: unknown): void {
    if ((msg as { ready?: boolean }).ready) {
      this.readyCount++;
      this.available.push(worker);
      if (this.readyCount === this.workers.length) this.readyResolve();
      this.processQueue();
      return;
    }

    const resolve = this.pending.get(worker);
    if (resolve) {
      this.pending.delete(worker);
      this.available.push(worker);
      resolve(msg as WorkerResult);
      this.processQueue();
    }
  }

  private processQueue(): void {
    while (this.available.length > 0 && this.jobQueue.length > 0) {
      const worker = this.available.pop()!;
      const { job, resolve } = this.jobQueue.shift()!;
      this.pending.set(worker, resolve);
      worker.postMessage(job);
    }
  }

  async ready(): Promise<void> {
    return this.readyPromise;
  }

  async run(job: unknown): Promise<WorkerResult> {
    return new Promise(resolve => {
      if (this.available.length > 0) {
        const worker = this.available.pop()!;
        this.pending.set(worker, resolve);
        worker.postMessage(job);
      } else {
        this.jobQueue.push({ job, resolve });
      }
    });
  }

  async terminate(): Promise<void> {
    await Promise.all(this.workers.map(w => w.terminate()));
  }
}

export async function runConformanceTests(config: Partial<RunnerConfig> = {}): Promise<TestStats> {
  const cfg: RunnerConfig = { ...DEFAULT_CONFIG, ...config };
  const startTime = Date.now();

  log('╔══════════════════════════════════════════════════════════╗', colors.cyan);
  log('║       Parallel Conformance Test Runner                   ║', colors.cyan);
  log('╚══════════════════════════════════════════════════════════╝', colors.cyan);

  // Load lib.d.ts
  let libSource = '';
  try {
    libSource = fs.readFileSync(cfg.libPath, 'utf8');
    log(`  Loaded lib.d.ts (${(libSource.length / 1024).toFixed(1)}KB)`, colors.dim);
  } catch {
    log('  Warning: Could not load lib.d.ts', colors.yellow);
  }

  // Collect test files
  log(`\nCollecting test files...`, colors.cyan);
  const allTestFiles: string[] = [];
  const testsPerCategory = Math.ceil(cfg.maxTests / cfg.categories.length);

  for (const category of cfg.categories) {
    const categoryDir = path.join(cfg.testsBasePath, category);
    if (fs.existsSync(categoryDir)) {
      const remaining = cfg.maxTests - allTestFiles.length;
      const limit = Math.min(testsPerCategory, remaining);
      const files = collectTestFiles(categoryDir, limit);
      allTestFiles.push(...files);
      log(`  ${category}: ${files.length} files`, colors.dim);
    }
  }

  log(`  Total: ${allTestFiles.length} test files`, colors.cyan);
  log(`  Workers: ${cfg.workers}`, colors.dim);

  if (allTestFiles.length === 0) {
    log('\nNo test files found!', colors.yellow);
    return { total: 0, passed: 0, failed: 0, crashed: 0, skipped: 0, timedOut: 0, byCategory: {}, missingCodes: new Map(), extraCodes: new Map() };
  }

  // Create worker pool
  const workerPath = path.join(__dirname, 'worker.js');
  const pool = new WorkerPool(workerPath, { wasmPkgPath: cfg.wasmPkgPath }, cfg.workers);

  log(`\nInitializing ${cfg.workers} workers...`, colors.cyan);
  await pool.ready();
  log(`  Workers ready!`, colors.green);

  // Run tests in parallel
  log(`\nRunning tests (${cfg.testTimeout}ms timeout per test)...`, colors.cyan);
  const stats: TestStats = {
    total: allTestFiles.length,
    passed: 0,
    failed: 0,
    crashed: 0,
    skipped: 0,
    timedOut: 0,
    byCategory: {},
    missingCodes: new Map(),
    extraCodes: new Map(),
  };

  let completed = 0;
  const updateProgress = () => {
    const pct = ((completed / allTestFiles.length) * 100).toFixed(1);
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    const rate = (completed / ((Date.now() - startTime) / 1000)).toFixed(0);
    process.stdout.write(`\r  Progress: ${completed}/${allTestFiles.length} (${pct}%) | ${rate} tests/sec | ${elapsed}s elapsed    `);
  };

  // Submit all jobs
  const promises = allTestFiles.map(async (filePath) => {
    const result = await pool.run({
      filePath,
      libSource,
      testsBasePath: cfg.testsBasePath,
      timeout: cfg.testTimeout,
    });

    completed++;
    if (!cfg.verbose) updateProgress();

    // Update stats
    if (!stats.byCategory[result.category]) {
      stats.byCategory[result.category] = { total: 0, passed: 0 };
    }
    stats.byCategory[result.category].total++;

    if (result.timedOut) {
      stats.timedOut++;
      stats.failed++;
      if (cfg.verbose) {
        log(`\n  ${result.relPath}: TIMEOUT`, colors.red);
      }
      return;
    }
    if (result.skipped) {
      stats.skipped++;
      return;
    }
    if (result.crashed) {
      stats.crashed++;
      stats.failed++;
      return;
    }

    const comparison = compareResults(result.tscCodes, result.wasmCodes);
    if (comparison.exactMatch) {
      stats.passed++;
      stats.byCategory[result.category].passed++;
    } else {
      stats.failed++;
      for (const code of comparison.missing) {
        stats.missingCodes.set(code, (stats.missingCodes.get(code) || 0) + 1);
      }
      for (const code of comparison.extra) {
        stats.extraCodes.set(code, (stats.extraCodes.get(code) || 0) + 1);
      }

      if (cfg.verbose) {
        log(`\n  ${result.relPath}:`, colors.yellow);
        if (comparison.missing.length > 0) log(`    Missing: TS${[...new Set(comparison.missing)].join(', TS')}`, colors.dim);
        if (comparison.extra.length > 0) log(`    Extra: TS${[...new Set(comparison.extra)].join(', TS')}`, colors.dim);
      }
    }
  });

  await Promise.all(promises);
  await pool.terminate();

  // Clear progress line
  if (!cfg.verbose) {
    process.stdout.write('\r' + ' '.repeat(80) + '\r');
  }

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
  const rate = (allTestFiles.length / ((Date.now() - startTime) / 1000)).toFixed(0);

  // Print results
  log('\n' + '═'.repeat(60), colors.dim);
  log('CONFORMANCE TEST RESULTS', colors.bold);
  log('═'.repeat(60), colors.dim);

  const passRate = stats.total > 0 ? ((stats.passed / stats.total) * 100).toFixed(1) : '0.0';
  log(`\nPass Rate: ${passRate}% (${stats.passed}/${stats.total})`, stats.passed === stats.total ? colors.green : colors.yellow);
  log(`Time: ${elapsed}s (${rate} tests/sec)`, colors.dim);

  log('\nSummary:', colors.bold);
  log(`  Passed:   ${stats.passed}`, colors.green);
  log(`  Failed:   ${stats.failed}`, stats.failed > 0 ? colors.red : colors.dim);
  log(`  Crashed:  ${stats.crashed}`, stats.crashed > 0 ? colors.red : colors.dim);
  log(`  Timeout:  ${stats.timedOut}`, stats.timedOut > 0 ? colors.yellow : colors.dim);
  log(`  Skipped:  ${stats.skipped}`, colors.dim);

  log('\nBy Category:', colors.bold);
  for (const [cat, catStats] of Object.entries(stats.byCategory)) {
    const catRate = catStats.total > 0 ? ((catStats.passed / catStats.total) * 100).toFixed(1) : '0.0';
    log(`  ${cat}: ${catStats.passed}/${catStats.total} (${catRate}%)`,
      catStats.passed === catStats.total ? colors.green : colors.yellow);
  }

  if (cfg.verbose) {
    log('\nTop Missing Errors:', colors.bold);
    const sortedMissing = [...stats.missingCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);
    for (const [code, count] of sortedMissing) {
      log(`  TS${code}: ${count}x`, colors.yellow);
    }

    log('\nTop Extra Errors:', colors.bold);
    const sortedExtra = [...stats.extraCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);
    for (const [code, count] of sortedExtra) {
      log(`  TS${code}: ${count}x`, colors.yellow);
    }
  }

  log('\n' + '═'.repeat(60), colors.dim);

  return stats;
}

// CLI
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const config: Partial<RunnerConfig> = {};

  for (const arg of args) {
    if (arg.startsWith('--max=')) config.maxTests = parseInt(arg.split('=')[1], 10);
    else if (arg.startsWith('--workers=')) config.workers = parseInt(arg.split('=')[1], 10);
    else if (arg === '--verbose' || arg === '-v') config.verbose = true;
    else if (arg.startsWith('--category=')) config.categories = arg.split('=')[1].split(',');
  }

  runConformanceTests(config).then(stats => {
    process.exit(stats.failed > 0 ? 1 : 0);
  });
}
