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

import { spawn, ChildProcess, execSync } from 'child_process';
import { createInterface, Interface } from 'readline';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';
import { loadTscCache, type CacheEntry } from './tsc-cache.js';
import {
  parseDirectivesOnly,
  parseTestCase,
  directivesToCheckOptions,
  shouldSkipTest,
  type CheckOptions,
  type HarnessOptions,
} from './test-utils.js';

// Memory configuration
const MEMORY_USAGE_PERCENT = 0.80; // Use 80% of available memory
const MEMORY_CHECK_INTERVAL_MS = 500;
const MIN_MEMORY_PER_WORKER_MB = 256; // Minimum 256MB per worker
const MAX_MEMORY_PER_WORKER_MB = 4096; // Maximum 4GB per worker

/**
 * Calculate memory limit per worker based on available system memory.
 * Uses 80% of total memory divided by number of workers.
 */
function calculateMemoryLimitMB(workerCount: number): number {
  const totalMemoryMB = Math.round(os.totalmem() / 1024 / 1024);
  const availableMemoryMB = Math.round(totalMemoryMB * MEMORY_USAGE_PERCENT);
  const perWorkerMB = Math.round(availableMemoryMB / workerCount);

  // Clamp between min and max
  return Math.max(MIN_MEMORY_PER_WORKER_MB, Math.min(MAX_MEMORY_PER_WORKER_MB, perWorkerMB));
}

/**
 * Run a promise with a timeout. Returns { result, timedOut }.
 */
async function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs: number
): Promise<{ result?: T; timedOut: boolean }> {
  let timeoutId: NodeJS.Timeout;
  const timeoutPromise = new Promise<{ timedOut: true }>((resolve) => {
    timeoutId = setTimeout(() => resolve({ timedOut: true }), timeoutMs);
  });

  try {
    const result = await Promise.race([
      promise.then(r => ({ result: r, timedOut: false as const })),
      timeoutPromise,
    ]);
    clearTimeout(timeoutId!);
    return result;
  } catch (err) {
    clearTimeout(timeoutId!);
    throw err;
  }
}

// Dynamic timeout: starts at 500ms, adapts to 10x average test time
const INITIAL_TIMEOUT_MS = 500;
const MIN_TIMEOUT_MS = 50;
const MAX_TIMEOUT_MS = 5000;
const TIMEOUT_MULTIPLIER = 10;

