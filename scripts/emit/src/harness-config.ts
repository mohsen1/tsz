import * as fs from 'node:fs';
import * as path from 'node:path';

export type DirectoryCompilerOption = 'outDir' | 'declarationDir' | 'rootDir';

export interface CompilerOptionSources {
  variant: Readonly<Record<string, unknown>>;
  directives: Readonly<Record<string, unknown>>;
  embeddedConfig: Readonly<Record<string, unknown>>;
}

export interface ResolvedDirectoryCompilerOptions {
  outDir?: string;
  declarationDir?: string;
  rootDir?: string;
}

const directoryOptionNames: readonly DirectoryCompilerOption[] = [
  'outDir',
  'declarationDir',
  'rootDir',
];

function readStringOption(
  source: Readonly<Record<string, unknown>>,
  name: DirectoryCompilerOption,
): string | undefined {
  const lowerName = name.toLowerCase();
  const direct = source[name] ?? source[lowerName];
  if (typeof direct === 'string') return direct;

  const matchingKey = Object.keys(source).find(key => key.toLowerCase() === lowerName);
  const value = matchingKey === undefined ? undefined : source[matchingKey];
  return typeof value === 'string' ? value : undefined;
}

/** Resolve path-valued compiler options with the baseline runner's precedence. */
export function resolveDirectoryCompilerOptions(
  sources: CompilerOptionSources,
): ResolvedDirectoryCompilerOptions {
  const resolved: ResolvedDirectoryCompilerOptions = {};
  for (const name of directoryOptionNames) {
    const value =
      readStringOption(sources.variant, name)
      ?? readStringOption(sources.directives, name)
      ?? readStringOption(sources.embeddedConfig, name);
    if (value !== undefined) resolved[name] = value;
  }
  return resolved;
}

/**
 * Turn a TypeScript corpus path into a path relative to one staged test root.
 *
 * The upstream harness uses a virtual filesystem, so `/pkg/src/a.ts` and
 * `A:/src/a.ts` are corpus-local paths rather than host-absolute paths. Clamp
 * parent traversal for those virtual-absolute forms, preserve drive names as
 * ordinary path components, and retain cwd-relative `..` semantics.
 */
export function corpusRelativePath(value: string): string {
  const slashPath = value.replace(/\\/g, '/');
  const isVirtualAbsolute = slashPath.startsWith('/') || /^[A-Za-z]:\//.test(slashPath);
  const normalized = isVirtualAbsolute
    ? path.posix.normalize(`/${slashPath.replace(/^\/+/, '')}`).slice(1)
    : path.posix.normalize(slashPath);
  return normalized === '.' ? '' : normalized;
}

/** Map a corpus-virtual path into the physical root for one emit test. */
export function corpusPhysicalPath(testRoot: string, value: string): string {
  const physicalRoot = path.resolve(testRoot);
  const relative = corpusRelativePath(value);
  return relative === '' ? physicalRoot : path.resolve(physicalRoot, ...relative.split('/'));
}

/** Map resolved virtual directory options to one staged physical test root. */
export function physicalDirectoryCompilerOptions(
  testRoot: string,
  options: ResolvedDirectoryCompilerOptions,
): ResolvedDirectoryCompilerOptions {
  return {
    ...(options.outDir === undefined ? {} : { outDir: corpusPhysicalPath(testRoot, options.outDir) }),
    ...(options.declarationDir === undefined
      ? {}
      : { declarationDir: corpusPhysicalPath(testRoot, options.declarationDir) }),
    ...(options.rootDir === undefined ? {} : { rootDir: corpusPhysicalPath(testRoot, options.rootDir) }),
  };
}

/** Stable identity for preventing staged inputs from being mistaken for emit. */
export function physicalPathIdentity(value: string): string {
  const absolute = path.resolve(value);
  try {
    return fs.realpathSync.native(absolute);
  } catch {
    return absolute;
  }
}

export function isStagedInputPath(inputPathSet: ReadonlySet<string>, value: string): boolean {
  return inputPathSet.has(physicalPathIdentity(value));
}
