#!/usr/bin/env node
/**
 * TSC Cache Generator
 *
 * Pre-computes TypeScript compiler results for all conformance tests.
 * Results are cached keyed by the TypeScript submodule SHA.
 *
 * Usage:
 *   node generate-cache.js [options]
 *
 * Options:
 *   --status    Show cache status
 *   --clear     Clear existing cache
 *   --workers=N Number of parallel workers (default: CPU count)
 */

import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { Worker } from 'worker_threads';
import { fileURLToPath } from 'url';
import {
  getCacheStatus,
  saveTscCache,
  clearTscCache,
  hashContent,
  getTypeScriptSha,
  getTypescriptNpmVersion,
  checkTypescriptVersion,
  type CacheEntry,
} from './tsc-cache.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../..');
const TESTS_BASE_PATH = path.join(ROOT_DIR, 'TypeScript/tests/cases');

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
  console.log(color + msg + colors.reset);
}

function collectTestFiles(dir: string): string[] {
  const files: string[] = [];
  function walk(d: string): void {
    try {
      for (const entry of fs.readdirSync(d)) {
        const p = path.join(d, entry);
        const stat = fs.statSync(p);
        if (stat.isDirectory()) walk(p);
        else if ((entry.endsWith('.ts') || entry.endsWith('.tsx')) && !entry.endsWith('.d.ts')) files.push(p);
      }
    } catch {}
  }
  walk(dir);
  return files;
}

async function showStatus(): Promise<void> {
  const status = getCacheStatus(ROOT_DIR);
  const tsVersion = checkTypescriptVersion(ROOT_DIR);

  log('\n' + '='.repeat(60), colors.cyan);
  log('              TSC Cache Status', colors.cyan);
  log('='.repeat(60), colors.cyan);

  log('\nTypeScript Submodule SHA:', colors.bold);
  log('  Current:  ' + (status.currentSha?.slice(0, 12) || 'unknown'), colors.dim);
  log('  Cached:   ' + (status.cachedSha?.slice(0, 12) || 'none'), colors.dim);

  log('\nTypeScript npm Version:', colors.bold);
  log('  Required:  ' + tsVersion.required, colors.dim);
  log('  Installed: ' + (tsVersion.installed || 'not installed'), tsVersion.matches ? colors.dim : colors.yellow);

  if (status.valid) {
    log('\nStatus: ' + colors.green + 'VALID' + colors.reset, colors.bold);
    log('  Tests cached: ' + status.testCount, colors.dim);
    log('  Generated:    ' + status.generatedAt, colors.dim);
  } else if (status.cachedSha) {
    log('\nStatus: ' + colors.yellow + 'STALE' + colors.reset + ' (TypeScript updated)', colors.bold);
    log("  Run 'npm run cache:generate' to regenerate", colors.dim);
  } else {
    log('\nStatus: ' + colors.yellow + 'NO CACHE' + colors.reset, colors.bold);
    log("  Run 'npm run cache:generate' to create", colors.dim);
  }

  log('\nCache file: ' + status.cacheFile, colors.dim);
}

