#!/usr/bin/env node
/**
 * Parallel Conformance Test Runner
 *
 * Runs each test in a separate child process with timeout.
 * This ensures hanging tests can be killed without affecting others.
 */
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { fork } from 'child_process';
import { fileURLToPath } from 'url';
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_CONFIG = {
    wasmPkgPath: path.resolve(__dirname, '../../pkg'),
    testsBasePath: path.resolve(__dirname, '../../TypeScript/tests/cases'),
    libPath: path.resolve(__dirname, '../../TypeScript/tests/lib/lib.d.ts'),
    maxTests: 500,
    verbose: false,
    categories: ['conformance', 'compiler'],
    workers: Math.max(1, os.cpus().length - 1),
    testTimeout: 10000, // 10 seconds per test
};
const colors = {
    reset: '\x1b[0m',
    red: '\x1b[31m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    cyan: '\x1b[36m',
    dim: '\x1b[2m',
    bold: '\x1b[1m',
};
function log(msg, color = '') {
    console.log(`${color}${msg}${colors.reset}`);
}
function collectTestFiles(dir, maxFiles) {
    const files = [];
    function walk(currentDir) {
        if (files.length >= maxFiles)
            return;
        let entries;
        try {
            entries = fs.readdirSync(currentDir);
        }
        catch {
            return;
        }
        for (const entry of entries) {
            if (files.length >= maxFiles)
                break;
            const fullPath = path.join(currentDir, entry);
            let stat;
            try {
                stat = fs.statSync(fullPath);
            }
            catch {
                continue;
            }
            if (stat.isDirectory())
                walk(fullPath);
            else if (entry.endsWith('.ts') && !entry.endsWith('.d.ts'))
                files.push(fullPath);
        }
    }
    walk(dir);
    return files;
}
function compareResults(tscCodes, wasmCodes) {
    const tscSet = new Map();
    const wasmSet = new Map();
    for (const c of tscCodes)
        tscSet.set(c, (tscSet.get(c) || 0) + 1);
    for (const c of wasmCodes)
        wasmSet.set(c, (wasmSet.get(c) || 0) + 1);
    const missing = [];
    const extra = [];
    for (const [code, count] of tscSet) {
        const wasmCount = wasmSet.get(code) || 0;
        for (let i = 0; i < count - wasmCount; i++)
            missing.push(code);
    }
    for (const [code, count] of wasmSet) {
        const tscCount = tscSet.get(code) || 0;
        for (let i = 0; i < count - tscCount; i++)
            extra.push(code);
    }
    return { exactMatch: missing.length === 0 && extra.length === 0, missing, extra };
}
/**
 * Run a single test in an isolated child process with timeout
 */
function runTestInProcess(filePath, wasmPkgPath, libPath, timeout, testsBasePath) {
    return new Promise((resolve) => {
        const relPath = filePath.replace(testsBasePath + path.sep, '');
        const executorPath = path.join(__dirname, 'test-executor.js');
        let child = null;
        let resolved = false;
        let output = '';
        const timeoutId = setTimeout(() => {
            if (!resolved) {
                resolved = true;
                if (child) {
                    child.kill('SIGKILL');
                }
                resolve({
                    filePath,
                    relPath,
                    category: 'unknown',
                    tscCodes: [],
                    wasmCodes: [],
                    crashed: false,
                    timedOut: true,
                    skipped: false,
                    error: `Timeout after ${timeout}ms`,
                });
            }
        }, timeout);
        try {
            child = fork(executorPath, [filePath, wasmPkgPath, libPath], {
                stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
                timeout: timeout + 1000, // Extra buffer for fork timeout
            });
            child.stdout?.on('data', (data) => {
                output += data.toString();
            });
            child.stderr?.on('data', (data) => {
                // Ignore stderr - might contain WASM warnings
            });
            child.on('exit', (code) => {
                clearTimeout(timeoutId);
                if (resolved)
                    return;
                resolved = true;
                try {
                    // Find JSON in output (might have extra content)
                    const jsonMatch = output.match(/\{[\s\S]*\}/);
                    if (jsonMatch) {
                        const result = JSON.parse(jsonMatch[0]);
                        resolve({
                            filePath,
                            relPath,
                            category: result.category || 'unknown',
                            tscCodes: result.tscCodes || [],
                            wasmCodes: result.wasmCodes || [],
                            crashed: result.crashed || false,
                            timedOut: false,
                            skipped: false,
                            error: result.error,
                        });
                    }
                    else {
                        resolve({
                            filePath,
                            relPath,
                            category: 'unknown',
                            tscCodes: [],
                            wasmCodes: [],
                            crashed: true,
                            timedOut: false,
                            skipped: false,
                            error: `No valid output from test process (exit code: ${code})`,
                        });
                    }
                }
                catch (e) {
                    resolve({
                        filePath,
                        relPath,
                        category: 'unknown',
                        tscCodes: [],
                        wasmCodes: [],
                        crashed: true,
                        timedOut: false,
                        skipped: false,
                        error: `Failed to parse output: ${e}`,
                    });
                }
            });
            child.on('error', (err) => {
                clearTimeout(timeoutId);
                if (resolved)
                    return;
                resolved = true;
                resolve({
                    filePath,
                    relPath,
                    category: 'unknown',
                    tscCodes: [],
                    wasmCodes: [],
                    crashed: true,
                    timedOut: false,
                    skipped: false,
                    error: `Process error: ${err.message}`,
                });
            });
        }
        catch (err) {
            clearTimeout(timeoutId);
            if (!resolved) {
                resolved = true;
                resolve({
                    filePath,
                    relPath,
                    category: 'unknown',
                    tscCodes: [],
                    wasmCodes: [],
                    crashed: true,
                    timedOut: false,
                    skipped: true,
                    error: `Failed to spawn: ${err}`,
                });
            }
        }
    });
}
/**
 * Process pool that runs tests with controlled concurrency
 */
class ProcessPool {
    maxConcurrency;
    running = 0;
    queue = [];
    constructor(maxConcurrency) {
        this.maxConcurrency = maxConcurrency;
    }
    async run(task) {
        if (this.running >= this.maxConcurrency) {
            await new Promise(resolve => this.queue.push(resolve));
        }
        this.running++;
        try {
            return await task();
        }
        finally {
            this.running--;
            const next = this.queue.shift();
            if (next)
                next();
        }
    }
}
export async function runConformanceTests(config = {}) {
    const cfg = { ...DEFAULT_CONFIG, ...config };
    const startTime = Date.now();
    log('╔══════════════════════════════════════════════════════════╗', colors.cyan);
    log('║     Parallel Conformance Test Runner (Process Pool)      ║', colors.cyan);
    log('╚══════════════════════════════════════════════════════════╝', colors.cyan);
    // Collect test files
    log(`\nCollecting test files...`, colors.cyan);
    const allTestFiles = [];
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
    log(`  Workers: ${cfg.workers} (${cfg.testTimeout}ms timeout per test)`, colors.dim);
    if (allTestFiles.length === 0) {
        log('\nNo test files found!', colors.yellow);
        return { total: 0, passed: 0, failed: 0, crashed: 0, skipped: 0, timedOut: 0, byCategory: {}, missingCodes: new Map(), extraCodes: new Map() };
    }
    // Run tests with process pool
    log(`\nRunning tests...`, colors.cyan);
    const stats = {
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
    const pool = new ProcessPool(cfg.workers);
    let completed = 0;
    const updateProgress = () => {
        const pct = ((completed / allTestFiles.length) * 100).toFixed(1);
        const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
        const rate = completed > 0 ? (completed / ((Date.now() - startTime) / 1000)).toFixed(0) : '0';
        process.stdout.write(`\r  Progress: ${completed}/${allTestFiles.length} (${pct}%) | ${rate} tests/sec | ${elapsed}s    `);
    };
    const promises = allTestFiles.map(filePath => pool.run(async () => {
        const result = await runTestInProcess(filePath, cfg.wasmPkgPath, cfg.libPath, cfg.testTimeout, cfg.testsBasePath);
        completed++;
        if (!cfg.verbose)
            updateProgress();
        // Update stats
        if (!stats.byCategory[result.category]) {
            stats.byCategory[result.category] = { total: 0, passed: 0 };
        }
        stats.byCategory[result.category].total++;
        if (result.timedOut) {
            stats.timedOut++;
            stats.failed++;
            if (cfg.verbose)
                log(`\n  ${result.relPath}: TIMEOUT`, colors.red);
            return;
        }
        if (result.skipped) {
            stats.skipped++;
            return;
        }
        if (result.crashed) {
            stats.crashed++;
            stats.failed++;
            if (cfg.verbose)
                log(`\n  ${result.relPath}: CRASH - ${result.error}`, colors.red);
            return;
        }
        const comparison = compareResults(result.tscCodes, result.wasmCodes);
        if (comparison.exactMatch) {
            stats.passed++;
            stats.byCategory[result.category].passed++;
        }
        else {
            stats.failed++;
            for (const code of comparison.missing) {
                stats.missingCodes.set(code, (stats.missingCodes.get(code) || 0) + 1);
            }
            for (const code of comparison.extra) {
                stats.extraCodes.set(code, (stats.extraCodes.get(code) || 0) + 1);
            }
            if (cfg.verbose) {
                log(`\n  ${result.relPath}:`, colors.yellow);
                if (comparison.missing.length > 0)
                    log(`    Missing: TS${[...new Set(comparison.missing)].join(', TS')}`, colors.dim);
                if (comparison.extra.length > 0)
                    log(`    Extra: TS${[...new Set(comparison.extra)].join(', TS')}`, colors.dim);
            }
        }
    }));
    await Promise.all(promises);
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
        log(`  ${cat}: ${catStats.passed}/${catStats.total} (${catRate}%)`, catStats.passed === catStats.total ? colors.green : colors.yellow);
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
    const config = {};
    for (const arg of args) {
        if (arg.startsWith('--max='))
            config.maxTests = parseInt(arg.split('=')[1], 10);
        else if (arg.startsWith('--workers='))
            config.workers = parseInt(arg.split('=')[1], 10);
        else if (arg.startsWith('--timeout='))
            config.testTimeout = parseInt(arg.split('=')[1], 10);
        else if (arg === '--verbose' || arg === '-v')
            config.verbose = true;
        else if (arg.startsWith('--category='))
            config.categories = arg.split('=')[1].split(',');
    }
    runConformanceTests(config).then(stats => {
        process.exit(stats.failed > 0 ? 1 : 0);
    });
}
//# sourceMappingURL=runner.js.map