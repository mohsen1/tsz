/**
 * Server-mode runner for conformance tests.
 *
 * Uses tsz-server (persistent process) instead of spawning a new process per test.
 * This provides 5-10x speedup by:
 * - Keeping TypeScript libs cached in memory
 * - Avoiding process spawn overhead
 * - Reusing type interner across tests
 *
 * Protocol: JSON lines over stdin/stdout (similar to tsserver)
 */
import { spawn, execSync, spawnSync } from 'child_process';
import { createInterface } from 'readline';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { loadTscCache } from './tsc-cache.js';
import { parseTestCase, directivesToCheckOptions, shouldSkipTest, } from './test-utils.js';
import { runTscOnFiles } from './tsc-runner.js';
// ============================================================================
// Binary File Detection
// ============================================================================
/**
 * Check if a file appears to be binary (UTF-16, corrupted, etc.)
 * Returns true if the file should emit TS1490 instead of being parsed.
 */
function isBinaryFile(filePath) {
    try {
        const buffer = fs.readFileSync(filePath);
        if (buffer.length === 0)
            return false;
        // Check for UTF-16 BOM
        // UTF-16 BE: FE FF
        // UTF-16 LE: FF FE
        if (buffer.length >= 2) {
            if ((buffer[0] === 0xFE && buffer[1] === 0xFF) ||
                (buffer[0] === 0xFF && buffer[1] === 0xFE)) {
                return true;
            }
        }
        // Check for many null bytes (binary file indicator)
        const nullCount = buffer.slice(0, 1024).filter(b => b === 0).length;
        if (nullCount > 10) {
            return true;
        }
        // Check for consecutive null bytes (UTF-16 or binary)
        let consecutiveNulls = 0;
        for (let i = 0; i < Math.min(512, buffer.length); i++) {
            if (buffer[i] === 0) {
                consecutiveNulls++;
                if (consecutiveNulls >= 4)
                    return true;
            }
            else {
                consecutiveNulls = 0;
            }
        }
        return false;
    }
    catch {
        return false;
    }
}
/**
 * Safely read a source file, returning empty content for binary files.
 * Binary files should have TS1490 emitted by the checker.
 */
function readSourceFile(filePath) {
    if (isBinaryFile(filePath)) {
        return { content: '', isBinary: true };
    }
    try {
        const content = fs.readFileSync(filePath, 'utf-8');
        return { content, isBinary: false };
    }
    catch {
        return { content: '', isBinary: true }; // Treat read errors as binary
    }
}
// ============================================================================
// tsz Binary Runner (for --print-test mode)
// ============================================================================
/**
 * Run tsz binary on test files and capture full output.
 * For multi-file tests, uses the server protocol instead of CLI.
 */
function runTszWithFullOutput(files, tszBinaryPath, libDir, options) {
    const fileEntries = Object.entries(files);
    if (fileEntries.length === 0) {
        return { stdout: '', stderr: '', codes: [] };
    }
    // For multi-file tests or tests needing special options, use server protocol
    // The CLI doesn't support all options (experimentalDecorators, strictNullChecks, etc.)
    const needsServer = fileEntries.length > 1 ||
        options.experimentalDecorators ||
        options.emitDecoratorMetadata ||
        options.esModuleInterop ||
        options.allowSyntheticDefaultImports ||
        options.strict ||
        options.strictNullChecks;
    if (needsServer) {
        // Use server protocol for full option support
        const serverPath = tszBinaryPath.replace(/tsz$/, 'tsz-server');
        const request = {
            type: 'check',
            id: 1,
            files,
            options,
        };
        try {
            const result = spawnSync(serverPath, ['--protocol', 'legacy'], {
                input: JSON.stringify(request) + '\n',
                encoding: 'utf-8',
                env: { ...process.env, TSZ_LIB_DIR: libDir },
                timeout: 30000,
            });
            const stdout = result.stdout || '';
            const stderr = result.stderr || '';
            // Parse response from server
            const codes = [];
            for (const line of stdout.split('\n')) {
                if (line.trim().startsWith('{')) {
                    try {
                        const response = JSON.parse(line);
                        if (response.codes) {
                            codes.push(...response.codes);
                        }
                    }
                    catch {
                        // Ignore parse errors
                    }
                }
            }
            // Also check stderr for error codes (in case of CLI-style output)
            for (const match of stderr.matchAll(/TS(\d{4,5})/g)) {
                codes.push(parseInt(match[1], 10));
            }
            return { stdout, stderr, codes };
        }
        catch (err) {
            return { stdout: '', stderr: String(err), codes: [] };
        }
    }
    // Single file with basic options - use CLI directly
    const [filePath, content] = fileEntries[0];
    // Build args
    const args = [];
    if (options.strict)
        args.push('--strict');
    // Handle comma-separated targets (e.g., "es2015,es2017") by taking the first one
    if (options.target) {
        const firstTarget = options.target.split(',')[0].trim();
        args.push(`--target=${firstTarget}`);
    }
    if (options.noLib)
        args.push('--noLib');
    // If the file exists on disk, use it directly; otherwise write to temp file
    let tempFile = null;
    let actualFilePath = filePath;
    if (!fs.existsSync(filePath)) {
        tempFile = path.join(os.tmpdir(), `tsz-print-test-${Date.now()}.ts`);
        fs.writeFileSync(tempFile, content);
        actualFilePath = tempFile;
    }
    args.push(actualFilePath);
    try {
        const result = spawnSync(tszBinaryPath, args, {
            encoding: 'utf-8',
            env: { ...process.env, TSZ_LIB_DIR: libDir },
            timeout: 30000,
        });
        const stdout = result.stdout || '';
        const stderr = result.stderr || '';
        // Parse error codes from output (look for TS#### patterns)
        const codes = [];
        for (const match of stderr.matchAll(/TS(\d{4,5})/g)) {
            codes.push(parseInt(match[1], 10));
        }
        return { stdout, stderr, codes };
    }
    finally {
        if (tempFile && fs.existsSync(tempFile)) {
            fs.unlinkSync(tempFile);
        }
    }
}
// Memory configuration
const MEMORY_USAGE_PERCENT = 0.80; // Use 80% of available memory
const MEMORY_CHECK_INTERVAL_MS = 1000; // Check every 1s (was 500ms - too aggressive)
const MAX_CONSECUTIVE_VIOLATIONS = 3; // Require 3 consecutive violations before killing
const MIN_MEMORY_PER_WORKER_MB = 256; // Minimum 256MB per worker
const MAX_MEMORY_PER_WORKER_MB = 4096; // Maximum 4GB per worker
/**
 * Calculate memory limit per worker based on available system memory.
 * Uses 80% of total memory divided by number of workers.
 */
function calculateMemoryLimitMB(workerCount) {
    const totalMemoryMB = Math.round(os.totalmem() / 1024 / 1024);
    const availableMemoryMB = Math.round(totalMemoryMB * MEMORY_USAGE_PERCENT);
    const perWorkerMB = Math.round(availableMemoryMB / workerCount);
    // Clamp between min and max
    return Math.max(MIN_MEMORY_PER_WORKER_MB, Math.min(MAX_MEMORY_PER_WORKER_MB, perWorkerMB));
}
/**
 * Run a promise with a timeout. Returns { result, timedOut }.
 */
