#!/usr/bin/env node
/**
 * High-Performance Parallel Conformance Test Runner
 *
 * Uses persistent worker threads that load WASM once.
 * Robust crash/OOM recovery with automatic worker respawn.
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

interface TestResult {
  tscCodes: number[];
  wasmCodes: number[];
  crashed: boolean;
  timedOut: boolean;
  oom: boolean;
  category: string;
  error?: string;
  filePath?: string;
}

interface TestStats {
  total: number;
  passed: number;
  failed: number;
  crashed: number;
  skipped: number;
  timedOut: number;
  oom: number;
  byCategory: Record<string, { total: number; passed: number }>;
  missingCodes: Map<number, number>;
  extraCodes: Map<number, number>;
  crashedTests: { path: string; error: string }[];
  oomTests: string[];
  timedOutTests: string[];
  workerStats: {
    spawned: number;
    crashed: number;
    respawned: number;
  };
}

const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  magenta: '\x1b[35m',
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
  resolve: (result: TestResult) => void;
  timer: NodeJS.Timeout;
  startTime: number;
}

interface WorkerInfo {
  worker: Worker;
  id: number;
  busy: boolean;
  currentTestId: number | null;
  testsProcessed: number;
  crashCount: number;
}

class WorkerPool {
  private workers: WorkerInfo[] = [];
  private pending = new Map<number, PendingTest>();
  private nextId = 0;
  private workerPath: string;
  private workerDataBase: { wasmPkgPath: string; libPath: string };
  private timeout: number;
  private testsBasePath: string;
  private nextWorkerId = 0;
  
  // Stats
  public workersSpawned = 0;
  public workersCrashed = 0;
  public workersRespawned = 0;

  constructor(
    count: number,
    workerPath: string,
    workerData: { wasmPkgPath: string; libPath: string },
    timeout: number,
    testsBasePath: string
  ) {
    this.workerPath = workerPath;
    this.workerDataBase = workerData;
    this.timeout = timeout;
    this.testsBasePath = testsBasePath;
    for (let i = 0; i < count; i++) this.spawnWorker();
  }

  private spawnWorker(): WorkerInfo {
    const id = this.nextWorkerId++;
    this.workersSpawned++;
    
    const worker = new Worker(this.workerPath, { 
      workerData: { ...this.workerDataBase, id },
    });

    const info: WorkerInfo = {
      worker,
      id,
      busy: false,
      currentTestId: null,
      testsProcessed: 0,
      crashCount: 0,
    };
    
    worker.on('message', (msg: any) => {
      if (msg.type === 'ready') {
        // Worker initialized successfully
        return;
      }
      
      if (msg.type === 'result') {
        const pending = this.pending.get(msg.id);
        if (pending) {
          clearTimeout(pending.timer);
          this.pending.delete(msg.id);
          info.busy = false;
          info.currentTestId = null;
          info.testsProcessed++;
          
          pending.resolve({
            tscCodes: msg.tscCodes,
            wasmCodes: msg.wasmCodes,
            crashed: msg.crashed,
            timedOut: false,
            oom: msg.oom || false,
            category: msg.category,
            error: msg.error,
            filePath: pending.relPath,
          });
        }
        return;
      }
      
      if (msg.type === 'crash') {
        // Worker reported a crash but is still alive
        info.crashCount++;
        return;
      }
      
      if (msg.type === 'heartbeat') {
        // Worker sent heartbeat - it's still alive but may be slow
        return;
      }
    });

    worker.on('error', (err) => {
      this.workersCrashed++;
      info.crashCount++;
      
      // Resolve any pending test as crashed
      if (info.currentTestId !== null) {
        const pending = this.pending.get(info.currentTestId);
        if (pending) {
          clearTimeout(pending.timer);
          this.pending.delete(info.currentTestId);
          pending.resolve({
            tscCodes: [],
            wasmCodes: [],
            crashed: true,
            timedOut: false,
            oom: err.message.includes('memory') || err.message.includes('heap'),
            category: 'unknown',
            error: `Worker error: ${err.message}`,
            filePath: pending.relPath,
          });
        }
      }
      
      // Replace crashed worker
      this.replaceWorker(info);
    });

    worker.on('exit', (code: number | null, signal: NodeJS.Signals | null) => {
      if (code !== 0 && code !== null) {
        // Abnormal exit
        this.workersCrashed++;
        info.crashCount++;
        
        // Handle any pending test
        if (info.currentTestId !== null) {
          const pending = this.pending.get(info.currentTestId);
          if (pending) {
            clearTimeout(pending.timer);
            this.pending.delete(info.currentTestId);
            
            // Signal 9 (SIGKILL) often indicates OOM killer
            const isOom = signal === 'SIGKILL' || code === 137;
            
            pending.resolve({
              tscCodes: [],
              wasmCodes: [],
              crashed: true,
              timedOut: false,
              oom: isOom,
              category: 'unknown',
              error: signal ? `Worker killed by ${signal}` : `Worker exited with code ${code}`,
              filePath: pending.relPath,
            });
          }
        }
        
        // Replace dead worker
        this.replaceWorker(info);
      }
    });

    this.workers.push(info);
    return info;
  }

  private replaceWorker(oldInfo: WorkerInfo): void {
    // Remove old worker from list
    const idx = this.workers.indexOf(oldInfo);
    if (idx >= 0) {
      this.workers.splice(idx, 1);
    }
    
    // Try to terminate if not already dead
    try {
      oldInfo.worker.terminate();
    } catch {}
    
    // Spawn replacement
    this.workersRespawned++;
    this.spawnWorker();
  }

  async ready(): Promise<void> {
    // Wait for all workers to signal ready
    await new Promise<void>((resolve) => {
      const checkReady = () => {
        const allReady = this.workers.every(w => !w.busy || w.testsProcessed > 0);
        if (allReady && this.workers.length > 0) {
          resolve();
        } else {
          setTimeout(checkReady, 50);
        }
      };
      setTimeout(checkReady, 100);
    });
  }

  run(filePath: string): Promise<TestResult> {
    return new Promise(resolve => {
      const id = this.nextId++;
      const relPath = filePath.replace(this.testsBasePath + path.sep, '');

      const tryRun = () => {
        // Find available worker
        const worker = this.workers.find(w => !w.busy);
        
        if (worker) {
          worker.busy = true;
          worker.currentTestId = id;
          
          const timer = setTimeout(() => {
            // Test timed out - terminate and respawn worker
            this.pending.delete(id);
            this.workersCrashed++;
            
            resolve({
              tscCodes: [],
              wasmCodes: [],
              crashed: false,
              timedOut: true,
              oom: false,
              category: 'unknown',
              error: `Timeout after ${this.timeout}ms`,
              filePath: relPath,
            });
            
            // Replace the stuck worker
            this.replaceWorker(worker);
          }, this.timeout);

          this.pending.set(id, { 
            id, 
            filePath, 
            relPath, 
            resolve, 
            timer,
            startTime: Date.now(),
          });
          
          worker.worker.postMessage({ id, filePath, testsBasePath: this.testsBasePath });
        } else {
          // No worker available, wait and retry
          setTimeout(tryRun, 10);
        }
      };

      tryRun();
    });
  }

  getStats(): { spawned: number; crashed: number; respawned: number } {
    return {
      spawned: this.workersSpawned,
      crashed: this.workersCrashed,
      respawned: this.workersRespawned,
    };
  }

  async terminate(): Promise<void> {
    for (const info of this.workers) {
      try {
        await info.worker.terminate();
      } catch {}
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
    return { 
      total: 0, passed: 0, failed: 0, crashed: 0, skipped: 0, timedOut: 0, oom: 0,
      byCategory: {}, missingCodes: new Map(), extraCodes: new Map(),
      crashedTests: [], oomTests: [], timedOutTests: [],
      workerStats: { spawned: 0, crashed: 0, respawned: 0 },
    };
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
    oom: 0,
    byCategory: {},
    missingCodes: new Map(),
    extraCodes: new Map(),
    crashedTests: [],
    oomTests: [],
    timedOutTests: [],
    workerStats: { spawned: 0, crashed: 0, respawned: 0 },
  };

  let completed = 0;
  const updateProgress = () => {
    const pct = ((completed / allTestFiles.length) * 100).toFixed(1);
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    const rate = completed > 0 ? (completed / ((Date.now() - startTime) / 1000)).toFixed(0) : '0';
    const workerStats = pool.getStats();
    const crashInfo = workerStats.crashed > 0 ? ` | ⚠ ${workerStats.crashed} crashes` : '';
    process.stdout.write(`\r  Progress: ${completed}/${allTestFiles.length} (${pct}%) | ${rate}/s | ${elapsed}s${crashInfo}    `);
  };

  // Run all tests
  log(`\nRunning tests...`, colors.cyan);
  
  const promises = allTestFiles.map(async (filePath) => {
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
      stats.timedOutTests.push(result.filePath || filePath);
      if (cfg.verbose) log(`\n  ${result.filePath}: TIMEOUT`, colors.yellow);
      return;
    }
    
    if (result.oom) {
      stats.oom++;
      stats.crashed++;
      stats.failed++;
      stats.oomTests.push(result.filePath || filePath);
      if (cfg.verbose) log(`\n  ${result.filePath}: OOM`, colors.red);
      return;
    }
    
    if (result.crashed) {
    stats.crashed++;
    stats.failed++;
      stats.crashedTests.push({ path: result.filePath || filePath, error: result.error || 'Unknown' });
      if (cfg.verbose) log(`\n  ${result.filePath}: CRASH - ${result.error}`, colors.red);
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
        log(`\n  ${result.filePath}:`, colors.yellow);
        if (cmp.missing.length) log(`    Missing: TS${[...new Set(cmp.missing)].join(', TS')}`, colors.dim);
        if (cmp.extra.length) log(`    Extra: TS${[...new Set(cmp.extra)].join(', TS')}`, colors.dim);
      }
    }
  });

  await Promise.all(promises);
  
  stats.workerStats = pool.getStats();
  await pool.terminate();

  if (!cfg.verbose) process.stdout.write('\r' + ' '.repeat(100) + '\r');

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
  log(`  ✓ Passed:   ${stats.passed}`, colors.green);
  log(`  ✗ Failed:   ${stats.failed - stats.crashed - stats.timedOut}`, stats.failed > stats.crashed + stats.timedOut ? colors.red : colors.dim);
  log(`  💥 Crashed:  ${stats.crashed - stats.oom}`, stats.crashed - stats.oom > 0 ? colors.red : colors.dim);
  log(`  💾 OOM:      ${stats.oom}`, stats.oom > 0 ? colors.magenta : colors.dim);
  log(`  ⏱ Timeout:  ${stats.timedOut}`, stats.timedOut > 0 ? colors.yellow : colors.dim);

  // Worker stats
  log('\nWorker Health:', colors.bold);
  log(`  Spawned:   ${stats.workerStats.spawned}`, colors.dim);
  log(`  Crashed:   ${stats.workerStats.crashed}`, stats.workerStats.crashed > 0 ? colors.red : colors.dim);
  log(`  Respawned: ${stats.workerStats.respawned}`, stats.workerStats.respawned > 0 ? colors.yellow : colors.dim);

    log('\nBy Category:', colors.bold);
  for (const [cat, s] of Object.entries(stats.byCategory)) {
    const r = s.total > 0 ? ((s.passed / s.total) * 100).toFixed(1) : '0.0';
    log(`  ${cat}: ${s.passed}/${s.total} (${r}%)`, s.passed === s.total ? colors.green : colors.yellow);
  }

  // Show problematic tests
  if (stats.crashedTests.length > 0) {
    log('\nCrashed Tests:', colors.red);
    for (const t of stats.crashedTests.slice(0, 5)) {
      log(`  ${t.path}`, colors.dim);
      log(`    ${t.error.slice(0, 80)}`, colors.dim);
    }
    if (stats.crashedTests.length > 5) {
      log(`  ... and ${stats.crashedTests.length - 5} more`, colors.dim);
    }
  }

  if (stats.oomTests.length > 0) {
    log('\nOOM Tests:', colors.magenta);
    for (const t of stats.oomTests.slice(0, 5)) {
      log(`  ${t}`, colors.dim);
    }
    if (stats.oomTests.length > 5) {
      log(`  ... and ${stats.oomTests.length - 5} more`, colors.dim);
    }
  }

  if (stats.timedOutTests.length > 0) {
    log('\nTimed Out Tests:', colors.yellow);
    for (const t of stats.timedOutTests.slice(0, 5)) {
      log(`  ${t}`, colors.dim);
    }
    if (stats.timedOutTests.length > 5) {
      log(`  ... and ${stats.timedOutTests.length - 5} more`, colors.dim);
    }
  }

  // Top errors
  log('\nTop Missing Errors:', colors.bold);
  for (const [c, n] of [...stats.missingCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
    log(`  TS${c}: ${n}x`, colors.yellow);
  }
  
  log('\nTop Extra Errors:', colors.bold);
  for (const [c, n] of [...stats.extraCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
    log(`  TS${c}: ${n}x`, colors.yellow);
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
