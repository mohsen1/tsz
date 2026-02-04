/**
 * CLI-based transpiler using native tsz binary
 *
 * Replaces WASM worker approach with CLI invocation to enable full type checking.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');

// CLI timeout in ms (same as worker timeout)
const CLI_TIMEOUT_MS = 400;

interface TranspileOptions {
  target: number;
  module: number;
  declaration?: boolean;
}

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
  private testsRun = 0;
  private tempDir: string;

  constructor() {
    this.tszPath = findTszBinary();
    this.tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tsz-emit-'));
  }

  /**
   * Transpile TypeScript source using the CLI
   */
  async transpile(
    source: string,
    target: number,
    module: number,
    declaration = false
  ): Promise<TranspileResult> {
    this.testsRun++;

    // Create temp file
    const testName = `test_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const inputFile = path.join(this.tempDir, `${testName}.ts`);

    try {
      // Write source to temp file
      fs.writeFileSync(inputFile, source, 'utf-8');

      // Build CLI args
      const targetArg = targetToCliArg(target);
      const moduleArg = moduleToCliArg(module);
      const args = [this.tszPath];

      if (declaration) {
        args.push('--declaration');
      }

      args.push('--target', targetArg);
      args.push('--module', moduleArg);
      args.push(inputFile);

      // Run CLI with timeout
      const start = Date.now();
      const output = execSync(args.join(' '), {
        cwd: this.tempDir,
        encoding: 'utf-8',
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: CLI_TIMEOUT_MS,
      });
      const elapsed = Date.now() - start;

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
      if (fs.existsSync(jsFile)) fs.unlinkSync(jsFile);
      if (dts && fs.existsSync(dtsFile)) fs.unlinkSync(dtsFile);

      return { js, dts };
    } catch (e) {
      // Handle timeout
      if (e instanceof Error && 'killed' in e && (e as any).signal === 'SIGTERM') {
        throw new Error('TIMEOUT');
      }
      throw e;
    } finally {
      // Clean up input file
      if (fs.existsSync(inputFile)) {
        fs.unlinkSync(inputFile);
      }
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
