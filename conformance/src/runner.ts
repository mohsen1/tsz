#!/usr/bin/env node
/**
 * High-Performance Parallel Conformance Test Runner
 *
 * Uses persistent worker threads that load WASM once.
 * Workers that hang are terminated and respawned.
 */

import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { Worker } from 'worker_threads';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

interface RunnerConfig {
  wasmPkgPath: string;
  testsBasePath: string;
  libPath: string;
  maxTests: number;
  verbose: boolean;
  categories: string[];
  workers: number;
  testTimeout: number;
}

const DEFAULT_CONFIG: RunnerConfig = {
  wasmPkgPath: path.resolve(__dirname, '../../pkg'),
  testsBasePath: path.resolve(__dirname, '../../TypeScript/tests/cases'),
  libPath: path.resolve(__dirname, '../../TypeScript/tests/lib/lib.d.ts'),
  maxTests: 500,
  verbose: false,
  categories: ['conformance', 'compiler'],
  workers: Math.max(1, os.cpus().length),
  testTimeout: 10000,
};

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
  function walk(d: string): void {
    if (files.length >= maxFiles) return;
    try {
      for (const entry of fs.readdirSync(d)) {
      if (files.length >= maxFiles) break;
        const p = path.join(d, entry);
        const stat = fs.statSync(p);
        if (stat.isDirectory()) walk(p);
        else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) files.push(p);
      }
    } catch {}
  }
  walk(dir);
  return files;
}

function compareResults(tsc: number[], wasm: number[]): { match: boolean; missing: number[]; extra: number[] } {
  const tscMap = new Map<number, number>();
  const wasmMap = new Map<number, number>();
  for (const c of tsc) tscMap.set(c, (tscMap.get(c) || 0) + 1);
  for (const c of wasm) wasmMap.set(c, (wasmMap.get(c) || 0) + 1);

  const missing: number[] = [];
  const extra: number[] = [];
  for (const [c, n] of tscMap) for (let i = 0; i < n - (wasmMap.get(c) || 0); i++) missing.push(c);
  for (const [c, n] of wasmMap) for (let i = 0; i < n - (tscMap.get(c) || 0); i++) extra.push(c);

  return { match: missing.length === 0 && extra.length === 0, missing, extra };
}

interface PendingTest {
  id: number;
  filePath: string;
  relPath: string;
  resolve: (result: { tscCodes: number[]; wasmCodes: number[]; crashed: boolean; timedOut: boolean; category: string; error?: string }) => void;
  timer: NodeJS.Timeout;
}

class WorkerPool {
  private workers: Worker[] = [];
  private available: Worker[] = [];
  private pending = new Map<number, PendingTest>();
  private nextId = 0;
  private workerPath: string;
  private workerData: { wasmPkgPath: string; libPath: string };
  private timeout: number;
  private testsBasePath: string;

  constructor(
    count: number,
    workerPath: string,
    workerData: { wasmPkgPath: string; libPath: string },
    timeout: number,
    testsBasePath: string
  ) {
    this.workerPath = workerPath;
    this.workerData = workerData;
    this.timeout = timeout;
    this.testsBasePath = testsBasePath;
    for (let i = 0; i < count; i++) this.spawnWorker();
  }

