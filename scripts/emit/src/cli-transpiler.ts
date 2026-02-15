/**
 * CLI-based transpiler using native tsz binary
 *
 * Replaces WASM worker approach with CLI invocation to enable full type checking.
 * Uses async execFile (no shell) for parallel execution support.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { execFile as execFileCb, execSync, type ChildProcess } from 'child_process';
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

interface SourceInputFile {
  name: string;
  content: string;
}

interface OutputPaths {
  jsPath: string;
  jsCandidates: string[];
  dtsPath: string;
  dtsCandidates: string[];
}

function dedupeUseStrictPreamble(text: string): string {
  const lines = text.split('\n');
  const out: string[] = [];
  let seen = false;
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === '"use strict";' || trimmed === "'use strict';") {
      if (!seen) {
        out.push(line);
        seen = true;
      }
      continue;
    }
    out.push(line);
  }
  return out.join('\n');
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
 * Find the tsz binary in common locations.
 * Preference order:
 * 1) TSZ_BIN env var (set by scripts/emit/run.sh)
 * 2) Local workspace targets
 * 3) PATH lookup
 */
function findTszBinary(): string {
  const envBin = process.env.TSZ_BIN;
  if (envBin && fs.existsSync(envBin)) {
    return envBin;
  }

  const possiblePaths = [
    path.join(ROOT_DIR, '.target/release/tsz'),
    path.join(ROOT_DIR, 'target/release/tsz'),
    tszInPath(),
  ].filter(Boolean) as string[];

  for (const binPath of possiblePaths) {
    if (fs.existsSync(binPath)) {
      return binPath;
    }
  }

  throw new Error('tsz binary not found. Build it with: CARGO_TARGET_DIR=.target cargo build --release -p tsz-cli --bin tsz');
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
  private activeChildren = new Set<ChildProcess>();

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
    options: {
      sourceFileName?: string;
      declaration?: boolean;
      alwaysStrict?: boolean;
      sourceMap?: boolean;
      inlineSourceMap?: boolean;
      downlevelIteration?: boolean;
      noEmitHelpers?: boolean;
      noEmitOnError?: boolean;
      importHelpers?: boolean;
      esModuleInterop?: boolean;
      useDefineForClassFields?: boolean;
      experimentalDecorators?: boolean;
      emitDecoratorMetadata?: boolean;
      jsx?: string;
      jsxFactory?: string;
      jsxFragmentFactory?: string;
      jsxImportSource?: string;
      moduleDetection?: string;
      outFile?: string;
      sourceFiles?: SourceInputFile[];
      expectedJsFileName?: string;
      expectedDtsFileName?: string;
    } = {}
  ): Promise<TranspileResult> {
    const {
      sourceFileName,
      declaration = false,
      alwaysStrict = false,
      sourceMap = false,
      inlineSourceMap = false,
      downlevelIteration = false,
      noEmitHelpers = false,
      noEmitOnError = false,
      importHelpers = false,
      esModuleInterop = false,
      useDefineForClassFields,
      experimentalDecorators = false,
      emitDecoratorMetadata = false,
      jsx,
      jsxFactory,
      jsxFragmentFactory,
      jsxImportSource,
      moduleDetection,
      outFile,
      sourceFiles,
      expectedJsFileName,
      expectedDtsFileName,
    } = options;
    const testName = `test_${this.counter++}`;
    const testDir = path.join(this.tempDir, testName);
    fs.mkdirSync(testDir, { recursive: true });

    const files: SourceInputFile[] = sourceFiles && sourceFiles.length > 0
      ? sourceFiles
      : [{
          name: sourceFileName ?? `${testName}.ts`,
          content: source,
        }];

    const inputFiles: string[] = [];
    const expectedOutputs: OutputPaths[] = [];

    for (const file of files) {
      const relName = file.name.replace(/^\/+/, '');
      const filePath = path.join(testDir, relName);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, file.content, 'utf-8');
      inputFiles.push(filePath);

      const extMatch = relName.match(/\.(ts|tsx|mts|cts)$/);
      const ext = extMatch ? `.${extMatch[1]}` : '.ts';
      const stem = filePath.replace(/\.(ts|tsx|mts|cts)$/, '');
      const sourceDefaultJsPath =
        ext === '.tsx' ? `${stem}.jsx` : ext === '.mts' ? `${stem}.mjs` : ext === '.cts' ? `${stem}.cjs` : `${stem}.js`;

      expectedOutputs.push({
        jsPath: sourceDefaultJsPath,
        jsCandidates: [
          sourceDefaultJsPath,
          `${stem}.js`,
          `${stem}.jsx`,
          `${stem}.mjs`,
          `${stem}.cjs`,
        ],
        dtsPath: `${stem}.d.ts`,
        dtsCandidates: [
          `${stem}.d.ts`,
          `${stem}.d.mts`,
          `${stem}.d.cts`,
        ],
      });
    }

    try {
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
      if (sourceMap) args.push('--sourceMap');
      if (inlineSourceMap) args.push('--inlineSourceMap');
      if (downlevelIteration) args.push('--downlevelIteration');
      if (noEmitHelpers) args.push('--noEmitHelpers');
      if (noEmitOnError) args.push('--noEmitOnError');
      if (importHelpers) args.push('--importHelpers');
      if (esModuleInterop) args.push('--esModuleInterop');
      if (useDefineForClassFields !== undefined) {
        args.push('--useDefineForClassFields', useDefineForClassFields ? 'true' : 'false');
      }
      if (experimentalDecorators) args.push('--experimentalDecorators');
      if (emitDecoratorMetadata) args.push('--emitDecoratorMetadata');
      if (jsx) args.push('--jsx', jsx);
      if (jsxFactory) args.push('--jsxFactory', jsxFactory);
      if (jsxFragmentFactory) args.push('--jsxFragmentFactory', jsxFragmentFactory);
      if (jsxImportSource) args.push('--jsxImportSource', jsxImportSource);
      if (moduleDetection) args.push('--moduleDetection', moduleDetection);
      if (outFile) args.push('--outFile', outFile);
      const trailingArgs = ['--target', targetArg, '--module', moduleArg, ...inputFiles];
      args.push(...trailingArgs);

      // Run CLI asynchronously without shell overhead.
      // Use SIGKILL for timeout so the child can't ignore the signal and linger.
      const runWithArgs = async (cliArgs: string[]) => {
        const promise = execFile(this.tszPath, cliArgs, {
          cwd: this.tempDir,
          encoding: 'utf-8',
          timeout: this.timeoutMs,
          killSignal: 'SIGKILL',
        });
        const child = promise.child;
        this.activeChildren.add(child);
        child.on('exit', () => this.activeChildren.delete(child));
        return await promise;
      };

      try {
        await runWithArgs(args);
      } catch (e) {
        const errorMsg = e instanceof Error ? e.message : String(e);
        // Declaration mode can fail on unresolved imports in isolated baseline snippets.
        // Retry with noCheck/noLib to still exercise declaration printer paths.
        const shouldRetryDeclarationFastPath =
          declaration &&
          (errorMsg.includes('TS2307') || errorMsg.includes('TS2304'));
        if (!shouldRetryDeclarationFastPath) {
          // Match tsc behavior: diagnostics can still produce outputs (exit code 2).
          // For JS-only emit mode, continue if JS output was generated.
          const hasJsOutput =
            (expectedJsFileName ? fs.existsSync(path.join(testDir, expectedJsFileName)) : false) ||
            expectedOutputs.some(o => o.jsCandidates.some(candidate => fs.existsSync(candidate)));
          const hasDtsOutput =
            (expectedDtsFileName ? fs.existsSync(path.join(testDir, expectedDtsFileName)) : false) ||
            expectedOutputs.some(o => o.dtsCandidates.some(candidate => fs.existsSync(candidate)));
          if (!declaration && hasJsOutput) {
            // continue
          } else if (declaration && hasDtsOutput) {
            // declaration emit produced output despite diagnostics
          } else {
            throw e;
          }
        } else {
          const retryArgs = ['--declaration', '--noCheck', '--noLib'];
          if (alwaysStrict) retryArgs.push('--alwaysStrict', 'true');
          if (sourceMap) retryArgs.push('--sourceMap');
          if (inlineSourceMap) retryArgs.push('--inlineSourceMap');
          if (downlevelIteration) retryArgs.push('--downlevelIteration');
          if (noEmitHelpers) retryArgs.push('--noEmitHelpers');
          if (noEmitOnError) retryArgs.push('--noEmitOnError');
          if (importHelpers) retryArgs.push('--importHelpers');
          if (esModuleInterop) retryArgs.push('--esModuleInterop');
          if (useDefineForClassFields !== undefined) {
            retryArgs.push('--useDefineForClassFields', useDefineForClassFields ? 'true' : 'false');
          }
          if (experimentalDecorators) retryArgs.push('--experimentalDecorators');
          if (emitDecoratorMetadata) retryArgs.push('--emitDecoratorMetadata');
          if (jsx) retryArgs.push('--jsx', jsx);
          if (jsxFactory) retryArgs.push('--jsxFactory', jsxFactory);
          if (jsxFragmentFactory) retryArgs.push('--jsxFragmentFactory', jsxFragmentFactory);
          if (jsxImportSource) retryArgs.push('--jsxImportSource', jsxImportSource);
          if (moduleDetection) retryArgs.push('--moduleDetection', moduleDetection);
          if (outFile) retryArgs.push('--outFile', outFile);
          retryArgs.push(...trailingArgs);
          await runWithArgs(retryArgs);
        }
      }

      // Read output files
      let js = '';
      let dts: string | null = null;

      const readNamedOutput = (name: string | undefined, dtsMode: boolean): string | null => {
        if (!name) return null;
        const outPath = path.join(testDir, name);
        if (!fs.existsSync(outPath)) return null;
        const content = fs.readFileSync(outPath, 'utf-8');
        return dtsMode ? content : content;
      };

      const readFirstExisting = (candidates: string[]): string | null => {
        for (const candidate of candidates) {
          if (fs.existsSync(candidate)) {
            return fs.readFileSync(candidate, 'utf-8');
          }
        }
        return null;
      };

      const namedJs = readNamedOutput(expectedJsFileName, false);
      if (namedJs !== null) {
        js = namedJs;
      } else {
        const chunks: string[] = [];
        let sawUseStrict = false;
        for (const out of expectedOutputs) {
          const chunkContent = readFirstExisting(out.jsCandidates);
          if (chunkContent !== null) {
            let chunk = chunkContent;
            const strictPrefix = /^\s*["']use strict["'];\s*/;
            if (sawUseStrict) {
              chunk = chunk.replace(strictPrefix, '');
            } else if (strictPrefix.test(chunk)) {
              sawUseStrict = true;
            }
            chunks.push(chunk);
          }
        }
        js = chunks.join('');
      }
      js = dedupeUseStrictPreamble(js);

      if (declaration) {
        const namedDts = readNamedOutput(expectedDtsFileName, true);
        if (namedDts !== null) {
          dts = namedDts;
        } else {
          const dtsChunks: string[] = [];
          for (const out of expectedOutputs) {
            const dtsChunk = readFirstExisting(out.dtsCandidates);
            if (dtsChunk !== null) {
              dtsChunks.push(dtsChunk);
            }
          }
          dts = dtsChunks.length > 0 ? dtsChunks.join('') : null;
        }
      }

      return { js, dts };
    } catch (e) {
      // Handle timeout (execFile sends SIGKILL on timeout)
      if (e instanceof Error && 'killed' in e && ((e as any).signal === 'SIGKILL' || (e as any).signal === 'SIGTERM')) {
        throw new Error('TIMEOUT');
      }
      throw e;
    } finally {
      try { fs.rmSync(testDir, { recursive: true, force: true }); } catch {}
    }
  }

  /**
   * Kill all in-flight child processes and clean up temp directory.
   */
  terminate(): void {
    for (const child of this.activeChildren) {
      try { child.kill('SIGKILL'); } catch {}
    }
    this.activeChildren.clear();

    if (fs.existsSync(this.tempDir)) {
      fs.rmSync(this.tempDir, { recursive: true, force: true });
    }
  }
}
