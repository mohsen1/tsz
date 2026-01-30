/**
 * Shared TSC runner for conformance testing.
 *
 * Extracted from cache-worker.ts for reuse in:
 * - cache-worker.ts (generates TSC cache)
 * - runner-server.ts (--print-test mode)
 */

import * as ts from 'typescript';
import * as fs from 'fs';
import * as path from 'path';
import { parseLibOption } from './test-utils.js';

// ============================================================================
// Types
// ============================================================================

export interface TestFile {
  name: string;
  content: string;
}

export interface ParsedTestCase {
  options: Record<string, unknown>;
  isMultiFile: boolean;
  files: TestFile[];
}

export interface DiagnosticInfo {
  code: number;
  message: string;
  file?: string;
  line?: number;
  column?: number;
}

export interface TscResult {
  codes: number[];
  diagnostics: DiagnosticInfo[];
}

// ============================================================================
// Lib Loading
// ============================================================================

const LIB_REFERENCE_RE = /\/\/\/\s*<reference\s+lib=["']([^"']+)["']\s*\/>/g;
const libContentCache = new Map<string, string>();
const libPathCache = new Map<string, string>();

export function parseLibReferences(source: string): string[] {
  const refs: string[] = [];
  for (const match of source.matchAll(LIB_REFERENCE_RE)) {
    if (match[1]) refs.push(match[1].trim().toLowerCase());
  }
  return refs;
}

export function resolveLibFilePath(libName: string, libDir: string): string | null {
  const normalized = libName.trim().toLowerCase();
  const cacheKey = `${libDir}:${normalized}`;
  if (libPathCache.has(cacheKey)) return libPathCache.get(cacheKey)!;
  if (!libDir || !fs.existsSync(path.join(libDir, 'es5.d.ts'))) return null;

  const candidates = [
    path.join(libDir, `${normalized}.d.ts`),
    path.join(libDir, `lib.${normalized}.d.ts`),
    path.join(libDir, `${normalized}.generated.d.ts`),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      libPathCache.set(cacheKey, candidate);
      return candidate;
    }
  }
  return null;
}

export function readLibContent(libName: string, libDir: string): string | null {
  const normalized = libName.trim().toLowerCase();
  const cacheKey = `${libDir}:${normalized}`;
  if (libContentCache.has(cacheKey)) return libContentCache.get(cacheKey)!;
  const libFilePath = resolveLibFilePath(normalized, libDir);
  if (!libFilePath) return null;
  const content = fs.readFileSync(libFilePath, 'utf8');
  libContentCache.set(cacheKey, content);
  return content;
}

function loadLibRecursive(
  libName: string,
  libDir: string,
  out: Map<string, string>,
  seen: Set<string>
): void {
  const normalized = libName.trim().toLowerCase();
  if (seen.has(normalized)) return;
  seen.add(normalized);

  const content = readLibContent(normalized, libDir);
  if (!content) return;

  out.set(`lib.${normalized}.d.ts`, content);

  for (const ref of parseLibReferences(content)) {
    loadLibRecursive(ref, libDir, out, seen);
  }
}

function normalizeTargetName(target: unknown): string {
  if (typeof target === 'number') {
    const name = (ts.ScriptTarget as Record<number, string>)[target];
    if (typeof name === 'string') return name.toLowerCase();
  }
  return String(target ?? 'es2020').toLowerCase();
}

function defaultCoreLibNameForTarget(targetName: string): string {
  switch (targetName) {
    case 'es3':
    case 'es5':
      return 'es5';
    case 'es6':
    case 'es2015':
      return 'es2015';
    case 'es2016':
      return 'es2016';
    case 'es2017':
      return 'es2017';
    case 'es2018':
      return 'es2018';
    case 'es2019':
      return 'es2019';
    case 'es2020':
      return 'es2020';
    case 'es2021':
      return 'es2021';
    case 'es2022':
      return 'es2022';
    case 'es2023':
      return 'es2023';
    case 'es2024':
      return 'es2024';
    case 'esnext':
      return 'esnext';
    default:
      return 'es5';
  }
}

function getLibNamesForTestCase(
  opts: Record<string, unknown>,
  compilerOptionsTarget: ts.ScriptTarget | undefined
): string[] {
  if (opts.nolib) return [];
  const explicit = parseLibOption(opts.lib);
  if (explicit.length > 0) return explicit;

  // No explicit @lib - return default libs based on target
  const targetName = normalizeTargetName(compilerOptionsTarget ?? opts.target);
  return [defaultCoreLibNameForTarget(targetName)];
}