  private spawnWorker(): Worker {
    const worker = new Worker(this.workerPath, { workerData: this.workerData });
    
    worker.on('message', (msg: any) => {
      if (msg.type === 'ready') {
        this.available.push(worker);
        return;
      }
      if (msg.type === 'result') {
        const pending = this.pending.get(msg.id);
        if (pending) {
          clearTimeout(pending.timer);
          this.pending.delete(msg.id);
          this.available.push(worker);
          pending.resolve({
            tscCodes: msg.tscCodes,
            wasmCodes: msg.wasmCodes,
            crashed: msg.crashed,
            timedOut: false,
            category: msg.category,
            error: msg.error,
          });
        }
      }
    });

    worker.on('error', (err) => {
      // Worker crashed - find any pending test and resolve as crashed
      for (const [id, pending] of this.pending) {
        clearTimeout(pending.timer);
        this.pending.delete(id);
        pending.resolve({
          tscCodes: [],
          wasmCodes: [],
          crashed: true,
          timedOut: false,
          category: 'unknown',
          error: `Worker error: ${err.message}`,
        });
      }
      // Remove from workers array and spawn replacement
      const idx = this.workers.indexOf(worker);
      if (idx >= 0) this.workers.splice(idx, 1);
      this.workers.push(this.spawnWorker());
    });

    worker.on('exit', (code) => {
      if (code !== 0) {
        // Unexpected exit - spawn replacement
        const idx = this.workers.indexOf(worker);
        if (idx >= 0) this.workers.splice(idx, 1);
        this.workers.push(this.spawnWorker());
      }
    });

    this.workers.push(worker);
    return worker;
  }

  async ready(): Promise<void> {
    // Wait for all workers to be ready
    while (this.available.length < this.workers.length) {
      await new Promise(r => setTimeout(r, 10));
    }
  }

  run(filePath: string): Promise<{ tscCodes: number[]; wasmCodes: number[]; crashed: boolean; timedOut: boolean; category: string; error?: string }> {
    return new Promise(resolve => {
      const id = this.nextId++;
      const relPath = filePath.replace(this.testsBasePath + path.sep, '');

      const tryRun = () => {
        if (this.available.length > 0) {
          const worker = this.available.pop()!;
          
          const timer = setTimeout(() => {
            // Test timed out - terminate worker and spawn new one
            this.pending.delete(id);
            const idx = this.workers.indexOf(worker);
            if (idx >= 0) this.workers.splice(idx, 1);
            worker.terminate();
            this.workers.push(this.spawnWorker());
            
            resolve({
              tscCodes: [],
              wasmCodes: [],
              crashed: false,
              timedOut: true,
              category: 'unknown',
              error: `Timeout after ${this.timeout}ms`,
            });
          }, this.timeout);

          this.pending.set(id, { id, filePath, relPath, resolve, timer });
          worker.postMessage({ id, filePath, testsBasePath: this.testsBasePath });
        } else {
          // No worker available, wait and retry
          setTimeout(tryRun, 5);
        }
      };

      tryRun();
    });
  }

  async terminate(): Promise<void> {
    for (const worker of this.workers) {
      await worker.terminate();
    }
  }
}

