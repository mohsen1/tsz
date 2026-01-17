#!/usr/bin/env node
/**
 * Process Pool Conformance Test Runner
 * Uses child processes instead of worker threads to avoid WASM finalization issues
 */

import { fork } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join, resolve } from 'path';
import { readdirSync, statSync, writeFileSync, unlinkSync, existsSync } from 'fs';
import { cpus } from 'os';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const CONFIG = {
  wasmPkgPath: resolve(__dirname, '../pkg'),
  conformanceDir: resolve(__dirname, '../tests/cases/conformance'),
};

const colors = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
};

function log(msg, color = '') {
  console.log(`${color}${msg}${colors.reset}`);
}

function getTestFiles(dir, maxFiles = 500) {
  const files = [];

  function walk(currentDir) {
    if (files.length >= maxFiles) return;

    const entries = readdirSync(currentDir);
    for (const entry of entries) {
      if (files.length >= maxFiles) break;

      const fullPath = join(currentDir, entry);
      const stat = statSync(fullPath);

      if (stat.isDirectory()) {
        walk(fullPath);
      } else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts')) {
        files.push(fullPath);
      }
    }
  }

  walk(dir);
  return files;
}

function chunkArray(array, chunks) {
  const result = [];
  const chunkSize = Math.ceil(array.length / chunks);
  for (let i = 0; i < array.length; i += chunkSize) {
    result.push(array.slice(i, i + chunkSize));
  }
  return result;
}

async function runChildProcess(testFiles, workerId, wasmPkgPath, conformanceDir, onProgress) {
  const TIMEOUT_MS = 5 * 60 * 1000; // 5 minute timeout per worker

  return new Promise((resolve, reject) => {
    // Write test files to a temp file for the child process
    const tempFile = `/tmp/conformance-tests-${workerId}-${Date.now()}.json`;
    writeFileSync(tempFile, JSON.stringify({ testFiles, wasmPkgPath, conformanceDir }));

    const child = fork(join(__dirname, 'conformance-child.mjs'), [tempFile], {
      stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
    });

    let results = [];
    let stderr = '';
    let timeoutId = null;

    const cleanup = () => {
      if (timeoutId) clearTimeout(timeoutId);
      try { unlinkSync(tempFile); } catch {}
    };

    // Set timeout to prevent hanging
    timeoutId = setTimeout(() => {
      log(`Worker ${workerId} timed out after ${TIMEOUT_MS / 1000}s, killing...`, colors.yellow);
      child.kill('SIGKILL');
      cleanup();
      reject(new Error(`Worker ${workerId} timed out after ${TIMEOUT_MS / 1000}s`));
    }, TIMEOUT_MS);

    child.stdout.on('data', (data) => {
      // Ignore stdout (children use IPC for progress)
    });

    child.stderr.on('data', (data) => {
      stderr += data.toString();
    });

    child.on('message', (msg) => {
      if (msg.type === 'done') {
        results = msg.results;
      } else if (msg.type === 'error') {
        cleanup();
        reject(new Error(msg.error));
      } else if (msg.type === 'progress' && onProgress) {
        onProgress(msg.completed, msg.total);
      }
    });

    child.on('exit', (code) => {
      cleanup();

      if (code !== 0 && results.length === 0) {
        reject(new Error(`Worker ${workerId} exited with code ${code}: ${stderr}`));
      } else {
        resolve(results);
      }
    });

    child.on('error', (err) => {
      cleanup();
      reject(err);
    });
  });
}

