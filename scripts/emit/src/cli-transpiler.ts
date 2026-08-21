/**
 * CLI-based transpiler using native tsz binary
 *
 * Uses the canonical process boundary so the harness observes the real product.
 * Uses async execFile (no shell) for parallel execution support.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { execFile as execFileCb, execSync, type ChildProcess } from 'child_process';
import { promisify } from 'util';
import { fileURLToPath } from 'url';
import { targetToCliArg, moduleToCliArg } from './ts-enums.js';
import type { CompilerOutcome, EmitProduct } from './canonical-products.js';
import {
  corpusPhysicalPath,
  corpusRelativePath,
  isStagedInputPath,
  physicalDirectoryCompilerOptions,
  physicalPathIdentity,
} from './harness-config.js';

const execFile = promisify(execFileCb);

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');

// Default CLI timeout in ms
const DEFAULT_TIMEOUT_MS = 5000;

export interface TranspileResult {
  jsProducts: EmitProduct[];
  dtsProducts: EmitProduct[];
  outcome: CompilerOutcome;
}

export interface CompilerExecutable {
  binaryPath: string;
  label: string;
}

interface SourceInputFile {
  name: string;
  content: string;
}

export interface LinkInput {
  target: string;
  link: string;
}

interface DerivedOutputPaths {
  jsPath: string;
  dtsPath: string;
}

interface CompilerFlagOptions {
  declaration?: boolean;
  noCheck?: boolean;
  noLib?: boolean;
  noEmit?: boolean;
  alwaysStrict?: boolean;
  sourceMap?: boolean;
  inlineSourceMap?: boolean;
  declarationMap?: boolean;
  downlevelIteration?: boolean;
  noEmitHelpers?: boolean;
  noEmitOnError?: boolean;
  strict?: boolean;
  allowJs?: boolean;
  allowUnreachableCode?: boolean;
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
  moduleResolution?: string;
  moduleDetection?: string;
  preserveConstEnums?: boolean;
  verbatimModuleSyntax?: boolean;
  rewriteRelativeImportExtensions?: boolean;
  isolatedModules?: boolean;
  importsNotUsedAsValues?: string;
  preserveValueImports?: boolean;
  removeComments?: boolean;
  stripInternal?: boolean;
  baseUrl?: string;
  outFile?: string;
  outDir?: string;
  declarationDir?: string;
  rootDir?: string;
  emitDeclarationOnly?: boolean;
}

// Longest common directory of a set of POSIX-style relative file paths,
// returned as a slash-joined prefix without a trailing slash (empty when the
// files share no directory beyond the root). Mirrors tsc's
// `getCommonSourceDirectory` component-wise comparison.
function longestCommonSourceDir(relPaths: string[]): string {
  if (relPaths.length === 0) return '';
  const dirComponents = relPaths.map(p => p.split('/').slice(0, -1));
  let common = dirComponents[0];
  for (const comps of dirComponents.slice(1)) {
    let i = 0;
    while (i < common.length && i < comps.length && common[i] === comps[i]) i++;
    common = common.slice(0, i);
    if (common.length === 0) break;
  }
  return common.join('/');
}

// Append the compiler-option flags used by the one canonical invocation.
function appendCompilerOptionFlags(args: string[], opts: CompilerFlagOptions): void {
  const booleanFlag = (name: string, value: boolean | undefined): void => {
    if (value !== undefined) args.push(name, String(value));
  };
  booleanFlag('--noCheck', opts.noCheck);
  booleanFlag('--declaration', opts.declaration);
  booleanFlag('--noLib', opts.noLib);
  booleanFlag('--noEmit', opts.noEmit);
  booleanFlag('--alwaysStrict', opts.alwaysStrict);
  booleanFlag('--sourceMap', opts.sourceMap);
  booleanFlag('--inlineSourceMap', opts.inlineSourceMap);
  booleanFlag('--declarationMap', opts.declarationMap);
  booleanFlag('--downlevelIteration', opts.downlevelIteration);
  booleanFlag('--noEmitHelpers', opts.noEmitHelpers);
  booleanFlag('--noEmitOnError', opts.noEmitOnError);
  booleanFlag('--strict', opts.strict);
  booleanFlag('--allowJs', opts.allowJs);
  booleanFlag('--allowUnreachableCode', opts.allowUnreachableCode);
  booleanFlag('--importHelpers', opts.importHelpers);
  booleanFlag('--esModuleInterop', opts.esModuleInterop);
  booleanFlag('--useDefineForClassFields', opts.useDefineForClassFields);
  booleanFlag('--experimentalDecorators', opts.experimentalDecorators);
  booleanFlag('--emitDecoratorMetadata', opts.emitDecoratorMetadata);
  booleanFlag('--strictNullChecks', opts.strictNullChecks);
  booleanFlag('--exactOptionalPropertyTypes', opts.exactOptionalPropertyTypes);
  if (opts.jsx) args.push('--jsx', opts.jsx);
  if (opts.jsxFactory) args.push('--jsxFactory', opts.jsxFactory);
  if (opts.jsxFragmentFactory) args.push('--jsxFragmentFactory', opts.jsxFragmentFactory);
  if (opts.jsxImportSource) args.push('--jsxImportSource', opts.jsxImportSource);
  if (opts.moduleResolution) args.push('--moduleResolution', opts.moduleResolution);
  if (opts.moduleDetection) args.push('--moduleDetection', opts.moduleDetection);
  booleanFlag('--preserveConstEnums', opts.preserveConstEnums);
  booleanFlag('--verbatimModuleSyntax', opts.verbatimModuleSyntax);
  booleanFlag('--rewriteRelativeImportExtensions', opts.rewriteRelativeImportExtensions);
  booleanFlag('--isolatedModules', opts.isolatedModules);
  if (opts.importsNotUsedAsValues) args.push('--importsNotUsedAsValues', opts.importsNotUsedAsValues);
  booleanFlag('--preserveValueImports', opts.preserveValueImports);
  booleanFlag('--removeComments', opts.removeComments);
  booleanFlag('--stripInternal', opts.stripInternal);
  if (opts.baseUrl) args.push('--baseUrl', opts.baseUrl);
  if (opts.outFile) args.push('--outFile', opts.outFile.replace(/^[/\\]+/, ''));
  if (opts.outDir) args.push('--outDir', opts.outDir);
  if (opts.declarationDir) args.push('--declarationDir', opts.declarationDir);
  if (opts.rootDir) args.push('--rootDir', opts.rootDir);
  booleanFlag('--emitDeclarationOnly', opts.emitDeclarationOnly);
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
 * CLI-based transpiler for canonical emit observations.
 */