// Lib directories for file-based resolution (set during test run)
let libDirs: string[] = [];

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
function prepareTestFiles(
  parsed: { files: Array<{ name: string; content: string }>; isMultiFile: boolean; harness: HarnessOptions },
  testFile: string
): { tempDir: string | null; files: Record<string, string>; currentDirectory?: string } {
  // Single file test - just return the original file
  if (!parsed.isMultiFile || parsed.files.length <= 1) {
    const files: Record<string, string> = {};
    for (const file of parsed.files) {
      // Use original test file path for single-file tests
      files[testFile] = file.content;
    }
    return { tempDir: null, files };
  }

  // Multi-file test - create temp directory and write real files
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), `tsz-test-${tempDirCounter++}-`));
  const files: Record<string, string> = {};

  for (const file of parsed.files) {
    // Normalize the filename (remove leading ./ if present)
    const normalizedName = file.name.replace(/^\.\//, '');
    const filePath = path.join(tempDir, normalizedName);

    // Create subdirectories if needed
    const dir = path.dirname(filePath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    // Write the file
    fs.writeFileSync(filePath, file.content, 'utf-8');

    // Add to files map with absolute path
    files[filePath] = file.content;
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
      } catch {
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
function cleanupTempDir(tempDir: string | null): void {
  if (tempDir) {
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup errors
    }
  }
}

export interface CheckResult {
  codes: number[];
  elapsed_ms: number;
  oom?: boolean;
}

export interface ServerStatus {
  memory_mb: number;
  checks_completed: number;
  cached_libs: number;
}

type ResponseCallback = (response: any) => void;

/**
 * Get memory usage of a process in MB (cross-platform).
 */
function getProcessMemoryMB(pid: number): number {
  try {
    if (process.platform === 'darwin') {
      // macOS: use ps command
      const output = execSync(`ps -o rss= -p ${pid}`, { encoding: 'utf-8' });
      const rssKB = parseInt(output.trim(), 10);
      return Math.round(rssKB / 1024);
    } else if (process.platform === 'linux') {
      // Linux: read from /proc
      const statm = fs.readFileSync(`/proc/${pid}/statm`, 'utf-8');
      const pages = parseInt(statm.split(' ')[1], 10);
      return Math.round((pages * 4096) / 1024 / 1024);
    }
  } catch {
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
  private proc: ChildProcess | null = null;
  private readline: Interface | null = null;
  private requestId = 0;
  private pending = new Map<number, ResponseCallback>();
  private ready = false;
  private readyPromise: Promise<void> | null = null;
  private serverPath: string;
  private libDir: string;
  private memoryLimitMB: number;
  private memoryCheckTimer: NodeJS.Timeout | null = null;
  private oomKilled = false;
  private checksCompleted = 0;

  constructor(options: { serverPath?: string; libDir?: string; memoryLimitMB: number }) {
    // Default to release binary in target directory
    this.serverPath = options.serverPath || path.join(__dirname, '../../target/release/tsz-server');
    this.libDir = options.libDir || path.join(__dirname, '../../TypeScript/src/lib');
    this.memoryLimitMB = options.memoryLimitMB;
  }

  get isOomKilled(): boolean {
    return this.oomKilled;
  }

  get isAlive(): boolean {
    return this.ready && this.proc !== null && !this.oomKilled;
  }

  get pid(): number | undefined {
    return this.proc?.pid;
  }

  /**
   * Start the server process.
   */
  async start(): Promise<void> {
    if (this.proc) {
      throw new Error('Server already started');
    }

    this.oomKilled = false;
    this.checksCompleted = 0;

    this.readyPromise = new Promise((resolve, reject) => {
      const env = {
        ...process.env,
        TSZ_LIB_DIR: this.libDir,
      };

      this.proc = spawn(this.serverPath, [], {
        stdio: ['pipe', 'pipe', 'pipe'],
        env,
      });

      // Handle EPIPE errors on stdin (when process is killed)
      this.proc.stdin?.on('error', (err: any) => {
        if (err.code === 'EPIPE' || err.code === 'ERR_STREAM_DESTROYED') {
          // Process was killed - this is expected, mark as OOM
          this.oomKilled = true;
        }
      });

      // Handle stderr for ready signal and logging
      this.proc.stderr?.on('data', (data: Buffer) => {
        const text = data.toString();
        if (text.includes('tsz-server ready')) {
          this.ready = true;
          resolve();
        }
        // Suppress stack overflow and debug output - these are handled via process exit
        // Only log genuine panics that aren't stack overflows
        if (text.includes('panic') && !text.includes('stack overflow')) {
          process.stderr.write(`[tsz-server] ${text}`);
        }
      });

      // Set up readline for stdout (responses)
      this.readline = createInterface({
        input: this.proc.stdout!,
        crlfDelay: Infinity,
      });

      this.readline.on('line', (line: string) => {
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
   */
  private startMemoryMonitor(): void {
    this.memoryCheckTimer = setInterval(() => {
      if (!this.proc?.pid) return;

      const memoryMB = getProcessMemoryMB(this.proc.pid);
      if (memoryMB > this.memoryLimitMB) {
        this.oomKilled = true;
        this.killProcess();
      }
    }, MEMORY_CHECK_INTERVAL_MS);
  }

  /**
   * Kill the server process (internal).
   */
  private killProcess(): void {
    if (this.proc) {
      // Reject all pending requests
      for (const [id, callback] of this.pending) {
        callback({ id, error: 'Process killed due to memory limit', oom: true });
      }
      this.pending.clear();

      try {
        this.proc.kill('SIGKILL');
      } catch {}
    }
  }

  /**
   * Force kill this worker (public, for timeout handling).
   */
  forceKill(): void {
    this.oomKilled = true; // Mark as needing restart
    this.killProcess();
  }

  /**
   * Clean up resources.
   */
  private cleanup(): void {
    if (this.memoryCheckTimer) {
      clearInterval(this.memoryCheckTimer);
      this.memoryCheckTimer = null;
    }
    this.proc = null;
    this.readline = null;
    this.ready = false;
  }

  /**
   * Stop the server process.
   */
  async stop(): Promise<void> {
    if (!this.proc || !this.ready) {
      this.cleanup();
      return;
    }

    try {
      await this.sendRequest({ type: 'shutdown', id: ++this.requestId });
    } catch {
      // Ignore errors during shutdown
    }

    // Force kill if still running
    if (this.proc) {
      try {
        this.proc.kill();
      } catch {}
    }

    this.cleanup();
  }

  /**
   * Check files and return error codes.
   */
  async check(files: Record<string, string>, options: CheckOptions = {}): Promise<CheckResult> {
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
  async status(): Promise<ServerStatus> {
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
  async recycle(): Promise<void> {
    if (!this.ready) {
      throw new Error('Server not ready');
    }

    const id = ++this.requestId;
    await this.sendRequest({ type: 'recycle', id });
  }

  private sendRequest(request: { type: string; id: number; [key: string]: any }): Promise<any> {
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
            if ((err as any).code === 'EPIPE' || (err as any).code === 'ERR_STREAM_DESTROYED') {
              this.oomKilled = true;
              resolve({ id: request.id, oom: true, error: 'Process was killed' });
            } else {
              reject(err);
            }
          }
        });
      } catch (err: any) {
        this.pending.delete(request.id);
        if (err.code === 'EPIPE' || err.code === 'ERR_STREAM_DESTROYED') {
          this.oomKilled = true;
          resolve({ id: request.id, oom: true, error: 'Process was killed' });
        } else {
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

  private handleResponse(line: string): void {
    try {
      const response = JSON.parse(line);
      const id = response.id;

      if (id !== undefined && this.pending.has(id)) {
        const callback = this.pending.get(id)!;
        this.pending.delete(id);
        callback(response);
      }
    } catch (err) {
      console.error('Failed to parse server response:', line);
    }
  }
}

/**
 * Pool of server clients for parallel test execution.
 * Automatically restarts workers that get OOM killed.
 */
export class TszServerPool {
  private clients: TszServerClient[] = [];
  private available: TszServerClient[] = [];
  private waiting: Array<(client: TszServerClient) => void> = [];
  private options: { serverPath?: string; libDir?: string; memoryLimitMB: number };
  private poolSize: number;

  // Stats
  public oomKills = 0;
  public totalRestarts = 0;

  constructor(size: number, options: { serverPath?: string; libDir?: string; memoryLimitMB: number }) {
    this.poolSize = size;
    this.options = options;
  }

  /**
   * Start all servers in the pool.
   */
  async start(): Promise<void> {
    const startPromises: Promise<void>[] = [];

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
  async stop(): Promise<void> {
    await Promise.all(this.clients.map(c => c.stop()));
    this.clients = [];
    this.available = [];
  }

  /**
   * Acquire a client from the pool.
   * If a client is dead (crashed or OOM), restart it first.
   */
  async acquire(): Promise<TszServerClient> {
    if (this.available.length > 0) {
      const client = this.available.pop()!;

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
  release(client: TszServerClient): void {
    // If client is dead (crashed or OOM), restart it asynchronously
    if (!client.isAlive) {
      this.restartAndRelease(client, client.isOomKilled);
      return;
    }

    if (this.waiting.length > 0) {
      const resolve = this.waiting.shift()!;
      resolve(client);
    } else {
      this.available.push(client);
    }
  }

  /**
   * Restart a dead client and release the new one.
   */
  private async restartAndRelease(deadClient: TszServerClient, wasOom: boolean): Promise<void> {
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
        const resolve = this.waiting.shift()!;
        resolve(newClient);
      } else {
        this.available.push(newClient);
      }
    } catch (err) {
      // Failed to start new client, try again later
      console.error('Failed to restart worker:', err);
    }
  }

  /**
   * Run a function with an acquired client.
   */
  async withClient<T>(fn: (client: TszServerClient) => Promise<T>): Promise<T> {
    const client = await this.acquire();
    try {
      return await fn(client);
    } finally {
      this.release(client);
    }
  }

  /**
   * Run a function with timeout. If timeout fires, kill the worker.
   * Returns { result, timedOut }.
   */
  async withClientTimeout<T>(
    fn: (client: TszServerClient) => Promise<T>,
    timeoutMs: number
  ): Promise<{ result?: T; timedOut: boolean }> {
    const client = await this.acquire();

    let timeoutId: NodeJS.Timeout;
    let timedOut = false;

    const timeoutPromise = new Promise<never>((_, reject) => {
      timeoutId = setTimeout(() => {
        timedOut = true;
        client.forceKill(); // Kill worker on timeout
        reject(new Error('timeout'));
      }, timeoutMs);
    });

    try {
      const result = await Promise.race([fn(client), timeoutPromise]);
      clearTimeout(timeoutId!);
      return { result, timedOut: false };
    } catch (err: any) {
      clearTimeout(timeoutId!);
      if (timedOut) {
        return { timedOut: true };
      }
      throw err;
    } finally {
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

// =============================================================================
// Server-mode Conformance Test Runner
// =============================================================================

interface ServerRunnerConfig {
  maxTests?: number;
  workers?: number;
  testTimeout?: number;
  verbose?: boolean;
  categories?: string[];
  memoryLimitMB?: number;
  filter?: string;
  printTest?: boolean;
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
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
};

function log(msg: string, color: string = '') {
  console.log(`${color}${msg}${colors.reset}`);
}

function formatNumber(n: number): string {
  return n.toLocaleString('en-US');
}

function formatMemory(mb: number): string {
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(1).replace(/\.0$/, '')}GB`;
  }
  return `${formatNumber(mb)}MB`;
}

/**
 * Print detailed information about a specific test.
 * Used with --print-test --filter=<pattern> for debugging.
 */
async function printTestDetails(
  testFile: string,
  relativePath: string,
  cacheEntries: Record<string, CacheEntry>,
  pool: TszServerPool,
  libDirs: string[]
): Promise<void> {
  const content = fs.readFileSync(testFile, 'utf-8');
  // Parse test case to handle @Filename directives for multi-file tests
  const parsed = parseTestCase(content, testFile);

  // For multi-file tests, write real files to temp directory
  const prepared = prepareTestFiles(parsed, testFile);
  const tempDir = prepared.tempDir;
  const files = prepared.files;

  const checkOptions = directivesToCheckOptions(parsed.directives, libDirs);
  const cacheEntry = cacheEntries[relativePath];
  const tscCodes = cacheEntry?.codes || [];

  log('\n' + '═'.repeat(70), colors.cyan);
  log(`  TEST: ${relativePath}`, colors.bold);
  log('═'.repeat(70), colors.cyan);

  // Print file content with line numbers
  log('\n📄 File Content:', colors.bold);
  log('─'.repeat(50), colors.dim);
  const lines = content.split('\n');
  lines.forEach((line, i) => {
    const lineNum = String(i + 1).padStart(3, ' ');
    log(`${colors.dim}${lineNum}│${colors.reset} ${line}`);
  });
  log('─'.repeat(50), colors.dim);

  // Print harness options (test control)
  log('\n🎛️  Harness Options:', colors.bold);
  const harnessEntries = Object.entries(parsed.harness).filter(([, v]) => v !== undefined);
  if (harnessEntries.length === 0) {
    log('  (none)', colors.dim);
  } else {
    for (const [key, value] of harnessEntries) {
      log(`  ${key}: ${JSON.stringify(value)}`, colors.magenta);
    }
  }

  // Print parsed compiler directives
  log('\n⚙️  Compiler Directives:', colors.bold);
  for (const [key, value] of Object.entries(parsed.directives)) {
    if (value !== undefined) {
      log(`  ${key}: ${JSON.stringify(value)}`, colors.yellow);
    }
  }

  // Check if test would be skipped
  const skipResult = shouldSkipTest(parsed.harness);
  if (skipResult.skip) {
    log(`\n⚠️  Test would be SKIPPED: ${skipResult.reason}`, colors.yellow);
  }

  // Print check options sent to server
  log('\n📤 Options sent to tsz-server:', colors.bold);
  log(`  ${JSON.stringify(checkOptions, null, 2).split('\n').join('\n  ')}`, colors.dim);

  // Run the check
  log('\n🔍 Running tsz check...', colors.bold);
  const { result, timedOut } = await pool.withClientTimeout(
    (client) => client.check(files, checkOptions),
    10000
  );

  if (timedOut) {
    log('  TIMEOUT', colors.red);
    return;
  }

  if (result?.oom) {
    log('  OOM', colors.red);
    return;
  }

  const tszCodes = result?.codes || [];

  // Print TSC expected errors
  log('\n📋 TSC Expected Errors (from cache):', colors.bold);
  if (tscCodes.length === 0) {
    log('  (none)', colors.green);
  } else {
    const grouped = tscCodes.reduce((acc, code) => {
      acc[code] = (acc[code] || 0) + 1;
      return acc;
    }, {} as Record<number, number>);
    for (const [code, count] of Object.entries(grouped)) {
      log(`  TS${code}: ${count}x`, colors.yellow);
    }
  }

  // Print tsz actual errors
  log('\n🔧 tsz Actual Errors:', colors.bold);
  if (tszCodes.length === 0) {
    log('  (none)', colors.green);
  } else {
    const grouped = tszCodes.reduce((acc, code) => {
      acc[code] = (acc[code] || 0) + 1;
      return acc;
    }, {} as Record<number, number>);
    for (const [code, count] of Object.entries(grouped)) {
      log(`  TS${code}: ${count}x`, colors.yellow);
    }
  }

  // Compare
  const tscSet = new Set(tscCodes);
  const tszSet = new Set(tszCodes);
  const missing = tscCodes.filter(c => !tszSet.has(c));
  const extra = tszCodes.filter(c => !tscSet.has(c));

  log('\n📊 Comparison:', colors.bold);
  if (missing.length === 0 && extra.length === 0) {
    log('  PASS - Exact match!', colors.green);
  } else {
    log('  FAIL', colors.red);
    if (missing.length > 0) {
      const grouped = missing.reduce((acc, code) => {
        acc[code] = (acc[code] || 0) + 1;
        return acc;
      }, {} as Record<number, number>);
      log('\n  Missing (tsz should emit but doesn\'t):', colors.yellow);
      for (const [code, count] of Object.entries(grouped)) {
        log(`    TS${code}: ${count}x`, colors.yellow);
      }
    }
    if (extra.length > 0) {
      const grouped = extra.reduce((acc, code) => {
        acc[code] = (acc[code] || 0) + 1;
        return acc;
      }, {} as Record<number, number>);
      log('\n  Extra (tsz emits but shouldn\'t):', colors.red);
      for (const [code, count] of Object.entries(grouped)) {
        log(`    TS${code}: ${count}x`, colors.red);
      }
    }
  }

  // Clean up temp directory for multi-file tests
  cleanupTempDir(tempDir);

  log('\n' + '═'.repeat(70) + '\n', colors.cyan);
}

/**
 * Run conformance tests using tsz-server (persistent process).
 *
 * This is 5-10x faster than spawn-per-test mode because:
 * - TypeScript libs are cached in memory
 * - No process spawn overhead per test
 * - Type interner is reused
 */
export async function runServerConformanceTests(config: ServerRunnerConfig = {}): Promise<TestStats> {
  const __dirname = path.dirname(new URL(import.meta.url).pathname);
  const ROOT_DIR = path.resolve(__dirname, '../..');

  const maxTests = config.maxTests ?? Infinity;
  const workerCount = config.workers ?? 8;
  const verbose = config.verbose ?? false;
  const categories = config.categories ?? ['conformance', 'compiler'];
  const testTimeout = config.testTimeout ?? 10000;
  const filter = config.filter;
  const printTest = config.printTest ?? false;
  // Calculate memory limit: 80% of total memory / number of workers
  const memoryLimitMB = config.memoryLimitMB ?? calculateMemoryLimitMB(workerCount);
  const totalMemoryMB = Math.round(os.totalmem() / 1024 / 1024);

  const testsBasePath = path.resolve(ROOT_DIR, 'TypeScript/tests/cases');
  const serverPath = process.env.TSZ_SERVER_BINARY || path.resolve(ROOT_DIR, '.target/release/tsz-server');
  const localLibDir = process.env.TSZ_LIB_DIR || path.resolve(ROOT_DIR, 'TypeScript/src/lib');

  // Set lib directories for universal resolver (used by directivesToCheckOptions)
  libDirs = [
    localLibDir,
    path.resolve(ROOT_DIR, 'TypeScript/tests/lib'),
  ];

  // Load TSC cache
  const tscCache = loadTscCache(ROOT_DIR);
  const cacheEntries = tscCache?.entries || {};

  log(`\n${'═'.repeat(60)}`, colors.cyan);
  log(`  TSZ Server Mode Conformance Runner`, colors.bold);
  log(`${'═'.repeat(60)}`, colors.cyan);
  log(`  Server: ${serverPath}`, colors.dim);
  log(`  Workers: ${workerCount}`, colors.dim);
  log(`  Max tests: ${formatNumber(maxTests)}`, colors.dim);
  log(`  Timeout: dynamic (10x avg, ${INITIAL_TIMEOUT_MS}ms initial)`, colors.dim);
  log(`  Memory: ${formatMemory(memoryLimitMB)}/worker (${formatMemory(Math.round(memoryLimitMB * workerCount))} total, system: ${formatMemory(totalMemoryMB)})`, colors.dim);
  log(`${'═'.repeat(60)}\n`, colors.cyan);

  // Initialize stats
  const stats: TestStats = {
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

  // Discover test files
  let testFiles: string[] = [];
  for (const category of categories) {
    const categoryPath = path.join(testsBasePath, category);
    if (fs.existsSync(categoryPath)) {
      discoverTests(categoryPath, testFiles, maxTests - testFiles.length);
    }
  }

  // Apply filter if specified
  if (filter) {
    const filterLower = filter.toLowerCase();
    testFiles = testFiles.filter(f => f.toLowerCase().includes(filterLower));
    log(`Filter "${filter}": ${formatNumber(testFiles.length)} tests match\n`, colors.yellow);
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
        await printTestDetails(testFile, relativePath, cacheEntries, pool, libDirs);
      }
      
      if (testFiles.length > 10) {
        log(`\n(Showing first 10 of ${testFiles.length} matching tests. Use a more specific filter.)`, colors.yellow);
      }
    } finally {
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

    // Process tests in parallel
    const startTime = Date.now();
    let completed = 0;

    // Dynamic timeout tracking (10x average test time)
    let testTimeSum = 0;
    let testTimeCount = 0;
    let currentTimeout = INITIAL_TIMEOUT_MS;

    const getDynamicTimeout = (): number => {
      if (testTimeCount < 10) return INITIAL_TIMEOUT_MS; // Need samples first
      const avg = testTimeSum / testTimeCount;
      const dynamic = Math.round(avg * TIMEOUT_MULTIPLIER);
      return Math.max(MIN_TIMEOUT_MS, Math.min(MAX_TIMEOUT_MS, dynamic));
    };

    const recordTestTime = (ms: number): void => {
      testTimeSum += ms;
      testTimeCount++;
      // Update timeout every 50 tests for stability
      if (testTimeCount % 50 === 0) {
        currentTimeout = getDynamicTimeout();
      }
    };

    // Progress bar state (time-throttled for smooth updates)
    const PROGRESS_BAR_WIDTH = 30;
    const PROGRESS_UPDATE_MS = 50;
    let lastProgressUpdate = 0;
    let progressInterval: NodeJS.Timeout | null = null;
    let cachedPoolStats = { oomKills: 0, totalRestarts: 0 };

    const renderProgressBar = (current: number, total: number): string => {
      const percent = total > 0 ? current / total : 0;
      const filled = Math.round(percent * PROGRESS_BAR_WIDTH);
      const empty = PROGRESS_BAR_WIDTH - filled;
      return `${colors.green}${'█'.repeat(filled)}${colors.dim}${'░'.repeat(empty)}${colors.reset}`;
    };

    const updateProgress = (force = false): void => {
      if (verbose) return;
      try {
        const now = Date.now();
        if (!force && now - lastProgressUpdate < PROGRESS_UPDATE_MS) return;
        lastProgressUpdate = now;

        const elapsed = (now - startTime) / 1000;
        const rate = elapsed > 0 ? completed / elapsed : 0;
        const percent = testFiles.length > 0 ? ((completed / testFiles.length) * 100).toFixed(1) : '0.0';

        try { cachedPoolStats = pool.stats; } catch {}

        const statusParts: string[] = [];
        if (stats.crashed > 0) statusParts.push(`${colors.red}err:${stats.crashed}${colors.reset}`);
        if (cachedPoolStats.oomKills > 0) statusParts.push(`${colors.magenta}oom:${cachedPoolStats.oomKills}${colors.reset}`);
        if (stats.timedOut > 0) statusParts.push(`${colors.yellow}to:${stats.timedOut}${colors.reset}`);
        statusParts.push(`${colors.dim}t:${currentTimeout}ms${colors.reset}`);
        const status = statusParts.length > 0 ? ` ${statusParts.join(' ')}` : '';

        const bar = renderProgressBar(completed, testFiles.length);
        const line = `  ${bar} ${percent.padStart(5)}% | ${formatNumber(completed).padStart(6)}/${formatNumber(testFiles.length).padEnd(6)} | ${rate.toFixed(0).padStart(4)}/s${status}`;
        process.stdout.write(`\r${line.padEnd(100)}`);
      } catch {}
    };

    if (!verbose) {
      progressInterval = setInterval(() => updateProgress(), PROGRESS_UPDATE_MS);
      progressInterval.unref();
    }

    const runTest = async (testFile: string): Promise<void> => {
      const category = path.basename(path.dirname(testFile));
      const relativePath = path.relative(testsBasePath, testFile);

      if (!stats.byCategory[category]) {
        stats.byCategory[category] = { total: 0, passed: 0 };
      }
      stats.byCategory[category].total++;
      stats.total++;

      let tempDir: string | null = null;
      try {
        const content = fs.readFileSync(testFile, 'utf-8');
        // Parse test case to handle @Filename directives for multi-file tests
        const parsed = parseTestCase(content, testFile);

        // Check if test should be skipped based on harness options
        const skipResult = shouldSkipTest(parsed.harness);
        if (skipResult.skip) {
          stats.skipped++;
          if (verbose) log(`[skip] ${relativePath}: ${skipResult.reason}`, colors.dim);
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
          (checkOptions as any).currentDirectory = prepared.currentDirectory;
        }

        // Get TSC baseline from cache or skip
        const cacheEntry = cacheEntries[relativePath];
        const tscCodes = cacheEntry?.codes || [];

        // Run check via server with dynamic timeout
        const { result, timedOut } = await pool.withClientTimeout(
          (client) => client.check(files, checkOptions),
          currentTimeout
        );

        // Check for timeout
        if (timedOut) {
          stats.timedOut++;
          stats.failed++;
          stats.timedOutTests.push(relativePath);
          if (verbose) log(`[timeout] ${relativePath}`, colors.yellow);
          return;
        }

        // Check for OOM
        if (result!.oom) {
          stats.oom++;
          stats.failed++;
          stats.oomTests.push(relativePath);
          if (verbose) log(`[oom] ${relativePath}`, colors.yellow);
          return;
        }

        // Record test time for dynamic timeout calculation
        if (result!.elapsed_ms > 0) {
          recordTestTime(result!.elapsed_ms);
        }

        const wasmCodes = result!.codes;

        // Compare results
        const tscSet = new Set(tscCodes);
        const wasmSet = new Set(wasmCodes);

        const missing = tscCodes.filter(c => !wasmSet.has(c));
        const extra = wasmCodes.filter(c => !tscSet.has(c));

        if (missing.length === 0 && extra.length === 0) {
          stats.passed++;
          stats.byCategory[category].passed++;
          if (verbose) log(`✓ ${relativePath}`, colors.green);
        } else {
          stats.failed++;
          if (verbose) {
            log(`✗ ${relativePath}`, colors.red);
            if (missing.length > 0) log(`  Missing: ${missing.join(', ')}`, colors.yellow);
            if (extra.length > 0) log(`  Extra: ${extra.join(', ')}`, colors.yellow);
          }
          for (const code of missing) {
            stats.missingCodes.set(code, (stats.missingCodes.get(code) || 0) + 1);
          }
          for (const code of extra) {
            stats.extraCodes.set(code, (stats.extraCodes.get(code) || 0) + 1);
          }
        }
      } catch (err: any) {
        stats.crashed++;
        stats.crashedTests.push({ path: relativePath, error: err.message });
        if (verbose) log(`💥 ${relativePath}: ${err.message}`, colors.red);
      } finally {
        // Clean up temp directory for multi-file tests
        cleanupTempDir(tempDir);
      }

      completed++;
      if (!verbose && completed % 10 === 0) {
        const elapsed = (Date.now() - startTime) / 1000;
        const rate = completed / elapsed;
        const poolStats = pool.stats;
        const oomInfo = poolStats.oomKills > 0 ? ` | OOM: ${poolStats.oomKills}` : '';
        const timeoutInfo = stats.timedOut > 0 ? ` | TO: ${stats.timedOut}` : '';
        const crashInfo = stats.crashed > 0 ? ` | Crash: ${stats.crashed}` : '';
        process.stdout.write(`\r  Progress: ${formatNumber(completed)}/${formatNumber(testFiles.length)} (${rate.toFixed(0)}/s)${crashInfo}${oomInfo}${timeoutInfo}    `);
      }
    };

    // Run tests with concurrency limit
    const concurrency = workerCount;
    const queue = [...testFiles];
    const running: Promise<void>[] = [];

    while (queue.length > 0 || running.length > 0) {
      while (running.length < concurrency && queue.length > 0) {
        const testFile = queue.shift()!;
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

    // Print summary (reversed order: details first, pass rate last)
    log('\n' + '═'.repeat(60), colors.dim);
    log('CONFORMANCE TEST RESULTS', colors.bold);
    log('═'.repeat(60), colors.dim);

    // Calculate stats
    const actualFailed = stats.failed - stats.oom - stats.timedOut;
    const effectiveTotal = stats.total - stats.skipped;
    const passRate = effectiveTotal > 0 ? ((stats.passed / effectiveTotal) * 100).toFixed(1) : '0.0';

    // Top errors first
    log('\nTop Extra Errors:', colors.bold);
    for (const [c, n] of [...stats.extraCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
      log(`  TS${c}: ${n}x`, colors.yellow);
    }

    log('\nTop Missing Errors:', colors.bold);
    for (const [c, n] of [...stats.missingCodes.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
      log(`  TS${c}: ${n}x`, colors.yellow);
    }

    // Problematic tests
    if (stats.timedOutTests.length > 0) {
      log('\nTimed Out Tests:', colors.yellow);
      for (const t of stats.timedOutTests.slice(0, 5)) {
        log(`  ${t}`, colors.dim);
      }
      if (stats.timedOutTests.length > 5) {
        log(`  ... and ${stats.timedOutTests.length - 5} more`, colors.dim);
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

    // By Category
    log('\nBy Category:', colors.bold);
    for (const [cat, s] of Object.entries(stats.byCategory)) {
      const r = s.total > 0 ? ((s.passed / s.total) * 100).toFixed(1) : '0.0';
      log(`  ${cat}: ${s.passed}/${s.total} (${r}%)`, s.passed === s.total ? colors.green : colors.yellow);
    }

    // Worker stats
    log('\nWorker Health:', colors.bold);
    log(`  Spawned:   ${workerCount}`, colors.dim);
    log(`  Crashed:   ${poolStats.oomKills}`, poolStats.oomKills > 0 ? colors.red : colors.dim);
    log(`  Respawned: ${poolStats.totalRestarts}`, poolStats.totalRestarts > 0 ? colors.yellow : colors.dim);

    // Summary
    log('\nSummary:', colors.bold);
    log(`  Passed:   ${formatNumber(stats.passed)}`, colors.green);
    log(`  Failed:   ${formatNumber(actualFailed)}`, actualFailed > 0 ? colors.red : colors.dim);
    log(`  Skipped:  ${formatNumber(stats.skipped)}`, stats.skipped > 0 ? colors.dim : colors.dim);
    log(`  Crashed:  ${formatNumber(stats.crashed)}`, stats.crashed > 0 ? colors.red : colors.dim);
    log(`  OOM:      ${formatNumber(stats.oom)}`, stats.oom > 0 ? colors.magenta : colors.dim);
    log(`  Timeout:  ${formatNumber(stats.timedOut)}`, stats.timedOut > 0 ? colors.yellow : colors.dim);

    // Time and Pass Rate last
    log(`\nTime: ${elapsed.toFixed(1)}s (${(effectiveTotal / elapsed).toFixed(0)} tests/sec)`, colors.dim);
    log(`\nPass Rate: ${passRate}% (${formatNumber(stats.passed)}/${formatNumber(effectiveTotal)})`, stats.passed === effectiveTotal ? colors.green : colors.yellow);
    if (stats.skipped > 0) {
      log(`  (${formatNumber(stats.skipped)} tests skipped due to harness directives)`, colors.dim);
    }

    log('\n' + '═'.repeat(60), colors.dim);

  } finally {
    await pool.stop();
  }

  return stats;
}

function discoverTests(dir: string, results: string[], limit: number): void {
  if (results.length >= limit) return;

  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (results.length >= limit) break;

    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      discoverTests(fullPath, results, limit);
    } else if (entry.name.endsWith('.ts') && !entry.name.endsWith('.d.ts')) {
      results.push(fullPath);
    }
  }
}