async function main() {
  const args = process.argv.slice(2);
  const maxTests = parseInt(args.find(a => a.startsWith('--max='))?.split('=')[1] || '999999', 10);
  const numWorkers = parseInt(args.find(a => a.startsWith('--workers='))?.split('=')[1] || String(cpus().length), 10);
  const category = args.find(a => !a.startsWith('-'))?.toLowerCase();

  log('Process Pool Conformance Test Runner', colors.bold);
  log('═'.repeat(60), colors.dim);
  log(`  Workers: ${numWorkers} (child processes)`, colors.cyan);

  let testDir = CONFIG.conformanceDir;
  if (category) {
    testDir = join(CONFIG.conformanceDir, category);
    log(`  Category: ${category}`, colors.cyan);
  }

  log(`\nCollecting test files (max ${maxTests})...`, colors.cyan);
  const testFiles = getTestFiles(testDir, maxTests);
  log(`  Found ${testFiles.length} test files`, colors.dim);

  // Split tests among workers
  const chunks = chunkArray(testFiles, numWorkers);
  log(`  Distributing across ${chunks.length} child processes (~${Math.ceil(testFiles.length / numWorkers)} tests each)`, colors.dim);

  log(`\nRunning tests in parallel (child processes)...`, colors.cyan);
  const startTime = Date.now();

  // Track progress from all workers
  const workerProgress = new Array(chunks.length).fill(0);
  let completedCount = 0;

  const updateProgress = (workerId, workerCompleted, workerTotal) => {
    const oldWorkerProgress = workerProgress[workerId];
    workerProgress[workerId] = workerCompleted;
    completedCount += (workerCompleted - oldWorkerProgress);
  };

  const progressInterval = setInterval(() => {
    process.stdout.write(`\r  Progress: ${completedCount}/${testFiles.length} (${((completedCount / testFiles.length) * 100).toFixed(1)}%)`);
  }, 500);

  try {
    // Run all child processes in parallel
    const childPromises = chunks.map((chunk, i) =>
      runChildProcess(chunk, i, CONFIG.wasmPkgPath, CONFIG.conformanceDir, (completed, total) => {
        updateProgress(i, completed, total);
      })
    );
    const childResults = await Promise.all(childPromises);

    clearInterval(progressInterval);
    console.log(''); // New line after progress

    // Aggregate results
    const allResults = childResults.flat();
    completedCount = allResults.length;

    const endTime = Date.now();
    const duration = (endTime - startTime) / 1000;
    const testsPerSec = (testFiles.length / duration).toFixed(1);

    // Calculate stats
    const stats = {
      total: 0,
      multiFile: 0,
      exactMatch: 0,
      sameCount: 0,
      crashed: 0,
      skipped: 0,
      missingErrors: 0,
      extraErrors: 0,
      byCategory: {},
    };

    const missingCodeCounts = {};
    const extraCodeCounts = {};
    const crashedFiles = [];

    for (const result of allResults) {
      if (result.skipped) {
        stats.skipped++;
        continue;
      }

      stats.total++;
      const cat = result.cat;
      stats.byCategory[cat] = stats.byCategory[cat] || { total: 0, exact: 0, same: 0 };
      stats.byCategory[cat].total++;

      if (result.isMultiFile) stats.multiFile++;

      if (result.crashed) {
        stats.crashed++;
        crashedFiles.push({ file: result.relPath, error: result.error });
        continue;
      }

      if (result.exactMatch) {
        stats.exactMatch++;
        stats.byCategory[cat].exact++;
      }

      if (result.sameCount) {
        stats.sameCount++;
        stats.byCategory[cat].same++;
      }

      if (result.missingInWasm && result.missingInWasm.length > 0) {
        stats.missingErrors++;
        for (const code of result.missingInWasm) {
          missingCodeCounts[code] = (missingCodeCounts[code] || 0) + 1;
        }
      }

      if (result.extraInWasm && result.extraInWasm.length > 0) {
        stats.extraErrors++;
        for (const code of result.extraInWasm) {
          extraCodeCounts[code] = (extraCodeCounts[code] || 0) + 1;
          // Track files with specific extra errors for debugging
          if (!stats.extraFiles) stats.extraFiles = {};
          if (!stats.extraFiles[code]) stats.extraFiles[code] = [];
          if (stats.extraFiles[code].length < 3) {
            stats.extraFiles[code].push(result.relPath);
          }
        }
      }
    }

    // Print report
    log('\n' + '═'.repeat(60), colors.bold);
    log('  CONFORMANCE TEST REPORT', colors.bold);
    log('═'.repeat(60), colors.bold);

    log(`\n  Performance:`, colors.cyan);
    log(`    Duration:         ${duration.toFixed(1)}s`);
    log(`    Throughput:       ${testsPerSec} tests/sec`, colors.green);
    log(`    Workers:          ${numWorkers} (child processes)`);

    log(`\n  Summary:`, colors.cyan);
    log(`    Files Found:      ${testFiles.length}`);
    log(`    Multi-File Tests: ${stats.multiFile}`, colors.cyan);
    log(`    Tests Run:        ${stats.total}`);
    log(`    Exact Match:      ${stats.exactMatch} (${(stats.exactMatch / stats.total * 100).toFixed(1)}%)`, colors.green);
    log(`    Same Error Count: ${stats.sameCount} (${(stats.sameCount / stats.total * 100).toFixed(1)}%)`, colors.blue);
    log(`    WASM Crashed:     ${stats.crashed}`, stats.crashed > 0 ? colors.red : '');
    if (stats.skipped > 0) {
      log(`    Skipped:          ${stats.skipped}`, colors.dim);
    }

    log(`\n  Parity Issues:`, colors.cyan);
    log(`    Tests with missing errors: ${stats.missingErrors} (${(stats.missingErrors / stats.total * 100).toFixed(1)}%)`, stats.missingErrors > 0 ? colors.red : colors.green);
    log(`    Tests with extra errors:   ${stats.extraErrors} (${(stats.extraErrors / stats.total * 100).toFixed(1)}%)`, stats.extraErrors > 0 ? colors.yellow : colors.green);

    if (Object.keys(missingCodeCounts).length > 0) {
      log(`\n  Most Common Missing Error Codes:`, colors.cyan);
      const sorted = Object.entries(missingCodeCounts).sort((a, b) => b[1] - a[1]).slice(0, 10);
      for (const [code, count] of sorted) {
        log(`    TS${code}: ${count} occurrences`, colors.red);
      }
    }

    if (Object.keys(extraCodeCounts).length > 0) {
      log(`\n  Most Common Extra Error Codes:`, colors.cyan);
      const sorted = Object.entries(extraCodeCounts).sort((a, b) => b[1] - a[1]).slice(0, 10);
      for (const [code, count] of sorted) {
        log(`    TS${code}: ${count} occurrences`, colors.yellow);
        // Show example files for debugging
        if (stats.extraFiles && stats.extraFiles[code]) {
          for (const file of stats.extraFiles[code].slice(0, 2)) {
            log(`      - ${file}`, colors.dim);
          }
        }
      }
    }

    if (Object.keys(stats.byCategory).length > 1) {
      log(`\n  By Category:`, colors.cyan);
      const sorted = Object.entries(stats.byCategory).sort((a, b) => b[1].total - a[1].total);
      for (const [cat, data] of sorted.slice(0, 15)) {
        const pct = (data.exact / data.total * 100).toFixed(0);
        const bar = '█'.repeat(Math.floor(pct / 5)) + '░'.repeat(20 - Math.floor(pct / 5));
        log(`    ${cat.padEnd(20)} ${bar} ${pct}% exact (${data.exact}/${data.total})`);
      }
    }

    if (crashedFiles.length > 0 && crashedFiles.length <= 10) {
      log(`\n  Crashed Files:`, colors.red);
      for (const { file, error } of crashedFiles) {
        log(`    ${file}: ${error?.slice(0, 80) || 'Unknown error'}`, colors.dim);
      }
    }

    log('\n' + '═'.repeat(60) + '\n', colors.bold);

  } catch (e) {
    clearInterval(progressInterval);
    console.error('Error running tests:', e);
    process.exit(1);
  }
}

main().catch(e => {
  console.error('Fatal error:', e);
  process.exit(1);
});