export function collectLibFiles(libNames: string[], libDir: string): Map<string, string> {
  const out = new Map<string, string>();
  const seen = new Set<string>();
  for (const libName of libNames) {
    loadLibRecursive(libName, libDir, out, seen);
  }
  return out;
}

// ============================================================================
// Target/Module Mapping
// ============================================================================

const TARGET_MAP: Record<string, ts.ScriptTarget> = {
  es3: ts.ScriptTarget.ES3,
  es5: ts.ScriptTarget.ES5,
  es6: ts.ScriptTarget.ES2015,
  es2015: ts.ScriptTarget.ES2015,
  es2016: ts.ScriptTarget.ES2016,
  es2017: ts.ScriptTarget.ES2017,
  es2018: ts.ScriptTarget.ES2018,
  es2019: ts.ScriptTarget.ES2019,
  es2020: ts.ScriptTarget.ES2020,
  es2021: ts.ScriptTarget.ES2021,
  es2022: ts.ScriptTarget.ES2022,
  es2023: ts.ScriptTarget.ES2023,
  es2024: ts.ScriptTarget.ES2024,
  esnext: ts.ScriptTarget.ESNext,
};

const MODULE_MAP: Record<string, ts.ModuleKind> = {
  none: ts.ModuleKind.None,
  commonjs: ts.ModuleKind.CommonJS,
  amd: ts.ModuleKind.AMD,
  umd: ts.ModuleKind.UMD,
  system: ts.ModuleKind.System,
  es6: ts.ModuleKind.ES2015,
  es2015: ts.ModuleKind.ES2015,
  es2020: ts.ModuleKind.ES2020,
  es2022: ts.ModuleKind.ES2022,
  esnext: ts.ModuleKind.ESNext,
  node16: ts.ModuleKind.Node16,
  node18: ts.ModuleKind.Node18,
  node20: ts.ModuleKind.Node20,
  nodenext: ts.ModuleKind.NodeNext,
  preserve: ts.ModuleKind.Preserve,
};

const MODULE_RESOLUTION_MAP: Record<string, ts.ModuleResolutionKind> = {
  classic: ts.ModuleResolutionKind.Classic,
  node: ts.ModuleResolutionKind.Node10,
  node10: ts.ModuleResolutionKind.Node10,
  node16: ts.ModuleResolutionKind.Node16,
  nodenext: ts.ModuleResolutionKind.NodeNext,
  bundler: ts.ModuleResolutionKind.Bundler,
};

const MODULE_DETECTION_MAP: Record<string, ts.ModuleDetectionKind> = {
  legacy: ts.ModuleDetectionKind.Legacy,
  auto: ts.ModuleDetectionKind.Auto,
  force: ts.ModuleDetectionKind.Force,
};

/**
 * For directives that support comma-separated multi-run values
 * (e.g. "@module: esnext, preserve"), extract the first value.
 */
function firstEnumValue(val: unknown): string {
  return String(val).split(',')[0].trim().toLowerCase();
}

// ============================================================================
// Test Parsing
// ============================================================================

