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
import { type CheckOptions } from './test-utils.js';
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
/**
 * Client for tsz-server.
 *
 * Manages a persistent tsz-server process and sends check requests.
 * Monitors memory usage and kills the process if it exceeds the limit.
 */
export declare class TszServerClient {
    private proc;
    private readline;
    private requestId;
    private pending;
    private ready;
    private readyPromise;
    private serverPath;
    private libDir;
    private memoryLimitMB;
    private memoryCheckTimer;
    private consecutiveViolations;
    private oomKilled;
    private checksCompleted;
    constructor(options: {
        serverPath?: string;
        libDir?: string;
        memoryLimitMB: number;
    });
    get isOomKilled(): boolean;
    get isAlive(): boolean;
    get pid(): number | undefined;
    /**
     * Start the server process.
     */
    start(): Promise<void>;
    /**
     * Start monitoring memory usage.
     * Uses a "strike" system: requires 3 consecutive violations before killing
     * to avoid false positives from transient memory spikes.
     */
    private startMemoryMonitor;
    /**
     * Kill the server process (internal).
     */
    private killProcess;
    /**
     * Force kill this worker (public, for timeout handling).
     */
    forceKill(): void;
    /**
     * Clean up resources.
     */
    private cleanup;
    /**
     * Stop the server process.
     */
    stop(): Promise<void>;
    /**
     * Check files and return error codes.
     */
    check(files: Record<string, string>, options?: CheckOptions): Promise<CheckResult>;
    /**
     * Get server status.
     */
    status(): Promise<ServerStatus>;
    /**
     * Recycle server (clear caches).
     */
    recycle(): Promise<void>;
    private sendRequest;
    private handleResponse;
}
/**
 * Pool of server clients for parallel test execution.
 * Automatically restarts workers that get OOM killed.
 */
export declare class TszServerPool {
    private clients;
    private available;
    private waiting;
    private options;
    private poolSize;
    oomKills: number;
    totalRestarts: number;
    constructor(size: number, options: {
        serverPath?: string;
        libDir?: string;
        memoryLimitMB: number;
    });
    /**
     * Start all servers in the pool.
     */
    start(): Promise<void>;
    /**
     * Stop all servers in the pool.
     */
    stop(): Promise<void>;
    /**
     * Acquire a client from the pool.
     * If a client is dead (crashed or OOM), restart it first.
     */
    acquire(): Promise<TszServerClient>;
    /**
     * Release a client back to the pool.
     * If client is dead, restart it before handing to waiters.
     */
    release(client: TszServerClient): void;
    /**
     * Restart a dead client and release the new one.
     */
    private restartAndRelease;
    /**
     * Run a function with an acquired client.
     */
    withClient<T>(fn: (client: TszServerClient) => Promise<T>): Promise<T>;
    /**
     * Run a function with timeout. If timeout fires, kill the worker.
     * Returns { result, timedOut }.
     */
    withClientTimeout<T>(fn: (client: TszServerClient) => Promise<T>, timeoutMs: number): Promise<{
        result?: T;
        timedOut: boolean;
    }>;
    /**
     * Get pool statistics.
     */
    get stats(): {
        total: number;
        available: number;
        waiting: number;
        oomKills: number;
        totalRestarts: number;
    };
}
export default TszServerClient;
interface ServerRunnerConfig {
    maxTests?: number;
    workers?: number;
    testTimeout?: number;
    verbose?: boolean;
    categories?: string[];
    memoryLimitMB?: number;
    filter?: string;
    errorCode?: number;
    printTest?: boolean;
    dumpResults?: string;
}
interface TestStats {
    total: number;
    passed: number;
    failed: number;
    crashed: number;
    skipped: number;
    timedOut: number;
    oom: number;
    byCategory: Record<string, {
        total: number;
        passed: number;
    }>;
    missingCodes: Map<number, number>;
    extraCodes: Map<number, number>;
    crashedTests: {
        path: string;
        error: string;
    }[];
    oomTests: string[];
    timedOutTests: string[];
    workerStats: {
        spawned: number;
        crashed: number;
        respawned: number;
    };
}
/**
 * Run conformance tests using tsz-server (persistent process).
 *
 * This is 5-10x faster than spawn-per-test mode because:
 * - TypeScript libs are cached in memory
 * - No process spawn overhead per test
 * - Type interner is reused
 */
export declare function runServerConformanceTests(config?: ServerRunnerConfig): Promise<TestStats>;
//# sourceMappingURL=runner-server.d.ts.map