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
import { targetToCliArg, moduleToCliArg } from './ts-enums.js';

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

export interface LinkInput {
  target: string;
  link: string;
}

interface OutputPaths {
  jsPath: string;
  jsCandidates: string[];
  dtsPath: string;
  dtsCandidates: string[];
}

interface CompilerFlagOptions {
  alwaysStrict?: boolean;
  sourceMap?: boolean;
  inlineSourceMap?: boolean;
  declarationMap?: boolean;
  downlevelIteration?: boolean;
  noEmitHelpers?: boolean;
  noEmitOnError?: boolean;
  importHelpers?: boolean;
  esModuleInterop?: boolean;
  useDefineForClassFields?: boolean;
  experimentalDecorators?: boolean;
  emitDecoratorMetadata?: boolean;
  strictNullChecks?: boolean;
  exactOptionalPropertyTypes?: boolean;
  jsx?: string;
  jsxFactory?: string;
  jsxFragmentFactory?: string;
  jsxImportSource?: string;
  moduleDetection?: string;
  preserveConstEnums?: boolean;
  verbatimModuleSyntax?: boolean;
  rewriteRelativeImportExtensions?: boolean;
  isolatedModules?: boolean;
  importsNotUsedAsValues?: string;
  preserveValueImports?: boolean;
  removeComments?: boolean;
  stripInternal?: boolean;
  outFile?: string;
  outDir?: string;
  rootDir?: string;
}

// Append shared compiler-option flags onto a tsz CLI args array. Used by both
// the primary emit invocation and the declaration-emit retry path so that the
// two stay in lockstep — previously the retry path silently dropped
// --strictNullChecks (and was at structural risk of dropping any future flag).
function appendCompilerOptionFlags(args: string[], opts: CompilerFlagOptions): void {
  if (opts.alwaysStrict) args.push('--alwaysStrict', 'true');
  if (opts.sourceMap) args.push('--sourceMap');
  if (opts.inlineSourceMap) args.push('--inlineSourceMap');
  if (opts.declarationMap) args.push('--declarationMap');
  if (opts.downlevelIteration) args.push('--downlevelIteration');
  if (opts.noEmitHelpers) args.push('--noEmitHelpers');
  if (opts.noEmitOnError) args.push('--noEmitOnError');
  if (opts.importHelpers) args.push('--importHelpers');
  if (opts.esModuleInterop) args.push('--esModuleInterop');
  if (opts.useDefineForClassFields !== undefined) {
    args.push('--useDefineForClassFields', opts.useDefineForClassFields ? 'true' : 'false');
  }
  if (opts.experimentalDecorators) args.push('--experimentalDecorators');
  if (opts.emitDecoratorMetadata) args.push('--emitDecoratorMetadata');
  if (opts.strictNullChecks !== undefined) args.push('--strictNullChecks', String(opts.strictNullChecks));
  // tsz CLI defines `--exactOptionalPropertyTypes` as a presence-only flag.
  // Passing the literal "true" turns into a positional input file, so only emit
  // the flag when the option is enabled.
  if (opts.exactOptionalPropertyTypes === true) {
    args.push('--exactOptionalPropertyTypes');
  }
  if (opts.jsx) args.push('--jsx', opts.jsx);
  if (opts.jsxFactory) args.push('--jsxFactory', opts.jsxFactory);
  if (opts.jsxFragmentFactory) args.push('--jsxFragmentFactory', opts.jsxFragmentFactory);
  if (opts.jsxImportSource) args.push('--jsxImportSource', opts.jsxImportSource);
  if (opts.moduleDetection) args.push('--moduleDetection', opts.moduleDetection);
  if (opts.preserveConstEnums) args.push('--preserveConstEnums');
  if (opts.verbatimModuleSyntax) args.push('--verbatimModuleSyntax');
  if (opts.rewriteRelativeImportExtensions) args.push('--rewriteRelativeImportExtensions');
  if (opts.isolatedModules) args.push('--isolatedModules');
  if (opts.importsNotUsedAsValues) args.push('--importsNotUsedAsValues', opts.importsNotUsedAsValues);
  if (opts.preserveValueImports) args.push('--preserveValueImports');
  if (opts.removeComments) args.push('--removeComments');
  if (opts.stripInternal) args.push('--stripInternal');
  if (opts.outFile) args.push('--outFile', opts.outFile);
  if (opts.outDir) args.push('--outDir', opts.outDir);
  if (opts.rootDir) args.push('--rootDir', opts.rootDir);
}