export function parseTestDirectives(code: string, filePath: string): ParsedTestCase {
  const lines = code.split('\n');
  const options: Record<string, unknown> = {};
  let isMultiFile = false;
  const files: TestFile[] = [];
  let currentFileName: string | null = null;
  let currentFileLines: string[] = [];
  const cleanLines: string[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    const filenameMatch = trimmed.match(/^\/\/\s*@filename:\s*(.+)$/i);
    if (filenameMatch) {
      isMultiFile = true;
      if (currentFileName) {
        files.push({ name: currentFileName, content: currentFileLines.join('\n') });
      }
      currentFileName = filenameMatch[1].trim();
      currentFileLines = [];
      continue;
    }

    const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
    if (optionMatch) {
      const [, key, value] = optionMatch;
      const lowKey = key.toLowerCase();
      // Only convert to boolean, keep everything else as string.
      // This prevents numeric strings (e.g. @jsxFragmentFactory: 234)
      // from being converted to numbers and later crashing TSC.
      if (value.toLowerCase() === 'true') options[lowKey] = true;
      else if (value.toLowerCase() === 'false') options[lowKey] = false;
      else options[lowKey] = value;
      continue;
    }

    if (isMultiFile && currentFileName) {
      currentFileLines.push(line);
    } else {
      cleanLines.push(line);
    }
  }

  if (isMultiFile && currentFileName) {
    files.push({ name: currentFileName, content: currentFileLines.join('\n') });
  }

  // For multi-file tests, also include the main test file if it's not already included
  // This is needed so TypeScript can use the main file path as the root for resolution
  if (isMultiFile) {
    const mainFileName = path.basename(filePath);
    if (!files.some(f => f.name === mainFileName)) {
      files.push({ name: mainFileName, content: cleanLines.join('\n') });
    }
  } else {
    files.push({ name: path.basename(filePath), content: cleanLines.join('\n') });
  }

  // Extract compiler options from embedded tsconfig.json in multi-file tests.
  // Many tests (especially compiler/ tests) put options inside a @filename: /tsconfig.json
  // file instead of as top-level @ directives. Merge those into options (directives win).
  if (isMultiFile) {
    for (const file of files) {
      if (path.basename(file.name) === 'tsconfig.json') {
        try {
          // Handle JSON with trailing commas by stripping them
          const cleanJson = file.content.replace(/,\s*([}\]])/g, '$1');
          const tsconfig = JSON.parse(cleanJson);
          if (tsconfig?.compilerOptions) {
            for (const [key, value] of Object.entries(tsconfig.compilerOptions)) {
              const lowKey = key.toLowerCase();
              // Only set if not already specified by a top-level @ directive
              if (!(lowKey in options)) {
                options[lowKey] = value;
              }
            }
          }
        } catch {
          // Invalid JSON in tsconfig.json - skip
        }
        break;
      }
    }
  }

  return { options, isMultiFile, files };
}

// ============================================================================
// Compiler Options
// ============================================================================

function toBool(val: unknown): boolean {
  if (typeof val === 'boolean') return val;
  if (typeof val === 'string') return val.toLowerCase() === 'true';
  return Boolean(val);
}