async function withTimeout(promise, timeoutMs) {
    let timeoutId;
    const timeoutPromise = new Promise((resolve) => {
        timeoutId = setTimeout(() => resolve({ timedOut: true }), timeoutMs);
    });
    try {
        const result = await Promise.race([
            promise.then(r => ({ result: r, timedOut: false })),
            timeoutPromise,
        ]);
        clearTimeout(timeoutId);
        return result;
    }
    catch (err) {
        clearTimeout(timeoutId);
        throw err;
    }
}
// Dynamic timeout: starts at 400ms, adapts to 10x average test time
const INITIAL_TIMEOUT_MS = 400; // Start with 400ms - account for cold worker cache rebuilds
const MIN_TIMEOUT_MS = 400; // Never go below 400ms
const MAX_TIMEOUT_MS = 60000;
const TIMEOUT_MULTIPLIER = 10;
// Lib directories for file-based resolution (set during test run)
let libDirs = [];
// Counter for unique temp directories
let tempDirCounter = 0;
/**
 * For multi-file tests, write virtual files to a temp directory as real files.
 * Returns the temp directory path and a map of real file paths to content.
 * For single-file tests, returns null tempDir and original files.
 *
 * Also handles:
 * - @link/@symlink directives for creating symlinks
 * - @currentDirectory for setting the working directory
 */