async function generateCache(workerCount: number): Promise<void> {
  const startTime = Date.now();
  const tsSha = getTypeScriptSha(ROOT_DIR);
  const tsVersion = checkTypescriptVersion(ROOT_DIR);

  log('\n' + '='.repeat(60), colors.cyan);
  log('           TSC Cache Generator', colors.cyan);
  log('='.repeat(60), colors.cyan);

  log('\nTypeScript Submodule: ' + (tsSha?.slice(0, 12) || 'unknown'), colors.dim);
  log('TypeScript npm:       ' + tsVersion.required + ' (installed: ' + (tsVersion.installed || 'none') + ')', colors.dim);
  log('Workers:              ' + workerCount, colors.dim);

  // Collect all test files
  log('\nCollecting test files...', colors.cyan);
  const categories = ['conformance', 'compiler'];
  const allFiles: string[] = [];

  for (const cat of categories) {
    const dir = path.join(TESTS_BASE_PATH, cat);
    if (fs.existsSync(dir)) {
      const files = collectTestFiles(dir);
      allFiles.push(...files);
      log('  ' + cat + ': ' + files.length + ' files', colors.dim);
    }
  }

  log('  Total: ' + allFiles.length + ' files', colors.cyan);

  // Generate cache using worker threads
  log('\nGenerating TSC results...', colors.cyan);

  const entries: Record<string, CacheEntry> = {};
  let completed = 0;
  let errors = 0;
  let tscCrashes = 0;

  // Create a pool of workers
  const workerPath = path.join(__dirname, 'cache-worker.js');
  const libPath = path.join(ROOT_DIR, 'TypeScript/tests/lib/lib.d.ts');
  const libDir = path.join(ROOT_DIR, 'TypeScript/src/lib');
  const libSource = fs.existsSync(libPath) ? fs.readFileSync(libPath, 'utf8') : '';

  const workers: Worker[] = [];
  const pending = new Map<number, { filePath: string; resolve: (codes: number[]) => void }>();
  let nextId = 0;
  let fileIndex = 0;

  const updateProgress = () => {
    const pct = ((completed / allFiles.length) * 100).toFixed(1);
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    const rate = completed > 0 ? (completed / ((Date.now() - startTime) / 1000)).toFixed(0) : '0';
    const crashInfo = tscCrashes > 0 ? ' | tsc crashes: ' + tscCrashes : '';
    process.stdout.write('\r  Progress: ' + completed + '/' + allFiles.length + ' (' + pct + '%) | ' + rate + '/s | ' + elapsed + 's' + crashInfo + '    ');
  };

  // Process file and store result
  const processResult = (filePath: string, codes: number[], tscCrashed?: boolean, tscError?: string) => {
    const relPath = filePath.replace(TESTS_BASE_PATH + path.sep, '');
    const content = fs.readFileSync(filePath, 'utf8');
    const entry: CacheEntry = {
      codes,
      hash: hashContent(content),
    };
    if (tscCrashed) {
      entry.tscCrashed = true;
      entry.tscError = tscError;
      tscCrashes++;
    }
    entries[relPath] = entry;
    completed++;
    updateProgress();
  };

  // Create workers
  for (let i = 0; i < workerCount; i++) {
    const worker = new Worker(workerPath, {
      workerData: { libSource, libDir, testsBasePath: TESTS_BASE_PATH },
    });

    worker.on('message', (msg: { id: number; codes: number[]; error?: string; type?: string; tscCrashed?: boolean; tscError?: string }) => {
      if (msg.type === 'ready') return;

      const p = pending.get(msg.id);
      if (p) {
        pending.delete(msg.id);
        if (msg.tscCrashed) {
          // TSC crashed on this test (stack overflow, undefined access, etc.)
          processResult(p.filePath, [], true, msg.tscError);
        } else if (msg.error) {
          errors++;
          processResult(p.filePath, []);
        } else {
          processResult(p.filePath, msg.codes);
        }
        p.resolve(msg.codes);

        // Send next file
        if (fileIndex < allFiles.length) {
          const nextFile = allFiles[fileIndex++];
          const id = nextId++;
          pending.set(id, { filePath: nextFile, resolve: () => {} });
          worker.postMessage({ id, filePath: nextFile });
        }
      }
    });

    worker.on('error', (err) => {
      console.error('\nWorker error:', err);
    });

    workers.push(worker);
  }

  // Start initial batch and wait for completion
  await new Promise<void>((resolve) => {
    // Initial dispatch
    for (const worker of workers) {
      if (fileIndex < allFiles.length) {
        const filePath = allFiles[fileIndex++];
        const id = nextId++;
        pending.set(id, {
          filePath,
          resolve: () => {},
        });
        worker.postMessage({ id, filePath });
      }
    }

    // Poll for completion
    const interval = setInterval(() => {
      if (completed >= allFiles.length) {
        clearInterval(interval);
        resolve();
      }
    }, 100);
  });

  // Terminate workers
  for (const worker of workers) {
    await worker.terminate();
  }

  process.stdout.write('\n');

  // Save cache
  log('\nSaving cache...', colors.cyan);
  const saved = saveTscCache(ROOT_DIR, entries);

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
  const rate = (allFiles.length / ((Date.now() - startTime) / 1000)).toFixed(0);

  log('\n' + '='.repeat(60), colors.dim);
  if (saved) {
    log('Cache generated successfully!', colors.green);
    log('  Tests: ' + Object.keys(entries).length, colors.dim);
    log('  TSC crashes: ' + tscCrashes, tscCrashes > 0 ? colors.yellow : colors.dim);
    log('  Other errors: ' + errors, errors > 0 ? colors.yellow : colors.dim);
    log('  Time: ' + elapsed + 's (' + rate + ' tests/s)', colors.dim);
    log('  SHA: ' + tsSha?.slice(0, 12), colors.dim);
  } else {
    log('Failed to save cache!', colors.red);
  }
  log('='.repeat(60), colors.dim);
}

// CLI
const args = process.argv.slice(2);
let workers = os.cpus().length;

for (const arg of args) {
  if (arg === '--status') {
    showStatus().then(() => process.exit(0));
  } else if (arg === '--clear') {
    if (clearTscCache()) {
      log('Cache cleared.', colors.green);
    } else {
      log('Failed to clear cache.', colors.red);
    }
    process.exit(0);
  } else if (arg.startsWith('--workers=')) {
    workers = parseInt(arg.split('=')[1], 10);
  }
}

// Check if --status or --clear was handled (they call process.exit)
// Default: generate cache
if (!args.includes('--status') && !args.includes('--clear')) {
  generateCache(workers).then(() => process.exit(0));
}