export class CliTranspiler {
  private compiler: CompilerExecutable;
  private counter = 0;
  private tempDir: string;
  private timeoutMs: number;
  private activeChildren = new Set<ChildProcess>();

  constructor(timeoutMs: number = DEFAULT_TIMEOUT_MS, compiler?: CompilerExecutable) {
    this.compiler = compiler ?? { binaryPath: findTszBinary(), label: 'tsz' };
    this.tempDir = fs.mkdtempSync(path.join(os.tmpdir(), `${this.compiler.label}-emit-`));
    this.timeoutMs = timeoutMs;
  }

  /**
   * Transpile TypeScript source using the CLI.
   * Uses async execFile (no shell) for parallel-safe execution.
   */
  async transpile(
    source: string,
    target: number | undefined,
    module: number | undefined,
    options: {
      sourceFileName?: string;
      declaration?: boolean;
      noCheck?: boolean;
      noLib?: boolean;
      noEmit?: boolean;
      alwaysStrict?: boolean;
      sourceMap?: boolean;
      inlineSourceMap?: boolean;
      declarationMap?: boolean;
      downlevelIteration?: boolean;
      noEmitHelpers?: boolean;
      noEmitOnError?: boolean;
      strict?: boolean;
      allowJs?: boolean;
      allowUnreachableCode?: boolean;
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
      moduleResolution?: string;
      moduleDetection?: string;
      preserveConstEnums?: boolean;
      verbatimModuleSyntax?: boolean;
      rewriteRelativeImportExtensions?: boolean;
      isolatedModules?: boolean;
      importsNotUsedAsValues?: string;
      preserveValueImports?: boolean;
      removeComments?: boolean;
      stripInternal?: boolean;
      baseUrl?: string;
      outFile?: string;
      outDir?: string;
      declarationDir?: string;
      rootDir?: string;
      emitDeclarationOnly?: boolean;
      sourceFiles?: SourceInputFile[];
      rootFileNames?: string[];
      links?: LinkInput[];
      lib?: string[];
    } = {}
  ): Promise<TranspileResult> {
    const {
      sourceFileName,
      declaration,
      noCheck,
      noLib,
      noEmit,
      alwaysStrict,
      sourceMap,
      inlineSourceMap,
      declarationMap,
      downlevelIteration,
      noEmitHelpers,
      noEmitOnError,
      strict,
      allowJs,
      allowUnreachableCode,
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
      moduleResolution,
      moduleDetection,
      preserveConstEnums,
      verbatimModuleSyntax,
      rewriteRelativeImportExtensions,
      isolatedModules,
      importsNotUsedAsValues,
      preserveValueImports,
      removeComments,
      stripInternal,
      baseUrl,
      outFile,
      outDir,
      declarationDir,
      rootDir,
      emitDeclarationOnly,
      sourceFiles,
      rootFileNames,
      links = [],
      lib,
    } = options;
    const declarationRequested = declaration === true || emitDeclarationOnly === true;
    const testName = `test_${this.counter++}`;
    const files: SourceInputFile[] = sourceFiles && sourceFiles.length > 0
      ? sourceFiles
      : [{
          name: sourceFileName ?? `${testName}.ts`,
          content: source,
        }];
    const corpusPaths = [
      ...files.map(file => file.name),
      ...links.flatMap(link => [link.target, link.link]),
      ...[outFile, outDir, declarationDir, rootDir].filter((value): value is string => value !== undefined),
    ];
    const maxParentDepth = corpusPaths.reduce((maximum, corpusPath) => {
      const parts = corpusRelativePath(corpusPath).split('/');
      let depth = 0;
      while (parts[depth] === '..') depth++;
      return Math.max(maximum, depth);
    }, 0);
    const testScopeDir = path.join(this.tempDir, testName);
    // Preserve cwd-relative `..` path semantics while ensuring no product can
    // leak into, or be reused from, another invocation's scope.
    const requestedTestDir = path.join(
      testScopeDir,
      ...Array.from({ length: maxParentDepth + 1 }, () => 'cwd'),
    );
    fs.mkdirSync(requestedTestDir, { recursive: true });
    const testDir = physicalPathIdentity(requestedTestDir);
    const normalizedOutFile = outFile ? corpusRelativePath(outFile) : undefined;
    const outputFilePath = normalizedOutFile ? corpusPhysicalPath(testDir, normalizedOutFile) : null;
    const outputDtsPath = outputFilePath?.replace(/\.js$/, '.d.ts') ?? null;

    const inputPathByCorpusName = new Map<string, string>();
    const inputPathSet = new Set<string>();
    // A staged JS/MJS/CJS source may already have the same name as a requested
    // output. Its mere existence is never evidence that the compiler emitted it.
    const isCollectableOutput = (candidate: string): boolean => {
      return fs.existsSync(candidate) && !isStagedInputPath(inputPathSet, candidate);
    };
    const derivedOutputs: DerivedOutputPaths[] = [];

    // When `rootDir` is unset, tsz (like tsc) lays output out relative to the
    // common source directory of the emittable inputs, not the test root. So a
    // test whose files all live under `src/` emits to `<outDir>/a.js`, not
    // `<outDir>/src/a.js`. Compute that common directory (over non-declaration,
    // non-node_modules sources) so the stripped output path is offered as an
    // exact output location below.
    const emittableRelNames = files
      .map(f => corpusRelativePath(f.name))
      .filter(
        rel =>
          !rel.endsWith('package.json') &&
          !rel.endsWith('tsconfig.json') &&
          !/\.d\.(?:ts|mts|cts)$/.test(rel) &&
          !rel.split('/').includes('node_modules'),
      );
    const commonSourceDir = rootDir ? '' : longestCommonSourceDir(emittableRelNames);
    const stripCommonSourceDir = (relStem: string): string =>
      commonSourceDir &&
      (relStem === commonSourceDir || relStem.startsWith(`${commonSourceDir}/`))
        ? relStem.slice(commonSourceDir.length).replace(/^\/+/, '')
        : relStem;

    for (const file of files) {
      const relName = corpusRelativePath(file.name);
      const filePath = corpusPhysicalPath(testDir, relName);
      fs.mkdirSync(path.dirname(filePath), { recursive: true });
      fs.writeFileSync(filePath, file.content, 'utf-8');

      // Auxiliary files (package.json, tsconfig.json) are written to disk
      // but not passed as CLI input arguments or expected to produce output.
      const isAuxiliary = relName.endsWith('package.json') || relName.endsWith('tsconfig.json');
      if (isAuxiliary) {
        continue;
      }

      inputPathSet.add(physicalPathIdentity(filePath));
      inputPathByCorpusName.set(relName, filePath);

      if (relName.endsWith('.d.ts') || relName.endsWith('.d.mts') || relName.endsWith('.d.cts')) {
        continue;
      }
      const extMatch = relName.match(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/);
      if (!extMatch) continue;
      const ext = `.${extMatch[1]}`;
      const stem = filePath.replace(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/, '');
      const normalizedRoot = rootDir === undefined ? undefined : corpusRelativePath(rootDir);
      let relativeStem = relName.replace(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/, '');
      if (rootDir !== undefined) {
        relativeStem = path.posix.relative(normalizedRoot ?? '', relativeStem);
      } else {
        relativeStem = stripCommonSourceDir(relativeStem);
      }

      const preservesJsx = jsx?.toLowerCase() === 'preserve';
      const jsExtension =
        ext === '.mts' || ext === '.mjs' ? '.mjs' :
        ext === '.cts' || ext === '.cjs' ? '.cjs' :
        ext === '.tsx' || ext === '.jsx' ? (preservesJsx ? '.jsx' : '.js') :
        '.js';
      const dtsExtension =
        ext === '.mts' || ext === '.mjs' ? '.d.mts' :
        ext === '.cts' || ext === '.cjs' ? '.d.cts' :
        '.d.ts';
      const jsStem = outDir
        ? path.join(corpusPhysicalPath(testDir, outDir), relativeStem)
        : stem;
      const declarationOutputDir = declarationDir ?? outDir;
      const dtsStem = declarationOutputDir
        ? path.join(corpusPhysicalPath(testDir, declarationOutputDir), relativeStem)
        : stem;

      derivedOutputs.push({
        jsPath: outputFilePath ?? `${jsStem}${jsExtension}`,
        dtsPath: outputDtsPath ?? `${dtsStem}${dtsExtension}`,
      });
    }

    for (const link of links) {
      const targetPath = corpusPhysicalPath(testDir, link.target);
      const linkPath = corpusPhysicalPath(testDir, link.link);
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

    const requestedRootNames = rootFileNames ?? files
      .map(file => corpusRelativePath(file.name))
      .filter(name => !name.endsWith('.json'));
    const rootInputFiles: string[] = [];
    let missingRootName: string | undefined;
    for (const rootName of requestedRootNames) {
      const normalizedName = corpusRelativePath(rootName);
      const inputPath = inputPathByCorpusName.get(normalizedName);
      if (!inputPath) {
        missingRootName ??= rootName;
      } else {
        rootInputFiles.push(inputPath);
      }
    }

    try {
      if (missingRootName) throw new Error(`UNREPRESENTABLE_ROOT_INPUT:${missingRootName}`);
      // Build one authored invocation. `--pretty false` and `--ignoreConfig`
      // are deterministic process/staging controls and are applied equally to
      // TypeScript 7 and TSZ; semantic shortcuts are never synthesized.
      const args: string[] = ['--pretty', 'false'];
      // The emit harness stages embedded @filename tsconfig.json files next to
      // explicit command-line inputs. That mirrors tsc baseline fixtures, but
      // the CLI intentionally rejects "files + discovered tsconfig" unless
      // --ignoreConfig is set.
      if (rootInputFiles.length > 0) args.push('--ignoreConfig');
      // The emit runner synthesizes explicit CLI invocations from baseline
      // files. Embedded tsconfig options are parsed and forwarded as flags
      // below; leaving tsconfig.json discoverable would make tsz stop with
      // TS5112 before it emits.
      const hasEmbeddedTsconfig = files.some(f => corpusRelativePath(f.name).endsWith('tsconfig.json'));
      if (hasEmbeddedTsconfig) {
        args.push('--ignoreConfig');
      }
      if (lib && lib.length > 0) args.push('--lib', lib.join(','));
      const physicalDirectories = physicalDirectoryCompilerOptions(testDir, {
        outDir,
        declarationDir,
        rootDir,
      });
      appendCompilerOptionFlags(args, {
        declaration,
        noCheck,
        noLib,
        noEmit,
        alwaysStrict,
        sourceMap,
        inlineSourceMap,
        declarationMap,
        downlevelIteration,
        noEmitHelpers,
        noEmitOnError,
        strict,
        allowJs,
        allowUnreachableCode,
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
        moduleResolution,
        moduleDetection,
        preserveConstEnums,
        verbatimModuleSyntax,
        rewriteRelativeImportExtensions,
        isolatedModules,
        importsNotUsedAsValues,
        preserveValueImports,
        removeComments,
        stripInternal,
        baseUrl,
        outFile,
        emitDeclarationOnly,
        ...physicalDirectories,
      });
      if (target !== undefined) args.push('--target', targetToCliArg(target));
      if (module !== undefined) args.push('--module', moduleToCliArg(module));
      args.push(...rootInputFiles);

      const normalizeOutputRelPath = (filePath: string): string => {
        return path.relative(testDir, physicalPathIdentity(filePath)).split(path.sep).join('/').replace(/\\/g, '/');
      };
      const productPaths = (dtsMode: boolean): string[] => {
        const paths = derivedOutputs.map(output => dtsMode ? output.dtsPath : output.jsPath);
        return [...new Set(paths.map(physicalPathIdentity))];
      };
      const collectActualProducts = (): { jsProducts: EmitProduct[]; dtsProducts: EmitProduct[] } => {
        const jsProducts: EmitProduct[] = [];
        const dtsProducts: EmitProduct[] = [];
        const seen = new Set<string>();
        const addFile = (entryPath: string): void => {
          if (!isCollectableOutput(entryPath)) return;
          const relativePath = normalizeOutputRelPath(entryPath);
          if (seen.has(relativePath)) return;
          seen.add(relativePath);
          const product = { path: relativePath, content: fs.readFileSync(entryPath).toString('latin1') };
          if (/\.d\.(?:ts|mts|cts)$/.test(relativePath)) {
            dtsProducts.push(product);
          } else if (/\.(?:js|jsx|mjs|cjs)$/.test(relativePath)) {
            jsProducts.push(product);
          }
        };
        const visit = (directory: string): void => {
          for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
            const entryPath = path.join(directory, entry.name);
            if (entry.isSymbolicLink()) continue;
            if (entry.isDirectory()) {
              visit(entryPath);
              continue;
            }
            if (entry.isFile()) addFile(entryPath);
          }
        };
        visit(testScopeDir);
        // Also add exact derived paths in case a symlinked output path is not
        // reached by the scope walk. Staged inputs remain excluded.
        for (const candidate of [...productPaths(false), ...productPaths(true)]) addFile(candidate);
        jsProducts.sort((left, right) => left.path.localeCompare(right.path));
        dtsProducts.sort((left, right) => left.path.localeCompare(right.path));
        return { jsProducts, dtsProducts };
      };
      const collectResult = (outcome: CompilerOutcome): TranspileResult => {
        const { jsProducts, dtsProducts } = collectActualProducts();
        return { jsProducts, dtsProducts, outcome };
      };

      // Run CLI asynchronously without shell overhead.
      // Use SIGKILL for timeout so the child can't ignore the signal and linger.
      const runWithArgs = async (cliArgs: string[]) => {
        const promise = execFile(this.compiler.binaryPath, cliArgs, {
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

      const diagnosticCodes = (stdout: unknown, stderr: unknown): string[] => {
        const text = `${typeof stdout === 'string' ? stdout : ''}\n${typeof stderr === 'string' ? stderr : ''}`;
        return [...text.matchAll(/\bTS(\d{4,5})\b/g)].map(match => `TS${match[1]}`);
      };
      try {
        const completed = await runWithArgs(args);
        return collectResult({
          exitCode: 0,
          diagnosticCodes: diagnosticCodes(completed.stdout, completed.stderr),
        });
      } catch (error) {
        const failure = error as {
          code?: number | string;
          killed?: boolean;
          signal?: string;
          stdout?: unknown;
          stderr?: unknown;
        };
        if (failure.killed) {
          throw new Error(`TIMEOUT:${this.compiler.label}`);
        }
        if (failure.signal) throw new Error(`CRASH:${this.compiler.label}:${failure.signal}`);
        if (typeof failure.code !== 'number') throw error;
        return collectResult({
          exitCode: failure.code,
          diagnosticCodes: diagnosticCodes(failure.stdout, failure.stderr),
        });
      }
    } catch (e) {
      // Handle timeout (execFile sends SIGKILL on timeout)
      if (e instanceof Error && 'killed' in e && ((e as any).signal === 'SIGKILL' || (e as any).signal === 'SIGTERM')) {
        throw new Error(`TIMEOUT:${this.compiler.label}`);
      }
      throw e;
    } finally {
      try { fs.rmSync(testScopeDir, { recursive: true, force: true }); } catch {}
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