function prepareTestFiles(parsed, testFile) {
    // Single file test - just return the original file
    if (!parsed.isMultiFile || parsed.files.length <= 1) {
        const files = {};
        for (const file of parsed.files) {
            // Use original test file path for single-file tests
            // Ensure content is always a string (defensive check for undefined from parser edge cases)
            files[testFile] = file.content ?? '';
        }
        return { tempDir: null, files };
    }
    // Multi-file test - create temp directory and write real files
    const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), `tsz-test-${tempDirCounter++}-`));
    const files = {};
    for (const file of parsed.files) {
        // Normalize the filename (remove leading ./ if present)
        const normalizedName = file.name.replace(/^\.\//, '');
        const filePath = path.join(tempDir, normalizedName);
        // Create subdirectories if needed
        const dir = path.dirname(filePath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }
        // Ensure content is always a string (defensive check for undefined from parser edge cases)
        const content = file.content ?? '';
        // Write the file
        fs.writeFileSync(filePath, content, 'utf-8');
        // Add to files map with absolute path
        files[filePath] = content;
    }
    // Handle symlinks from harness options
    if (parsed.harness.symlinks) {
        for (const { target, link } of parsed.harness.symlinks) {
            const linkPath = path.join(tempDir, link.replace(/^\.\//, ''));
            const targetPath = path.join(tempDir, target.replace(/^\.\//, ''));
            // Create parent directory for the link if needed
            const linkDir = path.dirname(linkPath);
            if (!fs.existsSync(linkDir)) {
                fs.mkdirSync(linkDir, { recursive: true });
            }
            try {
                // Create symlink (target must be relative to link location or absolute)
                const relativeTarget = path.relative(linkDir, targetPath);
                // Determine symlink type for Windows compatibility
                const symlinkType = fs.existsSync(targetPath) && fs.statSync(targetPath).isDirectory() ? 'dir' : 'file';
                fs.symlinkSync(relativeTarget, linkPath, symlinkType);
            }
            catch {
                // Symlink creation may fail on some systems (e.g., Windows without admin), ignore
            }
        }
    }
    // Handle currentDirectory from harness options
    const currentDirectory = parsed.harness.currentDirectory
        ? path.join(tempDir, parsed.harness.currentDirectory.replace(/^\.\//, ''))
        : undefined;
    return { tempDir, files, currentDirectory };
}
/**
 * Clean up temp directory after test
 */
function cleanupTempDir(tempDir) {
    if (tempDir) {
        try {
            fs.rmSync(tempDir, { recursive: true, force: true });
        }
        catch {
            // Ignore cleanup errors
        }
    }
}
/**
 * Error code groupings by root cause.
 * Maps error codes to their likely root cause and estimated impact multiplier.
 *
 * Note: TS2304 (Cannot find name) is intentionally omitted as it has many
 * different root causes and appears in both missing and extra errors.
 */
const ERROR_ROOT_CAUSES = {
    libTypes: {
        codes: [2318, 2583, 2584],
        description: 'Global/lib type resolution (Partial, Pick, Record, etc.)',
        hint: 'Fix utility type resolution in lib.d.ts',
        multiplier: 1.2,
    },
    nullChecks: {
        codes: [18050, 18047, 18048, 18049],
        description: "Null/undefined value checks (TS18050: 'X' is possibly null)",
        hint: 'Add strictNullChecks enforcement',
        multiplier: 1.3,
    },
    moduleResolution: {
        codes: [2307, 2792, 2834, 2835],
        description: 'Module/import resolution',
        hint: 'Fix module resolver for node/bundler modes',
        multiplier: 1.0,
    },
    operatorTypes: {
        codes: [2365, 2362, 2363, 2469],
        description: 'Operator type constraints (+, -, <, > on non-numbers)',
        hint: 'Implement binary operator type checking',
        multiplier: 1.0,
    },
    duplicateIdentifiers: {
        codes: [2300, 2451, 2392, 2393],
        description: 'Duplicate identifier detection',
        hint: 'Already implemented - check edge cases in merging',
        multiplier: 0.8,
    },
    strictMode: {
        codes: [1210, 1212, 1213, 1214],
        description: "Strict mode (eval/arguments in class body)",
        hint: 'Implement class body strict mode checking',
        multiplier: 1.5,
    },
    typeAssignability: {
        codes: [2322, 2345, 2741],
        description: 'Type assignability (broad category)',
        hint: 'Review specific failing tests for patterns',
        multiplier: 0.5, // Very broad, many root causes
    },
    propertyAccess: {
        codes: [2339, 2551],
        description: 'Property does not exist on type',
        hint: 'Often a symptom - check lib resolution first',
        multiplier: 0.5, // Often symptom of other issues
    },
    parserScanner: {
        codes: [1127, 1005, 1128, 1109, 1003],
        description: 'Parser/scanner errors (invalid char, expected token)',
        hint: 'Check Unicode and ASI edge cases',
        multiplier: 0.7,
    },
};
/**
 * Analyze test stats and return actionable items sorted by estimated impact.
 */
function analyzeForActionableItems(stats) {
    const items = [];
    for (const [, group] of Object.entries(ERROR_ROOT_CAUSES)) {
        let totalCount = 0;
        const presentCodes = [];
        for (const code of group.codes) {
            const missingCount = stats.missingCodes.get(code) || 0;
            if (missingCount > 0) {
                totalCount += missingCount;
                presentCodes.push(code);
            }
        }
        if (totalCount > 0) {
            // Estimate test impact (rough heuristic: each error ~= 0.7 tests, adjusted by multiplier)
            const estimatedTests = Math.round(totalCount * 0.7 * group.multiplier);
            items.push({
                description: group.description,
                errorCodes: presentCodes,
                estimatedTests,
                hint: group.hint,
            });
        }
    }
    // Sort by estimated impact descending
    items.sort((a, b) => b.estimatedTests - a.estimatedTests);
    return items;
}
/**
 * Get memory usage of a process in MB (cross-platform).
 */
function getProcessMemoryMB(pid) {
    try {
        if (process.platform === 'darwin') {
            // macOS: use ps command
            const output = execSync(`ps -o rss= -p ${pid}`, { encoding: 'utf-8' });
            const rssKB = parseInt(output.trim(), 10);
            return Math.round(rssKB / 1024);
        }
        else if (process.platform === 'linux') {
            // Linux: read from /proc
            const statm = fs.readFileSync(`/proc/${pid}/statm`, 'utf-8');
            const pages = parseInt(statm.split(' ')[1], 10);
            return Math.round((pages * 4096) / 1024 / 1024);
        }
    }
    catch {
        // Process may have exited
    }
    return 0;
}
/**
 * Client for tsz-server.
 *
 * Manages a persistent tsz-server process and sends check requests.
 * Monitors memory usage and kills the process if it exceeds the limit.
 */
export class TszServerClient {
    proc = null;
    readline = null;
    requestId = 0;
    pending = new Map();
    ready = false;
    readyPromise = null;
    serverPath;
    libDir;
    memoryLimitMB;
    memoryCheckTimer = null;
    consecutiveViolations = 0; // Track consecutive memory limit violations
    oomKilled = false;
    checksCompleted = 0;
    constructor(options) {
        // Default to release binary in target directory
        this.serverPath = options.serverPath || path.join(__dirname, '../../target/release/tsz-server');
        this.libDir = options.libDir || path.join(__dirname, '../../TypeScript/src/lib');
        this.memoryLimitMB = options.memoryLimitMB;
    }
    get isOomKilled() {
        return this.oomKilled;
    }
    get isAlive() {
        return this.ready && this.proc !== null && !this.oomKilled;
    }
    get pid() {
        return this.proc?.pid;
    }
    /**
     * Start the server process.
     */
    async start() {
        if (this.proc) {
            throw new Error('Server already started');
        }
        this.oomKilled = false;
        this.checksCompleted = 0;
        this.consecutiveViolations = 0;
        this.readyPromise = new Promise((resolve, reject) => {
            const env = {
                ...process.env,
                TSZ_LIB_DIR: this.libDir,
            };
            this.proc = spawn(this.serverPath, ['--protocol', 'legacy'], {
                stdio: ['pipe', 'pipe', 'pipe'],
                env,
            });
            // Handle EPIPE errors on stdin (when process is killed)
            this.proc.stdin?.on('error', (err) => {
                if (err.code === 'EPIPE' || err.code === 'ERR_STREAM_DESTROYED') {
                    // Process was killed - this is expected, mark as OOM
                    this.oomKilled = true;
                }
            });
            // Handle stderr for ready signal and logging
            const traceMode = !!process.env.TSZ_TRACE;
            this.proc.stderr?.on('data', (data) => {
                const text = data.toString();
                if (text.includes('tsz-server ready')) {
                    this.ready = true;
                    resolve();
                }
                // In trace mode, forward all stderr output for debugging
                if (traceMode) {
                    process.stderr.write(text);
                }
                // Suppress stack overflow and debug output - these are handled via process exit
                // Only log genuine panics that aren't stack overflows
                else if (text.includes('panic') && !text.includes('stack overflow')) {
                    process.stderr.write(`[tsz-server] ${text}`);
                }
            });
            // Set up readline for stdout (responses)
            this.readline = createInterface({
                input: this.proc.stdout,
                crlfDelay: Infinity,
            });
            this.readline.on('line', (line) => {
                this.handleResponse(line);
            });
            // Handle process exit
            this.proc.on('exit', (code, signal) => {
                if (!this.ready) {
                    reject(new Error(`Server exited before ready (code: ${code}, signal: ${signal})`));
                }
                // Check if killed by OOM killer (SIGKILL on Linux, or our memory monitor)
                if (signal === 'SIGKILL' || code === 137) {
                    this.oomKilled = true;
                }
                this.cleanup();
            });
            this.proc.on('error', (err) => {
                reject(err);
            });
            // Timeout for startup
            setTimeout(() => {
                if (!this.ready) {
                    reject(new Error('Server startup timeout'));
                }
            }, 10000);
        });
        await this.readyPromise;
        // Start memory monitoring
        this.startMemoryMonitor();
    }
    /**
     * Start monitoring memory usage.
     * Uses a "strike" system: requires 3 consecutive violations before killing
     * to avoid false positives from transient memory spikes.
     */
    startMemoryMonitor() {
        this.memoryCheckTimer = setInterval(() => {
            if (!this.proc?.pid)
                return;
            const memoryMB = getProcessMemoryMB(this.proc.pid);
            if (memoryMB > this.memoryLimitMB) {
                this.consecutiveViolations++;
                // Only kill after multiple consecutive violations to avoid false positives
                // from transient spikes (e.g., during lib merging or batch parsing)
                if (this.consecutiveViolations >= MAX_CONSECUTIVE_VIOLATIONS) {
                    this.oomKilled = true;
                    this.killProcess();
                }
            }
            else {
                // Reset counter if memory is back under limit
                this.consecutiveViolations = 0;
            }
        }, MEMORY_CHECK_INTERVAL_MS);
    }
    /**
     * Kill the server process (internal).
     */
    killProcess() {
        if (this.proc) {
            // Reject all pending requests
            for (const [id, callback] of this.pending) {
                callback({ id, error: 'Process killed due to memory limit', oom: true });
            }
            this.pending.clear();
            try {
                this.proc.kill('SIGKILL');
            }
            catch { }
        }
    }
    /**
     * Force kill this worker (public, for timeout handling).
     */
    forceKill() {
        this.oomKilled = true; // Mark as needing restart
        this.killProcess();
    }
    /**
     * Clean up resources.
     */
    cleanup() {
        if (this.memoryCheckTimer) {
            clearInterval(this.memoryCheckTimer);
            this.memoryCheckTimer = null;
        }
        this.consecutiveViolations = 0;
        this.proc = null;
        this.readline = null;
        this.ready = false;
    }
    /**
     * Stop the server process.
     */
    async stop() {
        if (!this.proc || !this.ready) {
            this.cleanup();
            return;
        }
        try {
            await this.sendRequest({ type: 'shutdown', id: ++this.requestId });
        }
        catch {
            // Ignore errors during shutdown
        }
        // Force kill if still running
        if (this.proc) {
            try {
                this.proc.kill();
            }
            catch { }
        }
        this.cleanup();
    }
    /**
     * Check files and return error codes.
     */
    async check(files, options = {}) {
        if (!this.ready) {
            throw new Error('Server not ready');
        }
        if (this.oomKilled) {
            return { codes: [], elapsed_ms: 0, oom: true };
        }
        const id = ++this.requestId;
        const request = {
            type: 'check',
            id,
            files,
            options,
        };
        const response = await this.sendRequest(request);
        if (response.oom) {
            return { codes: [], elapsed_ms: 0, oom: true };
        }
        if (response.error) {
            throw new Error(response.error);
        }
        this.checksCompleted++;
        return {
            codes: response.codes || [],
            elapsed_ms: response.elapsed_ms || 0,
        };
    }
    /**
     * Get server status.
     */
    async status() {
        if (!this.ready) {
            throw new Error('Server not ready');
        }
        const id = ++this.requestId;
        const response = await this.sendRequest({ type: 'status', id });
        return {
            memory_mb: response.memory_mb || 0,
            checks_completed: response.checks_completed || 0,
            cached_libs: response.cached_libs || 0,
        };
    }
    /**
     * Recycle server (clear caches).
     */
    async recycle() {
        if (!this.ready) {
            throw new Error('Server not ready');
        }
        const id = ++this.requestId;
        await this.sendRequest({ type: 'recycle', id });
    }
    sendRequest(request) {
        return new Promise((resolve, reject) => {
            // Check if process was OOM killed
            if (this.oomKilled) {
                resolve({ id: request.id, oom: true, error: 'Process was OOM killed' });
                return;
            }
            if (!this.proc?.stdin || !this.ready) {
                reject(new Error('Server not running'));
                return;
            }
            this.pending.set(request.id, resolve);
            const line = JSON.stringify(request) + '\n';
            try {
                this.proc.stdin.write(line, (err) => {
                    if (err) {
                        this.pending.delete(request.id);
                        // Handle EPIPE gracefully - process was killed
                        if (err.code === 'EPIPE' || err.code === 'ERR_STREAM_DESTROYED') {
                            this.oomKilled = true;
                            resolve({ id: request.id, oom: true, error: 'Process was killed' });
                        }
                        else {
                            reject(err);
                        }
                    }
                });
            }
            catch (err) {
                this.pending.delete(request.id);
                if (err.code === 'EPIPE' || err.code === 'ERR_STREAM_DESTROYED') {
                    this.oomKilled = true;
                    resolve({ id: request.id, oom: true, error: 'Process was killed' });
                }
                else {
                    reject(err);
                }
            }
            // Timeout for individual requests (30s)
            setTimeout(() => {
                if (this.pending.has(request.id)) {
                    this.pending.delete(request.id);
                    reject(new Error(`Request ${request.id} timeout`));
                }
            }, 30000);
        });
    }
    handleResponse(line) {
        try {
            const response = JSON.parse(line);
            const id = response.id;
            if (id !== undefined && this.pending.has(id)) {
                const callback = this.pending.get(id);
                this.pending.delete(id);
                callback(response);
            }
        }
        catch (err) {
            console.error('Failed to parse server response:', line);
        }
    }
}
/**
 * Pool of server clients for parallel test execution.
 * Automatically restarts workers that get OOM killed.
 */
export class TszServerPool {
    clients = [];
    available = [];
    waiting = [];
    options;
    poolSize;
    // Stats
    oomKills = 0;
    totalRestarts = 0;
    constructor(size, options) {
        this.poolSize = size;
        this.options = options;
    }
    /**
     * Start all servers in the pool.
     */
    async start() {
        const startPromises = [];
        for (let i = 0; i < this.poolSize; i++) {
            const client = new TszServerClient(this.options);
            this.clients.push(client);
            startPromises.push(client.start());
        }
        await Promise.all(startPromises);
        this.available = [...this.clients];
    }
    /**
     * Stop all servers in the pool.
     */
    async stop() {
        await Promise.all(this.clients.map(c => c.stop()));
        this.clients = [];
        this.available = [];
    }
    /**
     * Acquire a client from the pool.
     * If a client is dead (crashed or OOM), restart it first.
     */
    async acquire() {
        if (this.available.length > 0) {
            const client = this.available.pop();
            // Check if client is dead and needs restart
            if (!client.isAlive) {
                if (client.isOomKilled) {
                    this.oomKills++;
                }
                this.totalRestarts++;
                // Stop and remove the dead client
                await client.stop();
                const idx = this.clients.indexOf(client);
                if (idx >= 0) {
                    this.clients.splice(idx, 1);
                }
                // Create and start a new client
                const newClient = new TszServerClient(this.options);
                await newClient.start();
                this.clients.push(newClient);
                return newClient;
            }
            return client;
        }
        // Wait for a client to become available
        return new Promise((resolve) => {
            this.waiting.push(resolve);
        });
    }
    /**
     * Release a client back to the pool.
     * If client is dead, restart it before handing to waiters.
     */
    release(client) {
        // If client is dead (crashed or OOM), restart it asynchronously
        if (!client.isAlive) {
            this.restartAndRelease(client, client.isOomKilled);
            return;
        }
        if (this.waiting.length > 0) {
            const resolve = this.waiting.shift();
            resolve(client);
        }
        else {
            this.available.push(client);
        }
    }
    /**
     * Restart a dead client and release the new one.
     */
    async restartAndRelease(deadClient, wasOom) {
        if (wasOom) {
            this.oomKills++;
        }
        this.totalRestarts++;
        // Stop and remove the dead client
        await deadClient.stop();
        const idx = this.clients.indexOf(deadClient);
        if (idx >= 0) {
            this.clients.splice(idx, 1);
        }
        // Create and start a new client
        const newClient = new TszServerClient(this.options);
        try {
            await newClient.start();
            this.clients.push(newClient);
            // Now release the healthy client
            if (this.waiting.length > 0) {
                const resolve = this.waiting.shift();
                resolve(newClient);
            }
            else {
                this.available.push(newClient);
            }
        }
        catch (err) {
            // Failed to start new client, try again later
            console.error('Failed to restart worker:', err);
        }
    }
    /**
     * Run a function with an acquired client.
     */
    async withClient(fn) {
        const client = await this.acquire();
        try {
            return await fn(client);
        }
        finally {
            this.release(client);
        }
    }
    /**
     * Run a function with timeout. If timeout fires, kill the worker.
     * Returns { result, timedOut }.
     */
    async withClientTimeout(fn, timeoutMs) {
        const client = await this.acquire();
        let timeoutId;
        let timedOut = false;
        const timeoutPromise = new Promise((_, reject) => {
            timeoutId = setTimeout(() => {
                timedOut = true;
                client.forceKill(); // Kill worker on timeout
                reject(new Error('timeout'));
            }, timeoutMs);
        });
        try {
            const result = await Promise.race([fn(client), timeoutPromise]);
            clearTimeout(timeoutId);
            return { result, timedOut: false };
        }
        catch (err) {
            clearTimeout(timeoutId);
            if (timedOut) {
                return { timedOut: true };
            }
            throw err;
        }
        finally {
            this.release(client);
        }
    }
    /**
     * Get pool statistics.
     */
    get stats() {
        return {
            total: this.clients.length,
            available: this.available.length,
            waiting: this.waiting.length,
            oomKills: this.oomKills,
            totalRestarts: this.totalRestarts,
        };
    }
}
// Export for use in conformance runner
export default TszServerClient;
const colors = {
    reset: '\x1b[0m',
    red: '\x1b[31m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    blue: '\x1b[34m',
    magenta: '\x1b[35m',
    cyan: '\x1b[36m',
    dim: '\x1b[2m',
    bold: '\x1b[1m',
};
function log(msg, color = '') {
    console.log(`${color}${msg}${colors.reset}`);
}
function formatNumber(n) {
    return n.toLocaleString('en-US');
}
function formatMemory(mb) {
    if (mb >= 1024) {
        return `${(mb / 1024).toFixed(1).replace(/\.0$/, '')}GB`;
    }
    return `${formatNumber(mb)}MB`;
}
/**
 * Print detailed information about a specific test.
 * Used with --print-test --filter=<pattern> for debugging.
 *
 * Shows FULL error output from both TSC and tsz (not just codes).
 */
async function printTestDetails(testFile, relativePath, _cacheEntries, _pool, libDirs, tszBinaryPath) {
    const content = fs.readFileSync(testFile, 'utf-8');
    // Parse test case to handle @Filename directives for multi-file tests
    const parsed = parseTestCase(content, testFile);
    // For multi-file tests, write real files to temp directory
    const prepared = prepareTestFiles(parsed, testFile);
    const tempDir = prepared.tempDir;
    const files = prepared.files;
    const checkOptions = directivesToCheckOptions(parsed.directives, libDirs);
    log('\n' + '='.repeat(80), colors.cyan);
    log(`  TEST: ${relativePath}`, colors.bold);
    log('='.repeat(80), colors.cyan);
    // Print file content with line numbers
    log('\n[File Content]', colors.bold);
    log('-'.repeat(60), colors.dim);
    const lines = content.split('\n');
    lines.forEach((line, i) => {
        const lineNum = String(i + 1).padStart(3, ' ');
        log(`${colors.dim}${lineNum}|${colors.reset} ${line}`);
    });
    log('-'.repeat(60), colors.dim);
    // Print harness options (test control)
    log('\n[Harness Options]', colors.bold);
    const harnessEntries = Object.entries(parsed.harness).filter(([, v]) => v !== undefined);
    if (harnessEntries.length === 0) {
        log('  (none)', colors.dim);
    }
    else {
        for (const [key, value] of harnessEntries) {
            log(`  ${key}: ${JSON.stringify(value)}`, colors.magenta);
        }
    }
    // Print parsed compiler directives
    log('\n[Compiler Directives]', colors.bold);
    for (const [key, value] of Object.entries(parsed.directives)) {
        if (value !== undefined) {
            log(`  ${key}: ${JSON.stringify(value)}`, colors.yellow);
        }
    }
    // Check if test would be skipped
    const skipResult = shouldSkipTest(parsed.harness);
    if (skipResult.skip) {
        log(`\n[!] Test would be SKIPPED: ${skipResult.reason}`, colors.yellow);
    }
    // Print check options sent to compilers
    log('\n[Compiler Options]', colors.bold);
    log(`  ${JSON.stringify(checkOptions, null, 2).split('\n').join('\n  ')}`, colors.dim);
    // =========================================================================
    // Run TSC with full diagnostic output (using shared tsc-runner)
    // =========================================================================
    log('\n' + '-'.repeat(80), colors.dim);
    log('[TSC Output] (running TypeScript compiler)', colors.bold);
    log('-'.repeat(80), colors.dim);
    const libDir = libDirs[0] || '';
    const tscResult = runTscOnFiles(files, checkOptions, libDir, true);
    const tscDiagnostics = tscResult.diagnostics;
    if (tscDiagnostics.length === 0) {
        log('  (no errors)', colors.green);
    }
    else {
        for (const diag of tscDiagnostics) {
            const location = diag.file && diag.line
                ? `${colors.dim}${path.basename(diag.file)}(${diag.line},${diag.column})${colors.reset}: `
                : '';
            log(`  ${location}${colors.red}error${colors.reset} ${colors.cyan}TS${diag.code}${colors.reset}: ${diag.message}`);
        }
    }
    // =========================================================================
    // Run tsz with full diagnostic output
    // =========================================================================
    log('\n' + '-'.repeat(80), colors.dim);
    log('[tsz Output] (running tsz compiler)', colors.bold);
    log('-'.repeat(80), colors.dim);
    const tszResult = runTszWithFullOutput(files, tszBinaryPath, libDir, checkOptions);
    if (tszResult.stderr.trim()) {
        for (const line of tszResult.stderr.split('\n')) {
            if (line.trim()) {
                log(`  ${line}`);
            }
        }
    }
    else if (tszResult.stdout.trim()) {
        for (const line of tszResult.stdout.split('\n')) {
            if (line.trim()) {
                log(`  ${line}`);
            }
        }
    }
    else {
        log('  (no errors)', colors.green);
    }
    // =========================================================================
    // Compare error codes
    // =========================================================================
    log('\n' + '-'.repeat(80), colors.dim);
    log('[Comparison]', colors.bold);
    log('-'.repeat(80), colors.dim);
    const tscCodes = tscDiagnostics.map(d => d.code);
    const tszCodes = tszResult.codes;
    const tscSet = new Set(tscCodes);
    const tszSet = new Set(tszCodes);
    const missing = tscCodes.filter(c => !tszSet.has(c));
    const extra = tszCodes.filter(c => !tscSet.has(c));
    // Show code summary
    log(`\n  TSC codes: [${[...new Set(tscCodes)].sort((a, b) => a - b).map(c => `TS${c}`).join(', ')}]`, colors.dim);
    log(`  tsz codes: [${[...new Set(tszCodes)].sort((a, b) => a - b).map(c => `TS${c}`).join(', ')}]`, colors.dim);
    if (missing.length === 0 && extra.length === 0) {
        log('\n  [PASS] Error codes match!', colors.green);
    }
    else {
        log('\n  [FAIL] Error codes differ', colors.red);
        if (missing.length > 0) {
            const grouped = missing.reduce((acc, code) => {
                acc[code] = (acc[code] || 0) + 1;
                return acc;
            }, {});
            log('\n  Missing (tsz should emit but doesn\'t):', colors.yellow);
            for (const [code, count] of Object.entries(grouped)) {
                const diag = tscDiagnostics.find(d => d.code === Number(code));
                log(`    TS${code}: ${count}x`, colors.yellow);
                if (diag) {
                    log(`      -> ${diag.message.slice(0, 100)}${diag.message.length > 100 ? '...' : ''}`, colors.dim);
                }
            }
        }
        if (extra.length > 0) {
            const grouped = extra.reduce((acc, code) => {
                acc[code] = (acc[code] || 0) + 1;
                return acc;
            }, {});
            log('\n  Extra (tsz emits but shouldn\'t):', colors.red);
            for (const [code, count] of Object.entries(grouped)) {
                log(`    TS${code}: ${count}x`, colors.red);
            }
        }
    }
    // Clean up temp directory for multi-file tests
    cleanupTempDir(tempDir);
    log('\n' + '='.repeat(80) + '\n', colors.cyan);
}
/**
 * Run conformance tests using tsz-server (persistent process).
 *
 * This is 5-10x faster than spawn-per-test mode because:
 * - TypeScript libs are cached in memory
 * - No process spawn overhead per test
 * - Type interner is reused
 */
export async function runServerConformanceTests(config = {}) {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const ROOT_DIR = path.resolve(__dirname, '../../..');
    const maxTests = config.maxTests ?? Infinity;
    const workerCount = config.workers ?? 8;
    const verbose = config.verbose ?? false;
    const categories = config.categories ?? ['conformance', 'compiler'];
    const testTimeout = config.testTimeout ?? 10000;
    const filter = config.filter;
    const errorCode = config.errorCode;
    const printTest = config.printTest ?? false;
    const dumpResults = config.dumpResults;
    // Calculate memory limit: 80% of total memory / number of workers
    const memoryLimitMB = config.memoryLimitMB ?? calculateMemoryLimitMB(workerCount);
    const totalMemoryMB = Math.round(os.totalmem() / 1024 / 1024);
    const testsBasePath = path.resolve(ROOT_DIR, 'TypeScript/tests/cases');
    const serverPath = process.env.TSZ_SERVER_BINARY || path.resolve(ROOT_DIR, '.target/release/tsz-server');
    const tszBinaryPath = process.env.TSZ_BINARY || path.resolve(ROOT_DIR, '.target/release/tsz');
    const localLibDir = process.env.TSZ_LIB_DIR || path.resolve(ROOT_DIR, 'TypeScript/src/lib');
    // Set lib directories for universal resolver (used by directivesToCheckOptions)
    libDirs = [
        localLibDir,
        path.resolve(ROOT_DIR, 'TypeScript/tests/lib'),
    ];
    // Load TSC cache (required for meaningful conformance comparison)
    const tscCache = loadTscCache(ROOT_DIR);
    if (!tscCache) {
        log(`\nERROR: TSC cache not found!`, colors.red);
        log(`\n  The conformance runner requires a TSC baseline cache to compare against.`, colors.yellow);
        log(`  Without it, tests can only pass if tsz produces zero errors.\n`, colors.yellow);
        log(`  Generate the cache first:`, colors.bold);
        log(`    cd scripts/conformance && node dist/generate-cache.js\n`, colors.cyan);
        log(`  Or use the run.sh helper:`, colors.bold);
        log(`    bash scripts/conformance/run.sh cache generate\n`, colors.cyan);
        process.exit(2);
    }
    const cacheEntries = tscCache.entries;
    log(`\n  TSC cache loaded: ${Object.keys(cacheEntries).length} entries`, colors.dim);
    log(`\nTSZ Server Mode Conformance Runner`, colors.bold);
    log(`Server: ${serverPath}`, colors.dim);
    log(`Workers: ${workerCount}  Max: ${formatNumber(maxTests)}  Timeout: dynamic (10x avg, ${INITIAL_TIMEOUT_MS}ms initial)`, colors.dim);
    log(`Memory: ${formatMemory(memoryLimitMB)}/worker (${formatMemory(Math.round(memoryLimitMB * workerCount))} total, system: ${formatMemory(totalMemoryMB)})\n`, colors.dim);
    // Initialize stats
    const stats = {
        total: 0,
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
        workerStats: { spawned: workerCount, crashed: 0, respawned: 0 },
    };
    // Discover test files - when filtering by error code, discover all first then filter
    let testFiles = [];
    const discoveryLimit = errorCode ? Infinity : maxTests;
    for (const category of categories) {
        const categoryPath = path.join(testsBasePath, category);
        if (fs.existsSync(categoryPath)) {
            discoverTests(categoryPath, testFiles, discoveryLimit - testFiles.length);
        }
    }
    // Apply filter if specified
    if (filter) {
        const filterLower = filter.toLowerCase();
        testFiles = testFiles.filter(f => f.toLowerCase().includes(filterLower));
        log(`Filter "${filter}": ${formatNumber(testFiles.length)} tests match\n`, colors.yellow);
    }
    // Apply error code filter if specified
    if (errorCode) {
        const beforeCount = testFiles.length;
        testFiles = testFiles.filter(f => {
            const relativePath = path.relative(testsBasePath, f);
            const entry = cacheEntries[relativePath];
            // Include test if TSC expects this error code
            return entry?.codes?.includes(errorCode) ?? false;
        });
        log(`Error code TS${errorCode}: ${formatNumber(testFiles.length)} tests match (from ${formatNumber(beforeCount)})\n`, colors.yellow);
        // Apply max limit after error code filter
        if (testFiles.length > maxTests) {
            testFiles = testFiles.slice(0, maxTests);
            log(`Limited to first ${formatNumber(maxTests)} tests\n`, colors.dim);
        }
    }
    if (testFiles.length === 0) {
        log('No test files found!', colors.red);
        return stats;
    }
    // Handle --print-test mode
    if (printTest) {
        log(`Print-test mode: showing details for ${testFiles.length} test(s)\n`, colors.cyan);
        // Create a single-worker pool for print-test mode
        const pool = new TszServerPool(1, { serverPath, libDir: localLibDir, memoryLimitMB });
        try {
            await pool.start();
            for (const testFile of testFiles.slice(0, 10)) { // Limit to 10 for safety
                const relativePath = path.relative(testsBasePath, testFile);
                await printTestDetails(testFile, relativePath, cacheEntries, pool, libDirs, tszBinaryPath);
            }
            if (testFiles.length > 10) {
                log(`\n(Showing first 10 of ${testFiles.length} matching tests. Use a more specific filter.)`, colors.yellow);
            }
        }
        finally {
            await pool.stop();
        }
        return stats;
    }
    log(`Found ${formatNumber(testFiles.length)} test files\n`, colors.dim);
    // Create server pool with memory limits
    const pool = new TszServerPool(workerCount, { serverPath, libDir: localLibDir, memoryLimitMB });
    try {
        await pool.start();
        log(`Server pool started (${workerCount} workers, ${formatMemory(memoryLimitMB)} limit each)\n`, colors.green);
        // Per-test results for dump mode
        const perTestResults = [];
        // Process tests in parallel
        const startTime = Date.now();
        let completed = 0;
        // Dynamic timeout tracking (10x average test time)
        let testTimeSum = 0;
        let testTimeCount = 0;
        let currentTimeout = INITIAL_TIMEOUT_MS;
        let consecutiveTimeouts = 0;
        let consecutiveOom = 0;
        const FAST_TIMEOUT_MS = 200; // When many timeouts, use fast timeout
        const getDynamicTimeout = () => {
            // If we're seeing many consecutive timeouts/OOM, use fast timeout
            // This prevents workers from being blocked for 10s on each bad test
            if (consecutiveTimeouts >= 3 || consecutiveOom >= 3) {
                return FAST_TIMEOUT_MS;
            }
            if (testTimeCount < 10)
                return INITIAL_TIMEOUT_MS; // Need samples first
            const avg = testTimeSum / testTimeCount;
            const dynamic = Math.round(avg * TIMEOUT_MULTIPLIER);
            return Math.max(MIN_TIMEOUT_MS, Math.min(MAX_TIMEOUT_MS, dynamic));
        };
        const recordTestTime = (ms) => {
            testTimeSum += ms;
            testTimeCount++;
            consecutiveTimeouts = 0; // Reset on successful test
            consecutiveOom = 0;
            // Update timeout every 50 tests for stability
            if (testTimeCount % 50 === 0) {
                currentTimeout = getDynamicTimeout();
            }
        };
        const recordTimeout = () => {
            consecutiveTimeouts++;
            currentTimeout = getDynamicTimeout();
        };
        const recordOom = () => {
            consecutiveOom++;
            currentTimeout = getDynamicTimeout();
        };
        // Progress bar state (time-throttled for smooth updates)
        const PROGRESS_BAR_WIDTH = 30;
        const PROGRESS_UPDATE_MS = 50;
        let lastProgressUpdate = 0;
        let progressInterval = null;
        let cachedPoolStats = { oomKills: 0, totalRestarts: 0 };
        const renderProgressBar = (current, total) => {
            const percent = total > 0 ? current / total : 0;
            const filled = Math.round(percent * PROGRESS_BAR_WIDTH);
            const empty = PROGRESS_BAR_WIDTH - filled;
            return `${colors.green}${'█'.repeat(filled)}${colors.dim}${'░'.repeat(empty)}${colors.reset}`;
        };
        const updateProgress = (force = false) => {
            if (verbose)
                return;
            try {
                const now = Date.now();
                if (!force && now - lastProgressUpdate < PROGRESS_UPDATE_MS)
                    return;
                lastProgressUpdate = now;
                const elapsed = (now - startTime) / 1000;
                const rate = elapsed > 0 ? completed / elapsed : 0;
                const percent = testFiles.length > 0 ? ((completed / testFiles.length) * 100).toFixed(1) : '0.0';
                try {
                    cachedPoolStats = pool.stats;
                }
                catch { }
                const statusParts = [];
                if (stats.crashed > 0)
                    statusParts.push(`${colors.red}err:${stats.crashed}${colors.reset}`);
                if (cachedPoolStats.oomKills > 0)
                    statusParts.push(`${colors.magenta}oom:${cachedPoolStats.oomKills}${colors.reset}`);
                if (stats.timedOut > 0)
                    statusParts.push(`${colors.yellow}to:${stats.timedOut}${colors.reset}`);
                statusParts.push(`${colors.dim}t:${currentTimeout}ms${colors.reset}`);
                const status = statusParts.length > 0 ? ` ${statusParts.join(' ')}` : '';
                const bar = renderProgressBar(completed, testFiles.length);
                const line = `  ${bar} ${percent.padStart(5)}% | ${formatNumber(completed).padStart(6)}/${formatNumber(testFiles.length).padEnd(6)} | ${rate.toFixed(0).padStart(4)}/s${status}`;
                process.stdout.write(`\r${line.padEnd(100)}`);
            }
            catch { }
        };
        if (!verbose) {
            progressInterval = setInterval(() => updateProgress(), PROGRESS_UPDATE_MS);
            progressInterval.unref();
        }
        const runTest = async (testFile) => {
            const category = path.basename(path.dirname(testFile));
            const relativePath = path.relative(testsBasePath, testFile);
            if (!stats.byCategory[category]) {
                stats.byCategory[category] = { total: 0, passed: 0 };
            }
            stats.byCategory[category].total++;
            stats.total++;
            let tempDir = null;
            try {
                const content = fs.readFileSync(testFile, 'utf-8');
                // Parse test case to handle @Filename directives for multi-file tests
                const parsed = parseTestCase(content, testFile);
                // Check if test should be skipped based on harness options
                const skipResult = shouldSkipTest(parsed.harness);
                if (skipResult.skip) {
                    stats.skipped++;
                    if (verbose)
                        log(`[skip] ${relativePath}: ${skipResult.reason}`, colors.dim);
                    return;
                }
                // For multi-file tests, write real files to temp directory
                const prepared = prepareTestFiles(parsed, testFile);
                tempDir = prepared.tempDir;
                const files = prepared.files;
                // Parse test directives (@target, @lib, @strict, etc.)
                const checkOptions = directivesToCheckOptions(parsed.directives, libDirs);
                // If currentDirectory was specified, pass it to check options
                if (prepared.currentDirectory) {
                    checkOptions.currentDirectory = prepared.currentDirectory;
                }
                // Get TSC baseline from cache
                const cacheEntry = cacheEntries[relativePath];
                const tscCodes = cacheEntry?.codes || [];
                // Run check via server with dynamic timeout
                const { result, timedOut } = await pool.withClientTimeout((client) => client.check(files, checkOptions), currentTimeout);
                // Check for timeout
                if (timedOut) {
                    stats.timedOut++;
                    stats.failed++;
                    stats.timedOutTests.push(relativePath);
                    recordTimeout(); // Track for adaptive timeout
                    if (verbose)
                        log(`[timeout] ${relativePath}`, colors.yellow);
                    return;
                }
                // Check for OOM
                if (result.oom) {
                    stats.oom++;
                    stats.failed++;
                    stats.oomTests.push(relativePath);
                    recordOom(); // Track for adaptive timeout
                    if (verbose)
                        log(`[oom] ${relativePath}`, colors.yellow);
                    return;
                }
                // Record test time for dynamic timeout calculation
                if (result.elapsed_ms > 0) {
                    recordTestTime(result.elapsed_ms);
                }
                const wasmCodes = result.codes;
                // Compare results - exact set match of unique error codes
                const tscSet = new Set(tscCodes);
                const wasmSet = new Set(wasmCodes);
                const missing = tscCodes.filter(c => !wasmSet.has(c));
                const extra = wasmCodes.filter(c => !tscSet.has(c));
                if (missing.length === 0 && extra.length === 0) {
                    stats.passed++;
                    stats.byCategory[category].passed++;
                    if (verbose)
                        log(`[pass] ${relativePath}`, colors.green);
                    if (dumpResults)
                        perTestResults.push({ path: relativePath, missing: [], extra: [], tscCodes, tszCodes: wasmCodes, status: 'pass' });
                }
                else {
                    stats.failed++;
                    if (verbose) {
                        log(`[fail] ${relativePath}`, colors.red);
                        if (missing.length > 0)
                            log(`  Missing: ${missing.join(', ')}`, colors.yellow);
                        if (extra.length > 0)
                            log(`  Extra: ${extra.join(', ')}`, colors.yellow);
                    }
                    for (const code of missing) {
                        stats.missingCodes.set(code, (stats.missingCodes.get(code) || 0) + 1);
                    }
                    for (const code of extra) {
                        stats.extraCodes.set(code, (stats.extraCodes.get(code) || 0) + 1);
                    }
                    if (dumpResults)
                        perTestResults.push({ path: relativePath, missing: [...new Set(missing)], extra: [...new Set(extra)], tscCodes, tszCodes: wasmCodes, status: 'fail' });
                }
            }
            catch (err) {
                stats.crashed++;
                stats.crashedTests.push({ path: relativePath, error: err.message });
                if (verbose)
                    log(`[crash] ${relativePath}: ${err.message}`, colors.red);
            }
            finally {
                // Clean up temp directory for multi-file tests
                cleanupTempDir(tempDir);
            }
            completed++;
            updateProgress();
        };
        // Run tests with concurrency limit
        const concurrency = workerCount;
        const queue = [...testFiles];
        const running = [];
        while (queue.length > 0 || running.length > 0) {
            while (running.length < concurrency && queue.length > 0) {
                const testFile = queue.shift();
                const promise = runTest(testFile).then(() => {
                    running.splice(running.indexOf(promise), 1);
                });
                running.push(promise);
            }
            if (running.length > 0) {
                await Promise.race(running);
            }
        }
        // Stop progress interval and show final state
        if (progressInterval) {
            clearInterval(progressInterval);
            progressInterval = null;
        }
        updateProgress(true);
        const elapsed = (Date.now() - startTime) / 1000;
        process.stdout.write('\r' + ' '.repeat(100) + '\r');
        // Update worker stats from pool
        const poolStats = pool.stats;
        stats.workerStats.crashed = poolStats.oomKills;
        stats.workerStats.respawned = poolStats.totalRestarts;
        // Print summary
        log('\nCONFORMANCE TEST RESULTS', colors.bold);
        // Calculate stats
        const actualFailed = stats.failed - stats.oom - stats.timedOut;
        const effectiveTotal = stats.total - stats.skipped;
        const passRate = effectiveTotal > 0 ? ((stats.passed / effectiveTotal) * 100).toFixed(1) : '0.0';
        // Pass rate and summary first (most important)
        log(`\nPass Rate: ${colors.bold}${passRate}%${colors.reset} (${formatNumber(stats.passed)}/${formatNumber(effectiveTotal)})`, stats.passed === effectiveTotal ? colors.green : colors.yellow);
        log(`Time: ${elapsed.toFixed(1)}s (${(effectiveTotal / elapsed).toFixed(0)} tests/sec)`, colors.dim);
        // Actionable insights - grouped by root cause
        const actionableItems = analyzeForActionableItems(stats);
        if (actionableItems.length > 0) {
            log('\nHighest Impact Fixes:', colors.bold);
            for (const item of actionableItems.slice(0, 6)) {
                const impact = item.estimatedTests >= 100 ? colors.green : item.estimatedTests >= 50 ? colors.yellow : colors.dim;
                log(`  ${impact}~${item.estimatedTests} tests${colors.reset}: ${item.description}`);
                log(`    ${colors.dim}${item.errorCodes.map(c => `TS${c}`).join(', ')} → ${item.hint}${colors.reset}`);
            }
        }
        // Compact error summary (only if not verbose)
        if (!verbose) {
            const topMissing = [...stats.missingCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5);
            const topExtra = [...stats.extraCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5);
            if (topMissing.length > 0 || topExtra.length > 0) {
                log('\nError Summary:', colors.bold);
                if (topMissing.length > 0) {
                    log(`  Missing: ${topMissing.map(([c, n]) => `TS${c}(${n})`).join(' ')}`, colors.yellow);
                }
                if (topExtra.length > 0) {
                    log(`  Extra:   ${topExtra.map(([c, n]) => `TS${c}(${n})`).join(' ')}`, colors.red);
                }
            }
        }
        // Verbose mode: show all categories
        if (verbose) {
            log('\nBy Category:', colors.bold);
            // Sort by failure count descending
            const sortedCategories = Object.entries(stats.byCategory)
                .sort((a, b) => (b[1].total - b[1].passed) - (a[1].total - a[1].passed));
            for (const [cat, s] of sortedCategories) {
                const r = s.total > 0 ? ((s.passed / s.total) * 100).toFixed(1) : '0.0';
                log(`  ${cat}: ${s.passed}/${s.total} (${r}%)`, s.passed === s.total ? colors.green : colors.yellow);
            }
            log('\nTop Extra Errors:', colors.bold);
            for (const [c, n] of [...stats.extraCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
                log(`  TS${c}: ${n}x`, colors.yellow);
            }
            log('\nTop Missing Errors:', colors.bold);
            for (const [c, n] of [...stats.missingCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
                log(`  TS${c}: ${n}x`, colors.yellow);
            }
        }
        // Problematic tests (always show, but limit count)
        if (stats.timedOutTests.length > 0) {
            log('\nTimed Out Tests:', colors.yellow);
            for (const t of stats.timedOutTests.slice(0, 3)) {
                log(`  ${t}`, colors.dim);
            }
            if (stats.timedOutTests.length > 3) {
                log(`  ... and ${stats.timedOutTests.length - 3} more`, colors.dim);
            }
        }
        if (stats.crashedTests.length > 0) {
            log('\nCrashed Tests:', colors.red);
            for (const t of stats.crashedTests.slice(0, 3)) {
                log(`  ${t.path}`, colors.dim);
                log(`    ${t.error.slice(0, 80)}`, colors.dim);
            }
            if (stats.crashedTests.length > 3) {
                log(`  ... and ${stats.crashedTests.length - 3} more`, colors.dim);
            }
        }
        // Worker stats (compact)
        if (poolStats.oomKills > 0 || poolStats.totalRestarts > 0) {
            log('\nWorker Health:', colors.bold);
            log(`  Spawned: ${workerCount} | Crashed: ${poolStats.oomKills} | Respawned: ${poolStats.totalRestarts}`, poolStats.oomKills > 0 ? colors.red : colors.dim);
        }
        // Summary counts
        log('\nSummary:', colors.bold);
        log(`  Pass Rate: ${passRate}%  Time: ${elapsed.toFixed(1)}s  (${(effectiveTotal / elapsed).toFixed(0)} tests/sec)`, stats.passed === effectiveTotal ? colors.green : colors.yellow);
        log(`  Passed:  ${formatNumber(stats.passed)}  Failed: ${formatNumber(actualFailed)}  Skipped: ${formatNumber(stats.skipped)}`, actualFailed > 0 ? colors.yellow : colors.green);
        if (stats.crashed > 0 || stats.oom > 0 || stats.timedOut > 0) {
            log(`  Crashed: ${stats.crashed}  OOM: ${stats.oom}  Timeout: ${stats.timedOut}`, colors.red);
        }
        log(`\nTip: --error-code=TSXXXX to filter by error, --filter=PATTERN --print-test for details, --trace to dig deep (add tracing::debug! in Rust)`, colors.dim);
        // Dump per-test results if requested
        if (dumpResults) {
            fs.writeFileSync(dumpResults, JSON.stringify(perTestResults, null, 0));
            log(`\nDumped ${perTestResults.length} test results to ${dumpResults}`, colors.cyan);
        }
    }
    finally {
        await pool.stop();
    }
    return stats;
}
function discoverTests(dir, results, limit) {
    if (results.length >= limit)
        return;
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
        if (results.length >= limit)
            break;
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
            discoverTests(fullPath, results, limit);
        }
        else if ((entry.name.endsWith('.ts') || entry.name.endsWith('.tsx')) &&
            !entry.name.endsWith('.d.ts')) {
            results.push(fullPath);
        }
    }
}
//# sourceMappingURL=runner-server.js.map