function toCompilerOptions(opts: Record<string, unknown>): ts.CompilerOptions {
  const options: ts.CompilerOptions = {};

  // noEmit - respect the directive, default to true
  if (opts.noemit !== undefined) {
    options.noEmit = toBool(opts.noemit);
  } else {
    options.noEmit = true;
  }

  // Target - handle comma-separated multi-run values (take first)
  if (opts.target !== undefined) {
    const t = firstEnumValue(opts.target);
    options.target = TARGET_MAP[t] ?? ts.ScriptTarget.ES2020;
  } else {
    options.target = ts.ScriptTarget.ES2020;
  }

  // Module - handle comma-separated multi-run values (take first)
  if (opts.module !== undefined) {
    const m = firstEnumValue(opts.module);
    options.module = MODULE_MAP[m] ?? ts.ModuleKind.ESNext;
  } else {
    options.module = ts.ModuleKind.ESNext;
  }

  // Module resolution - critical for bundler/node tests
  if (opts.moduleresolution !== undefined) {
    const mr = firstEnumValue(opts.moduleresolution);
    if (MODULE_RESOLUTION_MAP[mr] !== undefined) {
      options.moduleResolution = MODULE_RESOLUTION_MAP[mr];
    }
  }

  // Module detection
  if (opts.moduledetection !== undefined) {
    const md = firstEnumValue(opts.moduledetection);
    if (MODULE_DETECTION_MAP[md] !== undefined) {
      options.moduleDetection = MODULE_DETECTION_MAP[md];
    }
  }

  // Strict mode flags
  if (opts.strict !== undefined) options.strict = toBool(opts.strict);
  if (opts.noimplicitany !== undefined) options.noImplicitAny = toBool(opts.noimplicitany);
  if (opts.strictnullchecks !== undefined) options.strictNullChecks = toBool(opts.strictnullchecks);
  if (opts.strictfunctiontypes !== undefined) options.strictFunctionTypes = toBool(opts.strictfunctiontypes);
  if (opts.strictbindcallapply !== undefined) options.strictBindCallApply = toBool(opts.strictbindcallapply);
  if (opts.strictpropertyinitialization !== undefined) options.strictPropertyInitialization = toBool(opts.strictpropertyinitialization);
  if (opts.strictbuiltiniteratorreturn !== undefined) options.strictBuiltinIteratorReturn = toBool(opts.strictbuiltiniteratorreturn);
  if (opts.noimplicitthis !== undefined) options.noImplicitThis = toBool(opts.noimplicitthis);
  if (opts.useunknownincatchvariables !== undefined) options.useUnknownInCatchVariables = toBool(opts.useunknownincatchvariables);
  if (opts.alwaysstrict !== undefined) options.alwaysStrict = toBool(opts.alwaysstrict);
  if (opts.nolib !== undefined) options.noLib = toBool(opts.nolib);

  // Additional checks
  if (opts.nopropertyaccessfromindexsignature !== undefined) {
    options.noPropertyAccessFromIndexSignature = toBool(opts.nopropertyaccessfromindexsignature);
  }
  if (opts.nouncheckedindexedaccess !== undefined) {
    options.noUncheckedIndexedAccess = toBool(opts.nouncheckedindexedaccess);
  }
  if (opts.exactoptionalpropertytypes !== undefined) {
    options.exactOptionalPropertyTypes = toBool(opts.exactoptionalpropertytypes);
  }
  if (opts.noimplicitreturns !== undefined) {
    options.noImplicitReturns = toBool(opts.noimplicitreturns);
  }
  if (opts.nofallthroughcasesinswitch !== undefined) {
    options.noFallthroughCasesInSwitch = toBool(opts.nofallthroughcasesinswitch);
  }
  if (opts.nounusedlocals !== undefined) {
    options.noUnusedLocals = toBool(opts.nounusedlocals);
  }
  if (opts.nounusedparameters !== undefined) {
    options.noUnusedParameters = toBool(opts.nounusedparameters);
  }
  if (opts.allowunusedlabels !== undefined) {
    options.allowUnusedLabels = toBool(opts.allowunusedlabels);
  }
  if (opts.allowunreachablecode !== undefined) {
    options.allowUnreachableCode = toBool(opts.allowunreachablecode);
  }
  if (opts.noimplicitoverride !== undefined) {
    options.noImplicitOverride = toBool(opts.noimplicitoverride);
  }

  // JavaScript support (needed for @allowJs and @checkJs tests)
  if (opts.allowjs !== undefined) options.allowJs = toBool(opts.allowjs);
  if (opts.checkjs !== undefined) options.checkJs = toBool(opts.checkjs);

  // JSX options - handle comma-separated multi-run values
  if (opts.jsx !== undefined) {
    const jsx = firstEnumValue(opts.jsx);
    const jsxMap: Record<string, ts.JsxEmit> = {
      'preserve': ts.JsxEmit.Preserve,
      'react': ts.JsxEmit.React,
      'react-native': ts.JsxEmit.ReactNative,
      'react-jsx': ts.JsxEmit.ReactJSX,
      'react-jsxdev': ts.JsxEmit.ReactJSXDev,
    };
    options.jsx = jsxMap[jsx] ?? ts.JsxEmit.React;
  }
  if (opts.jsxfactory !== undefined) {
    options.jsxFactory = String(opts.jsxfactory);
  }
  if (opts.jsxfragmentfactory !== undefined) {
    options.jsxFragmentFactory = String(opts.jsxfragmentfactory);
  }
  if (opts.jsximportsource !== undefined) {
    options.jsxImportSource = String(opts.jsximportsource);
  }

  // Class fields and decorators
  if (opts.usedefineforclassfields !== undefined) {
    options.useDefineForClassFields = toBool(opts.usedefineforclassfields);
  }
  if (opts.experimentaldecorators !== undefined) {
    options.experimentalDecorators = toBool(opts.experimentaldecorators);
  }
  if (opts.emitdecoratormetadata !== undefined) {
    options.emitDecoratorMetadata = toBool(opts.emitdecoratormetadata);
  }

  // Module options
  if (opts.allowsyntheticdefaultimports !== undefined) {
    options.allowSyntheticDefaultImports = toBool(opts.allowsyntheticdefaultimports);
  }
  if (opts.esmoduleinterop !== undefined) {
    options.esModuleInterop = toBool(opts.esmoduleinterop);
  }
  if (opts.preservesymlinks !== undefined) {
    options.preserveSymlinks = toBool(opts.preservesymlinks);
  }
  if (opts.allowumdglobalaccess !== undefined) {
    options.allowUmdGlobalAccess = toBool(opts.allowumdglobalaccess);
  }
  if (opts.allowimportingtsextensions !== undefined) {
    options.allowImportingTsExtensions = toBool(opts.allowimportingtsextensions);
  }
  if (opts.resolvejsonmodule !== undefined) {
    options.resolveJsonModule = toBool(opts.resolvejsonmodule);
  }
  if (opts.noresolve !== undefined) {
    options.noResolve = toBool(opts.noresolve);
  }

  // Package.json resolution (critical for bundler/node module tests)
  if (opts.resolvepackagejsonexports !== undefined) {
    options.resolvePackageJsonExports = toBool(opts.resolvepackagejsonexports);
  }
  if (opts.resolvepackagejsonimports !== undefined) {
    options.resolvePackageJsonImports = toBool(opts.resolvepackagejsonimports);
  }
  if (opts.customconditions !== undefined) {
    const val = opts.customconditions;
    if (Array.isArray(val)) {
      options.customConditions = val.map(String);
    } else {
      options.customConditions = String(val).split(',').map(s => s.trim()).filter(Boolean);
    }
  }

  // Path mapping (needed for tests with tsconfig.json paths)
  if (opts.baseurl !== undefined) {
    options.baseUrl = String(opts.baseurl);
  }
  if (opts.paths !== undefined && typeof opts.paths === 'object' && opts.paths !== null) {
    options.paths = opts.paths as Record<string, string[]>;
  }
  if (opts.rootdirs !== undefined) {
    if (Array.isArray(opts.rootdirs)) {
      options.rootDirs = opts.rootdirs.map(String);
    } else {
      options.rootDirs = String(opts.rootdirs).split(',').map(s => s.trim()).filter(Boolean);
    }
  }
  if (opts.typeroots !== undefined) {
    if (Array.isArray(opts.typeroots)) {
      options.typeRoots = opts.typeroots.map(String);
    } else {
      options.typeRoots = String(opts.typeroots).split(',').map(s => s.trim()).filter(Boolean);
    }
  }
  if (opts.types !== undefined) {
    if (Array.isArray(opts.types)) {
      options.types = opts.types.map(String);
    } else {
      options.types = String(opts.types).split(',').map(s => s.trim()).filter(Boolean);
    }
  }
  if (opts.modulesuffixes !== undefined) {
    if (Array.isArray(opts.modulesuffixes)) {
      options.moduleSuffixes = opts.modulesuffixes.map(String);
    } else {
      options.moduleSuffixes = String(opts.modulesuffixes).split(',').map(s => s.trim());
    }
  }

  // Emit options
  if (opts.outdir !== undefined) options.outDir = String(opts.outdir);
  if (opts.outfile !== undefined) options.outFile = String(opts.outfile);
  if (opts.rootdir !== undefined) options.rootDir = String(opts.rootdir);
  if (opts.declarationdir !== undefined) options.declarationDir = String(opts.declarationdir);
  if (opts.sourcemap !== undefined) options.sourceMap = toBool(opts.sourcemap);
  if (opts.inlinesourcemap !== undefined) options.inlineSourceMap = toBool(opts.inlinesourcemap);
  if (opts.inlinesources !== undefined) options.inlineSources = toBool(opts.inlinesources);
  if (opts.removecomments !== undefined) options.removeComments = toBool(opts.removecomments);
  if (opts.importhelpers !== undefined) options.importHelpers = toBool(opts.importhelpers);
  if (opts.downleveliteration !== undefined) options.downlevelIteration = toBool(opts.downleveliteration);
  if (opts.preserveconstenums !== undefined) options.preserveConstEnums = toBool(opts.preserveconstenums);
  if (opts.noemithelpers !== undefined) options.noEmitHelpers = toBool(opts.noemithelpers);
  if (opts.noemitonerror !== undefined) options.noEmitOnError = toBool(opts.noemitonerror);
  if (opts.emitdeclarationonly !== undefined) options.emitDeclarationOnly = toBool(opts.emitdeclarationonly);
  if (opts.declarationmap !== undefined) options.declarationMap = toBool(opts.declarationmap);

  // Interop constraints
  if (opts.isolatedmodules !== undefined) {
    options.isolatedModules = toBool(opts.isolatedmodules);
  }
  if (opts.verbatimmodulesyntax !== undefined) {
    options.verbatimModuleSyntax = toBool(opts.verbatimmodulesyntax);
  }
  if (opts.isolateddeclarations !== undefined) {
    options.isolatedDeclarations = toBool(opts.isolateddeclarations);
  }

  // Declaration
  if (opts.declaration !== undefined) {
    options.declaration = toBool(opts.declaration);
  }

  // Skip lib check
  if (opts.skiplibcheck !== undefined) {
    options.skipLibCheck = toBool(opts.skiplibcheck);
  }
  if (opts.skipdefaultlibcheck !== undefined) {
    options.skipDefaultLibCheck = toBool(opts.skipdefaultlibcheck);
  }

  // Additional module-related options
  if (opts.allowarbitraryextensions !== undefined) {
    options.allowArbitraryExtensions = toBool(opts.allowarbitraryextensions);
  }
  if (opts.rewriterelativeimportextensions !== undefined) {
    options.rewriteRelativeImportExtensions = toBool(opts.rewriterelativeimportextensions);
  }

  // Backwards compatibility
  if (opts.suppressexcesspropertyerrors !== undefined) {
    options.suppressExcessPropertyErrors = toBool(opts.suppressexcesspropertyerrors);
  }
  if (opts.suppressimplicitanyindexerrors !== undefined) {
    options.suppressImplicitAnyIndexErrors = toBool(opts.suppressimplicitanyindexerrors);
  }
  if (opts.noimplicitusestrict !== undefined) {
    options.noImplicitUseStrict = toBool(opts.noimplicitusestrict);
  }
  if (opts.nostrictgenericchecks !== undefined) {
    options.noStrictGenericChecks = toBool(opts.nostrictgenericchecks);
  }
  if (opts.keyofstringsonly !== undefined) {
    options.keyofStringsOnly = toBool(opts.keyofstringsonly);
  }

  // Max node module depth
  if (opts.maxnodemodulejsdepth !== undefined) {
    options.maxNodeModuleJsDepth = Number(opts.maxnodemodulejsdepth) || 0;
  }

  // Trace resolution (useful for debugging but doesn't affect diagnostics)
  if (opts.traceresolution !== undefined) {
    options.traceResolution = toBool(opts.traceresolution);
  }

  return options;
}