export async function runConformanceTests(config: Partial<RunnerConfig> = {}): Promise<TestStats> {
  const cfg: RunnerConfig = { ...DEFAULT_CONFIG, ...config };
  const startTime = Date.now();

  log('╔══════════════════════════════════════════════════════════╗', colors.cyan);
  log('║    High-Performance Parallel Conformance Test Runner     ║', colors.cyan);
  log('╚══════════════════════════════════════════════════════════╝', colors.cyan);

  // Collect test files
  log(`\nCollecting test files...`, colors.cyan);
  const allTestFiles: string[] = [];
  const perCat = Math.ceil(cfg.maxTests / cfg.categories.length);

  for (const cat of cfg.categories) {
    const dir = path.join(cfg.testsBasePath, cat);
    if (fs.existsSync(dir)) {
      const remaining = cfg.maxTests - allTestFiles.length;
      const files = collectTestFiles(dir, Math.min(perCat, remaining));
      allTestFiles.push(...files);
      log(`  ${cat}: ${files.length} files`, colors.dim);
    }
  }

  log(`  Total: ${allTestFiles.length} test files`, colors.cyan);
  log(`  Workers: ${cfg.workers} | Timeout: ${cfg.testTimeout}ms`, colors.dim);

  if (allTestFiles.length === 0) {
    log('\nNo test files found!', colors.yellow);
    return { total: 0, passed: 0, failed: 0, crashed: 0, skipped: 0, timedOut: 0, byCategory: {}, missingCodes: new Map(), extraCodes: new Map() };
  }

  // Create worker pool
  const workerPath = path.join(__dirname, 'worker.js');
  const pool = new WorkerPool(
    cfg.workers,
    workerPath,
    { wasmPkgPath: cfg.wasmPkgPath, libPath: cfg.libPath },
    cfg.testTimeout,
    cfg.testsBasePath
  );

  log(`\nInitializing ${cfg.workers} workers...`, colors.cyan);
  await pool.ready();
  log(`  Workers ready!`, colors.green);

  // Stats
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
    const rate = completed > 0 ? (completed / ((Date.now() - startTime) / 1000)).toFixed(0) : '0';
    process.stdout.write(`\r  Progress: ${completed}/${allTestFiles.length} (${pct}%) | ${rate} tests/sec | ${elapsed}s    `);
  };

  // Run all tests
  log(`\nRunning tests...`, colors.cyan);

  const promises = allTestFiles.map(async (filePath) => {
    const relPath = filePath.replace(cfg.testsBasePath + path.sep, '');
    const result = await pool.run(filePath);
    
    completed++;
    if (!cfg.verbose) updateProgress();

    // Update stats
    const cat = result.category;
    if (!stats.byCategory[cat]) stats.byCategory[cat] = { total: 0, passed: 0 };
    stats.byCategory[cat].total++;

    if (result.timedOut) {
      stats.timedOut++;
      stats.failed++;
      if (cfg.verbose) log(`\n  ${relPath}: TIMEOUT`, colors.red);
      return;
    }
    if (result.crashed) {
      stats.crashed++;
      stats.failed++;
      if (cfg.verbose) log(`\n  ${relPath}: CRASH - ${result.error}`, colors.red);
      return;
    }

    const cmp = compareResults(result.tscCodes, result.wasmCodes);
    if (cmp.match) {
      stats.passed++;
      stats.byCategory[cat].passed++;
    } else {
      stats.failed++;
      for (const c of cmp.missing) stats.missingCodes.set(c, (stats.missingCodes.get(c) || 0) + 1);
      for (const c of cmp.extra) stats.extraCodes.set(c, (stats.extraCodes.get(c) || 0) + 1);
      if (cfg.verbose) {
        log(`\n  ${relPath}:`, colors.yellow);
        if (cmp.missing.length) log(`    Missing: TS${[...new Set(cmp.missing)].join(', TS')}`, colors.dim);
        if (cmp.extra.length) log(`    Extra: TS${[...new Set(cmp.extra)].join(', TS')}`, colors.dim);
      }
    }
  });

  await Promise.all(promises);
  await pool.terminate();

  if (!cfg.verbose) process.stdout.write('\r' + ' '.repeat(80) + '\r');

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
  const rate = (allTestFiles.length / ((Date.now() - startTime) / 1000)).toFixed(0);

  // Results
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
  for (const [cat, s] of Object.entries(stats.byCategory)) {
    const r = s.total > 0 ? ((s.passed / s.total) * 100).toFixed(1) : '0.0';
    log(`  ${cat}: ${s.passed}/${s.total} (${r}%)`, s.passed === s.total ? colors.green : colors.yellow);
  }

  if (cfg.verbose) {
    log('\nTop Missing:', colors.bold);
    for (const [c, n] of [...stats.missingCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10)) {
      log(`  TS${c}: ${n}x`, colors.yellow);
    }
    log('\nTop Extra:', colors.bold);
    for (const [c, n] of [...stats.extraCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10)) {
      log(`  TS${c}: ${n}x`, colors.yellow);
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
    else if (arg.startsWith('--timeout=')) config.testTimeout = parseInt(arg.split('=')[1], 10);
    else if (arg === '--verbose' || arg === '-v') config.verbose = true;
    else if (arg.startsWith('--category=')) config.categories = arg.split('=')[1].split(',');
  }

  runConformanceTests(config).then(stats => {
    process.exit(stats.failed > 0 ? 1 : 0);
  });
}