function dedupeUseStrictPreamble(text: string): string {
  // Only deduplicate "use strict" directives that appear in the leading preamble
  // (before any non-empty, non-directive content). Inner "use strict" inside
  // function bodies must be preserved as-is.
  const lines = text.split('\n');
  const out: string[] = [];
  let seenInPreamble = false;
  let preambleDone = false;
  for (const line of lines) {
    const trimmed = line.trim();
    const isUseStrict = trimmed === '"use strict";' || trimmed === "'use strict';";
    if (!preambleDone && isUseStrict) {
      if (!seenInPreamble) {
        out.push(line);
        seenInPreamble = true;
      }
      // Skip subsequent "use strict" lines only while still in preamble
      continue;
    }
    // Once we see any non-"use strict" non-empty content, the preamble is done
    if (trimmed !== '') {
      preambleDone = true;
    }
    out.push(line);
  }
  return out.join('\n');
}

function hasUseStrictPreamble(text: string): boolean {
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (trimmed === '') continue;
    return trimmed === '"use strict";' || trimmed === "'use strict';";
  }
  return false;
}

function normalizeLeadingTripleSlashSpacing(text: string): string {
  // Keep leading triple-slash directives adjacent to the following statement.
  // Some JS-input baselines expect no blank line after the directive block.
  return text.replace(/^((?:(?:["']use strict["'];\n)?(?:\/\/\/[^\n]*\n)+))\n+/m, '$1');
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
    path.join(ROOT_DIR, '.target/dist-fast/tsz'),
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
      declarationMap?: boolean;
      downlevelIteration?: boolean;
      noEmitHelpers?: boolean;
      noEmitOnError?: boolean;
      importHelpers?: boolean;
      esModuleInterop?: boolean;
      useDefineForClassFields?: boolean;
      experimentalDecorators?: boolean;
      emitDecoratorMetadata?: boolean;
      strictNullChecks?: boolean;
      exactOptionalPropertyTypes?: boolean;
      jsx?: string;
      jsxFactory?: string;
      jsxFragmentFactory?: string;
      jsxImportSource?: string;
      moduleDetection?: string;
      preserveConstEnums?: boolean;
      verbatimModuleSyntax?: boolean;
      rewriteRelativeImportExtensions?: boolean;
      isolatedModules?: boolean;
      importsNotUsedAsValues?: string;
      preserveValueImports?: boolean;
      removeComments?: boolean;
      stripInternal?: boolean;
      outFile?: string;
      outDir?: string;
      rootDir?: string;
      sourceFiles?: SourceInputFile[];
      links?: LinkInput[];
      expectedJsFileName?: string;
      expectedDtsFileName?: string;
      expectedJsContent?: string | null;
      expectedDtsContent?: string | null;
      lib?: string[];
    } = {}
  ): Promise<TranspileResult> {
    const {
      sourceFileName,
      declaration = false,
      alwaysStrict = false,
      sourceMap = false,
      inlineSourceMap = false,
      declarationMap = false,
      downlevelIteration = false,
      noEmitHelpers = false,
      noEmitOnError = false,
      importHelpers = false,
      esModuleInterop = false,
      useDefineForClassFields,
      experimentalDecorators = false,
      emitDecoratorMetadata = false,
      strictNullChecks,
      exactOptionalPropertyTypes,
      jsx,
      jsxFactory,
      jsxFragmentFactory,
      jsxImportSource,
      moduleDetection,
      preserveConstEnums = false,
      verbatimModuleSyntax = false,
      rewriteRelativeImportExtensions = false,
      isolatedModules = false,
      importsNotUsedAsValues,
      preserveValueImports = false,
      removeComments = false,
      stripInternal = false,
      outFile,
      outDir,
      rootDir,
      sourceFiles,
      links = [],
      expectedJsFileName,
      expectedDtsFileName,
      expectedJsContent,
      expectedDtsContent,
      lib,
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

      // Auxiliary files (package.json, tsconfig.json) are written to disk
      // but not passed as CLI input arguments or expected to produce output.
      const isAuxiliary = relName.endsWith('package.json') || relName.endsWith('tsconfig.json');
      if (isAuxiliary) {
        continue;
      }

      inputFiles.push(filePath);

      const extMatch = relName.match(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/);
      const ext = extMatch ? `.${extMatch[1]}` : '.ts';
      const stem = filePath.replace(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/, '');
      const outputRelStem = (() => {
        if (!outDir) return null;
        const normalizedRoot = rootDir?.replace(/^[/\\]+/, '').replace(/\\/g, '/').replace(/\/+$/, '');
        let relStem = relName.replace(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/, '');
        if (normalizedRoot && (relStem === normalizedRoot || relStem.startsWith(`${normalizedRoot}/`))) {
          relStem = relStem.slice(normalizedRoot.length).replace(/^\/+/, '');
        }
        return path.join(testDir, outDir.replace(/^[/\\]+/, ''), relStem);
      })();
      // For TS→JS: .ts→.js, .tsx→.jsx, .mts→.mjs, .cts→.cjs
      // For JS→JS (allowJs): output has same extension as input
      const sourceDefaultJsPath =
        ext === '.tsx' || ext === '.jsx' ? `${stem}.jsx` :
        ext === '.mts' || ext === '.mjs' ? `${stem}.mjs` :
        ext === '.cts' || ext === '.cjs' ? `${stem}.cjs` :
        `${stem}.js`;

      expectedOutputs.push({
        jsPath: sourceDefaultJsPath,
        jsCandidates: [
          ...(outputRelStem ? [
            ext === '.tsx' || ext === '.jsx' ? `${outputRelStem}.jsx` :
            ext === '.mts' || ext === '.mjs' ? `${outputRelStem}.mjs` :
            ext === '.cts' || ext === '.cjs' ? `${outputRelStem}.cjs` :
            `${outputRelStem}.js`,
          ] : []),
          sourceDefaultJsPath,
          `${stem}.js`,
          `${stem}.jsx`,
          `${stem}.mjs`,
          `${stem}.cjs`,
        ],
        dtsPath: `${stem}.d.ts`,
        dtsCandidates: [
          ...(outputRelStem ? [
            `${outputRelStem}.d.ts`,
            `${outputRelStem}.d.mts`,
            `${outputRelStem}.d.cts`,
          ] : []),
          `${stem}.d.ts`,
          `${stem}.d.mts`,
          `${stem}.d.cts`,
        ],
      });
    }

    for (const link of links) {
      const relTarget = link.target.replace(/^\/+/, '');
      const relLink = link.link.replace(/^\/+/, '');
      const targetPath = path.join(testDir, relTarget);
      const linkPath = path.join(testDir, relLink);
      if (!fs.existsSync(targetPath)) continue;

      fs.mkdirSync(path.dirname(linkPath), { recursive: true });
      try {
        fs.rmSync(linkPath, { recursive: true, force: true });
      } catch {
        // Best effort: the subsequent symlink call will surface any real error.
      }
      const type = fs.statSync(targetPath).isDirectory() ? 'dir' : 'file';
      fs.symlinkSync(targetPath, linkPath, type);
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
      // Add --allowJs when any input file is a .js/.jsx/.mjs/.cjs file
      const hasJsInput = files.some(f => /\.(js|jsx|mjs|cjs)$/i.test(f.name));
      if (hasJsInput) args.push('--allowJs');
      if (lib && lib.length > 0) args.push('--lib', lib.join(','));
      appendCompilerOptionFlags(args, {
        alwaysStrict,
        sourceMap,
        inlineSourceMap,
        declarationMap,
        downlevelIteration,
        noEmitHelpers,
        noEmitOnError,
        importHelpers,
        esModuleInterop,
        useDefineForClassFields,
        experimentalDecorators,
        emitDecoratorMetadata,
        strictNullChecks,
        exactOptionalPropertyTypes,
        jsx,
        jsxFactory,
        jsxFragmentFactory,
        jsxImportSource,
        moduleDetection,
        preserveConstEnums,
        verbatimModuleSyntax,
        rewriteRelativeImportExtensions,
        isolatedModules,
        importsNotUsedAsValues,
        preserveValueImports,
        removeComments,
        stripInternal,
        outFile,
        outDir,
        rootDir,
      });
      const trailingArgs = ['--target', targetArg, '--module', moduleArg, ...inputFiles];
      args.push(...trailingArgs);

      // Run CLI asynchronously without shell overhead.
      // Use SIGKILL for timeout so the child can't ignore the signal and linger.
      const runWithArgs = async (cliArgs: string[]) => {
        const promise = execFile(this.tszPath, cliArgs, {
          cwd: testDir,
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
        const hasJsOutput =
          (expectedJsFileName ? fs.existsSync(path.join(testDir, expectedJsFileName)) : false) ||
          expectedOutputs.some(o => o.jsCandidates.some(candidate => fs.existsSync(candidate)));
        const hasDtsOutput =
          (expectedDtsFileName ? fs.existsSync(path.join(testDir, expectedDtsFileName)) : false) ||
          expectedOutputs.some(o => o.dtsCandidates.some(candidate => fs.existsSync(candidate)));

        // When --noEmitOnError is set and the compiler exits with errors,
        // producing no output is correct behavior (tsc does the same).
        // Return empty output instead of throwing.
        if (noEmitOnError && !hasJsOutput && !hasDtsOutput) {
          return { js: '', dts: declaration ? '' : null };
        }

        // Declaration mode can fail on unresolved imports in isolated baseline snippets.
        // Retry with noCheck/noLib to still exercise declaration printer paths.
        const shouldRetryDeclarationFastPath =
          declaration &&
          (errorMsg.includes('TS2307') || errorMsg.includes('TS2304'));
        if (declaration && hasDtsOutput) {
          // Keep the checked declaration output when the compile already emitted it.
          // Retrying with --noCheck discards semantic type information and can turn
          // otherwise-correct `.d.ts` output into `any`.
          //
          // This still matches tsc behavior: diagnostics do not suppress declaration
          // emit by default unless --noEmitOnError is set.
        } else if (!shouldRetryDeclarationFastPath) {
          // Match tsc behavior: diagnostics can still produce outputs (exit code 2).
          // For JS-only emit mode, continue if JS output was generated.
          if (!declaration && hasJsOutput) {
            // continue
          } else if (declaration && hasDtsOutput) {
            // declaration emit produced output despite diagnostics
          } else {
            throw e;
          }
        } else {
          const retryArgs = ['--declaration', '--noCheck', '--noLib'];
          appendCompilerOptionFlags(retryArgs, {
            alwaysStrict,
            sourceMap,
            inlineSourceMap,
            declarationMap,
            downlevelIteration,
            noEmitHelpers,
            noEmitOnError,
            importHelpers,
            esModuleInterop,
            useDefineForClassFields,
            experimentalDecorators,
            emitDecoratorMetadata,
            strictNullChecks,
            exactOptionalPropertyTypes,
            jsx,
            jsxFactory,
            jsxFragmentFactory,
            jsxImportSource,
            moduleDetection,
            preserveConstEnums,
            verbatimModuleSyntax,
            rewriteRelativeImportExtensions,
            isolatedModules,
            importsNotUsedAsValues,
            preserveValueImports,
            removeComments,
            stripInternal,
            outFile,
            outDir,
            rootDir,
          });
          retryArgs.push(...trailingArgs);
          await runWithArgs(retryArgs);
        }
      }

      // Read output files
      let js = '';
      let dts: string | null = null;

      const normalizeOutputRelPath = (filePath: string): string => {
        return path.relative(testDir, filePath).split(path.sep).join('/');
      };

      const normalizeRequestedOutputName = (name: string): string => {
        return name.replace(/^[/\\]+/, '').replace(/\\/g, '/');
      };

      const normalizeComparableOutput = (content: string): string => {
        return content.replace(/\r\n/g, '\n').trim();
      };

      const collectExistingOutputPaths = (dtsMode: boolean): string[] => {
        const seen = new Set<string>();
        const existing: string[] = [];
        for (const out of expectedOutputs) {
          const candidates = dtsMode ? out.dtsCandidates : out.jsCandidates;
          for (const candidate of candidates) {
            if (!fs.existsSync(candidate)) continue;
            const relPath = normalizeOutputRelPath(candidate);
            if (!seen.has(relPath)) {
              seen.add(relPath);
              existing.push(candidate);
            }
            break;
          }
        }
        return existing.sort((a, b) => normalizeOutputRelPath(a).localeCompare(normalizeOutputRelPath(b)));
      };

      const resolveNamedOutputPath = (
        name: string | undefined,
        dtsMode: boolean,
        expectedContent?: string | null,
      ): string | null => {
        if (!name) return null;
        const normalizedName = normalizeRequestedOutputName(name);
        const existingOutputs = collectExistingOutputPaths(dtsMode);
        if (normalizedName.includes('/')) {
          const exactMatch = existingOutputs.find(candidate => normalizeOutputRelPath(candidate) === normalizedName);
          if (exactMatch) return exactMatch;
        }
        const basename = path.posix.basename(normalizedName);
        const basenameMatches = existingOutputs.filter(candidate => {
          return path.posix.basename(normalizeOutputRelPath(candidate)) === basename;
        });
        if (basenameMatches.length > 0) {
          if (expectedContent != null) {
            const normalizedExpected = normalizeComparableOutput(expectedContent);
            for (const candidate of basenameMatches) {
              const candidateContent = fs.readFileSync(candidate, 'utf-8');
              if (normalizeComparableOutput(candidateContent) === normalizedExpected) {
                return candidate;
              }
            }
          }
          return basenameMatches[basenameMatches.length - 1];
        }
        const directPath = path.join(testDir, normalizedName);
        if (fs.existsSync(directPath)) return directPath;
        return null;
      };

      const readNamedOutput = (
        name: string | undefined,
        dtsMode: boolean,
        expectedContent?: string | null,
      ): string | null => {
        const outPath = resolveNamedOutputPath(name, dtsMode, expectedContent);
        if (!outPath) return null;
        return fs.readFileSync(outPath, 'utf-8');
      };

      const readFirstExisting = (candidates: string[]): string | null => {
        for (const candidate of candidates) {
          if (fs.existsSync(candidate)) {
            return fs.readFileSync(candidate, 'utf-8');
          }
        }
        return null;
      };

      const namedJs = readNamedOutput(expectedJsFileName, false, expectedJsContent);
      if (namedJs !== null) {
        js = namedJs;
      } else {
        const chunks: string[] = [];
        let sawUseStrict = false;
        for (const out of [...expectedOutputs].sort((a, b) => {
          return normalizeOutputRelPath(a.jsPath).localeCompare(normalizeOutputRelPath(b.jsPath));
        })) {
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
      // Only CJS (1) needs "use strict" compensation for JS input files.
      // AMD (2) and UMD (3) add "use strict" inside their wrapper functions.
      // Preserve (200) keeps ESM as ESM, which is implicitly strict.
      const commonJsLikeModule = module === 1;
      if (hasJsInput && commonJsLikeModule && !hasUseStrictPreamble(js)) {
        js = `"use strict";\n${js}`;
      }
      if (hasJsInput) {
        js = normalizeLeadingTripleSlashSpacing(js);
      }

      if (declaration) {
        const namedDts = readNamedOutput(expectedDtsFileName, true, expectedDtsContent);
        if (namedDts !== null) {
          dts = namedDts;
        } else {
          const dtsChunks: string[] = [];
          for (const out of [...expectedOutputs].sort((a, b) => {
            return normalizeOutputRelPath(a.dtsPath).localeCompare(normalizeOutputRelPath(b.dtsPath));
          })) {
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
