/**
 * CLI-based transpiler using native tsz binary
 *
 * Replaces WASM worker approach with CLI invocation to enable full type checking.
 * Uses async execFile (no shell) for parallel execution support.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { execFile as execFileCb, execSync } from 'child_process';
import { promisify } from 'util';
import { fileURLToPath } from 'url';

const execFile = promisify(execFileCb);

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');

// Default CLI timeout in ms
const DEFAULT_TIMEOUT_MS = 5000;

interface TranspileResult {
  js: string;
  dts?: string | null;
}

// Convert target number to CLI arg
function targetToCliArg(target: number): string {
  const targets: Record<number, string> = {
    0: 'es3',
    1: 'es5',
    2: 'es2015',
    3: 'es2016',
    4: 'es2017',
    5: 'es2018',
    6: 'es2019',
    7: 'es2020',
    8: 'es2021',
    9: 'es2022',
    99: 'esnext',
  };
  return targets[target] || 'es5';
}

// Convert module number to CLI arg
function moduleToCliArg(module: number): string {
  const modules: Record<number, string> = {
    0: 'none',
    1: 'commonjs',
    2: 'amd',
    3: 'umd',
    4: 'system',
    5: 'es2015',
    6: 'es2020',
    7: 'es2022',
    99: 'esnext',
    100: 'node16',
    199: 'nodenext',
  };
  return modules[module] || 'none';
}

/**
 * Find the tsz binary in common locations
 */
function findTszBinary(): string {
  const possiblePaths = [
    path.join(ROOT_DIR, '.target/release/tsz'), // Local build (uses .target from .cargo/config.toml)
    '/Users/mohsenazimi/.cargo/bin/tsz',       // User cargo bin (from which)
    tszInPath(),                                 // Global installation
  ].filter(Boolean);

  for (const binPath of possiblePaths) {
    if (binPath && fs.existsSync(binPath)) {
      return binPath;
    }
  }

  throw new Error('tsz binary not found. Run: cargo build --release');
}

function tszInPath(): string | null {
  try {
    const whichResult = execSync('which tsz', { encoding: 'utf-8', stdio: ['pipe', 'pipe', 'ignore'] }).trim();
    return whichResult || null;
  } catch {
    return null;
  }
}

/**
 * CLI-based transpiler (replaces WASM worker)
 */
export class CliTranspiler {
  private tszPath: string;
  private counter = 0;
  private tempDir: string;
  private timeoutMs: number;

  constructor(timeoutMs: number = DEFAULT_TIMEOUT_MS) {
    this.tszPath = findTszBinary();
    this.tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tsz-emit-'));
    this.timeoutMs = timeoutMs;
  }

  /**
   * Transpile TypeScript source using the CLI.
   * Uses async execFile (no shell) for parallel-safe execution.
   */
  async transpile(
    source: string,
    target: number,
    module: number,
    options: { declaration?: boolean; alwaysStrict?: boolean } = {}
  ): Promise<TranspileResult> {
    const { declaration = false, alwaysStrict = false } = options;
    const testName = `test_${this.counter++}`;
    const inputFile = path.join(this.tempDir, `${testName}.ts`);

    try {
      fs.writeFileSync(inputFile, source, 'utf-8');

      const targetArg = targetToCliArg(target);
      const moduleArg = moduleToCliArg(module);

      // Build args array (no shell parsing needed with execFile)
      const args: string[] = [];
      if (declaration) args.push('--declaration');
      // Skip type checking and lib loading for JS-only emit -- the emitter
      // only needs syntax. Type checking accounts for ~77% of per-test time
      // and lib loading accounts for another ~50% of the remainder.
      if (!declaration) {
        args.push('--noCheck', '--noLib');
      }
      if (alwaysStrict) args.push('--alwaysStrict', 'true');
      args.push('--target', targetArg, '--module', moduleArg, inputFile);

      // Run CLI asynchronously without shell overhead
      await execFile(this.tszPath, args, {
        cwd: this.tempDir,
        encoding: 'utf-8',
        timeout: this.timeoutMs,
      });

      // Read output files
      const jsFile = inputFile.replace('.ts', '.js');
      const dtsFile = inputFile.replace('.ts', '.d.ts');

      let js = '';
      let dts: string | null = null;

      if (fs.existsSync(jsFile)) {
        js = fs.readFileSync(jsFile, 'utf-8');
      }

      if (declaration && fs.existsSync(dtsFile)) {
        dts = fs.readFileSync(dtsFile, 'utf-8');
      }

      // Clean up output files
      try { fs.unlinkSync(jsFile); } catch {}
      try { fs.unlinkSync(dtsFile); } catch {}

      return { js, dts };
    } catch (e) {
      // Handle timeout (execFile sends SIGTERM on timeout)
      if (e instanceof Error && 'killed' in e && (e as any).signal === 'SIGTERM') {
        throw new Error('TIMEOUT');
      }
      throw e;
    } finally {
      try { fs.unlinkSync(inputFile); } catch {}
    }
  }

  /**
   * Clean up temp directory
   */
  terminate(): void {
    if (fs.existsSync(this.tempDir)) {
      fs.rmSync(this.tempDir, { recursive: true, force: true });
    }
  }
}