// ============================================================================
// Main TSC Runner
// ============================================================================

/**
 * Run TSC on a test case and return results.
 * 
 * @param testCase Parsed test case with files and options
 * @param libDir Directory containing lib.*.d.ts files
 * @param libSource Optional fallback lib.d.ts content
 * @param includeMessages If true, include full diagnostic messages (slower)
 */
export function runTsc(
  testCase: ParsedTestCase,
  libDir: string,
  libSource: string = '',
  includeMessages: boolean = false,
  rootFilePath: string = ''
): TscResult {
  const compilerOptions = toCompilerOptions(testCase.options);
  const sourceFiles = new Map<string, ts.SourceFile>();
  const fileNames: string[] = [];
  const libNames = getLibNamesForTestCase(testCase.options, compilerOptions.target);
  const libFiles = libNames.length ? collectLibFiles(libNames, libDir) : new Map<string, string>();

  // Use rootFilePath to resolve relative file names
  const rootDir = rootFilePath ? path.dirname(rootFilePath) : '.';

  // Determine if test uses absolute virtual paths (e.g. @Filename: /main.ts)
  const hasAbsoluteVirtualPaths = testCase.files.some(f => f.name.startsWith('/'));
  // For tests with absolute virtual paths, use '/' as the virtual root
  const virtualRoot = hasAbsoluteVirtualPaths ? '/' : rootDir;

  // Build a content map from all test files for the virtual filesystem.
  // This is separate from sourceFiles (which holds ts.SourceFile objects) because
  // module resolution uses readFile/fileExists, not getSourceFile.
  const virtualFileContents = new Map<string, string>();

  for (const file of testCase.files) {
    let scriptKind = ts.ScriptKind.TS;
    const lowerName = file.name.toLowerCase();
    if (lowerName.endsWith('.js') || lowerName.endsWith('.cjs') || lowerName.endsWith('.mjs')) {
      scriptKind = ts.ScriptKind.JS;
    } else if (lowerName.endsWith('.jsx')) {
      scriptKind = ts.ScriptKind.JSX;
    } else if (lowerName.endsWith('.tsx')) {
      scriptKind = ts.ScriptKind.TSX;
    } else if (lowerName.endsWith('.json')) {
      scriptKind = ts.ScriptKind.JSON;
    } else if (lowerName.endsWith('.d.ts') || lowerName.endsWith('.d.cts') || lowerName.endsWith('.d.mts')) {
      scriptKind = ts.ScriptKind.TS;
    } else if (lowerName.endsWith('.cts') || lowerName.endsWith('.mts')) {
      scriptKind = ts.ScriptKind.TS;
    }

    // Resolve file name relative to root directory
    const resolvedFileName = path.resolve(rootDir, file.name);
    virtualFileContents.set(resolvedFileName, file.content);

    const sf = ts.createSourceFile(
      resolvedFileName,
      file.content,
      compilerOptions.target ?? ts.ScriptTarget.ES2020,
      true,
      scriptKind
    );
    sourceFiles.set(resolvedFileName, sf);

    // Only add actual source files as root program files.
    // Declaration files (.d.ts), JSON files, and node_modules files should
    // be in the virtual filesystem but NOT passed as root files -
    // TypeScript discovers them through module resolution.
    const isDeclaration = lowerName.endsWith('.d.ts') || lowerName.endsWith('.d.cts') || lowerName.endsWith('.d.mts');
    const isJson = lowerName.endsWith('.json');
    const isInNodeModules = resolvedFileName.includes('/node_modules/');
    if (!isDeclaration && !isJson && !isInNodeModules) {
      fileNames.push(resolvedFileName);
    }
  }

  if (!compilerOptions.noLib && libFiles.size) {
    for (const [name, content] of libFiles.entries()) {
      sourceFiles.set(
        name,
        ts.createSourceFile(
          name,
          content,
          compilerOptions.target ?? ts.ScriptTarget.ES2020,
          true
        )
      );
    }
  } else if (!compilerOptions.noLib && libSource) {
    sourceFiles.set('lib.d.ts', ts.createSourceFile(
      'lib.d.ts',
      libSource,
      compilerOptions.target ?? ts.ScriptTarget.ES2020,
      true
    ));
  }

  // Build virtual directory structure from all known file paths.
  // This is critical for module resolution, which calls directoryExists()
  // to find node_modules, package directories, etc.
  const virtualDirs = new Set<string>();
  for (const filePath of sourceFiles.keys()) {
    let dir = path.dirname(filePath);
    while (dir && dir !== path.dirname(dir)) {
      if (virtualDirs.has(dir)) break;
      virtualDirs.add(dir);
      dir = path.dirname(dir);
    }
    // Add root
    virtualDirs.add(dir);
  }

  const host = ts.createCompilerHost(compilerOptions);

  host.getSourceFile = (name) => {
    return sourceFiles.get(name) ?? sourceFiles.get(path.basename(name));
  };

  host.fileExists = (name) => {
    if (sourceFiles.has(name)) return true;
    if (virtualFileContents.has(name)) return true;
    // Fallback: check by basename (for lib files)
    if (sourceFiles.has(path.basename(name))) return true;
    return false;
  };

  host.readFile = (name) => {
    // Check source files first (includes parsed TS/JS/JSON files)
    const sf = sourceFiles.get(name);
    if (sf) return sf.getFullText();

    // Check virtual file contents (raw content, handles all file types)
    if (virtualFileContents.has(name)) return virtualFileContents.get(name);

    // Try to find the file by resolving test file names
    const file = testCase.files.find(f => {
      const resolved = path.resolve(rootDir, f.name);
      return resolved === name;
    });
    if (file) return file.content;

    // Check lib files
    if (libFiles.has(name)) return libFiles.get(name);
    const baseName = path.basename(name);
    if (libFiles.has(baseName)) return libFiles.get(baseName);
    if (libSource && baseName === 'lib.d.ts') return libSource;

    return undefined;
  };

  // Virtual directory support - critical for module resolution.
  // Without this, TypeScript can't find node_modules/ or package directories
  // in the virtual filesystem, causing resolution failures and crashes.
  host.directoryExists = (dirName) => {
    return virtualDirs.has(dirName);
  };

  host.getDirectories = (dirName) => {
    const result: string[] = [];
    const prefix = dirName.endsWith('/') ? dirName : dirName + '/';
    for (const dir of virtualDirs) {
      if (dir.startsWith(prefix) && dir !== dirName) {
        const remaining = dir.slice(prefix.length);
        const firstPart = remaining.split('/')[0];
        if (firstPart && !result.includes(firstPart)) {
          result.push(firstPart);
        }
      }
    }
    return result;
  };

  host.realpath = (name) => name;

  host.getDefaultLibFileName = () => {
    // Return the correct default lib file for the target.
    // TypeScript uses getDefaultLibFileName as the root of the library
    // dependency graph - it only loads libs reachable via /// <reference>
    // from this file. If we always return lib.es5.d.ts, ES2015+ libs
    // (like lib.es2015.iterable.d.ts) are never discovered, causing
    // false TS2488 and other errors.
    const target = compilerOptions.target ?? ts.ScriptTarget.ES2020;
    const targetLibMap: Record<number, string> = {
      [ts.ScriptTarget.ES3]: 'lib.es5.d.ts',
      [ts.ScriptTarget.ES5]: 'lib.es5.d.ts',
      [ts.ScriptTarget.ES2015]: 'lib.es2015.d.ts',
      [ts.ScriptTarget.ES2016]: 'lib.es2016.d.ts',
      [ts.ScriptTarget.ES2017]: 'lib.es2017.d.ts',
      [ts.ScriptTarget.ES2018]: 'lib.es2018.d.ts',
      [ts.ScriptTarget.ES2019]: 'lib.es2019.d.ts',
      [ts.ScriptTarget.ES2020]: 'lib.es2020.d.ts',
      [ts.ScriptTarget.ES2021]: 'lib.es2021.d.ts',
      [ts.ScriptTarget.ES2022]: 'lib.es2022.d.ts',
    };
    const defaultLib = targetLibMap[target] ?? 'lib.esnext.d.ts';
    if (sourceFiles.has(defaultLib)) return defaultLib;
    // Fallback: if the target-specific lib isn't loaded, use es5
    if (sourceFiles.has('lib.es5.d.ts')) return 'lib.es5.d.ts';
    return 'lib.d.ts';
  };

  host.getCurrentDirectory = () => virtualRoot;
  host.getCanonicalFileName = (name) => name;
  host.useCaseSensitiveFileNames = () => true;
  host.getNewLine = () => '\n';
  host.writeFile = () => {};

  const program = ts.createProgram(fileNames, compilerOptions, host);
  const codes: number[] = [];
  const diagnostics: DiagnosticInfo[] = [];

  const processDiagnostic = (d: ts.Diagnostic) => {
    codes.push(d.code);
    if (includeMessages) {
      const message = ts.flattenDiagnosticMessageText(d.messageText, '\n');
      const info: DiagnosticInfo = { code: d.code, message };
      if (d.file && d.start !== undefined) {
        const { line, character } = d.file.getLineAndCharacterOfPosition(d.start);
        info.file = d.file.fileName;
        info.line = line + 1;
        info.column = character + 1;
      }
      diagnostics.push(info);
    }
  };

  // Only get diagnostics for files the program actually includes.
  // Using program.getSourceFiles() instead of our sourceFiles map avoids
  // crashes from querying diagnostics on files TypeScript excluded
  // (e.g. .js files when allowJs is false, .map files, etc.).
  for (const sf of program.getSourceFiles()) {
    if (sf.fileName.startsWith('lib.') && sf.fileName.endsWith('.d.ts')) continue;
    if (sf.isDeclarationFile && libFiles.has(sf.fileName)) continue;
    for (const d of program.getSyntacticDiagnostics(sf)) processDiagnostic(d);
    for (const d of program.getSemanticDiagnostics(sf)) processDiagnostic(d);
  }

  for (const d of program.getGlobalDiagnostics()) processDiagnostic(d);

  return { codes, diagnostics };
}

/**
 * Run TSC on files (map of filename -> content) and return results.
 * Convenience wrapper for use with CheckOptions-style input.
 */
export function runTscOnFiles(
  files: Record<string, string>,
  options: Record<string, unknown>,
  libDir: string,
  includeMessages: boolean = false
): TscResult {
  // Convert to ParsedTestCase format
  const testFiles: TestFile[] = Object.entries(files).map(([name, content]) => ({ name, content }));
  
  // Normalize option keys to lowercase for toCompilerOptions
  const testOptions: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(options)) {
    testOptions[key.toLowerCase()] = value;
  }

  const testCase: ParsedTestCase = {
    options: testOptions,
    isMultiFile: testFiles.length > 1,
    files: testFiles,
  };

  return runTsc(testCase, libDir, '', includeMessages);
}
