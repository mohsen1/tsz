import { parse as parseJsonc, type ParseError } from 'jsonc-parser';

export type AuthoredOptionSource = 'embedded-config' | 'directive' | 'filename-variant';

export interface AuthoredOption {
  key: string;
  value: unknown;
  source: AuthoredOptionSource;
}

export interface AuthoredOptionSources {
  variant: Readonly<Record<string, unknown>>;
  directives: Readonly<Record<string, unknown>>;
  embeddedConfig: Readonly<Record<string, unknown>>;
}

export function extractAuthoredVariantFromFilename(
  filename: string,
): { base: string } & Record<string, string | undefined> {
  const match = filename.match(/^(.+?)\(([^)]+)\)\.js$/);
  if (!match) return { base: filename.replace(/\.js$/, '') };

  const result: { base: string } & Record<string, string | undefined> = { base: match[1] };
  for (const rawVariant of match[2].split(',')) {
    const variant = rawVariant;
    const separator = variant.indexOf('=');
    const key = separator < 0 ? variant : variant.slice(0, separator);
    const value = separator < 0 ? undefined : variant.slice(separator + 1);
    result[key.toLowerCase()] = value;
  }
  return result;
}

const BOOLEAN_FORWARDED_OPTIONS = new Set([
  'allowjs',
  'allowunreachablecode',
  'alwaysstrict',
  'checkjs',
  'declaration',
  'declarationmap',
  'downleveliteration',
  'emitdeclarationonly',
  'emitdecoratormetadata',
  'esmoduleinterop',
  'exactoptionalpropertytypes',
  'experimentaldecorators',
  'importhelpers',
  'inlinesourcemap',
  'isolatedmodules',
  'nocheck',
  'noemit',
  'noemithelpers',
  'noemitonerror',
  'noimplicitany',
  'nolib',
  'nounusedlocals',
  'nounusedparameters',
  'preserveconstenums',
  'preservevalueimports',
  'removecomments',
  'rewriterelativeimportextensions',
  'sourcemap',
  'skiplibcheck',
  'strict',
  'strictnullchecks',
  'strictpropertyinitialization',
  'stripinternal',
  'usedefineforclassfields',
  'verbatimmodulesyntax',
]);

const STRING_FORWARDED_OPTIONS = new Set([
  'baseurl',
  'declarationdir',
  'importsnotusedasvalues',
  'jsx',
  'jsxfactory',
  'jsxfragmentfactory',
  'jsximportsource',
  'module',
  'moduledetection',
  'moduleresolution',
  'outdir',
  'outfile',
  'rootdir',
  'target',
]);

const TARGET_VALUES = new Set([
  'es3', 'es5', 'es6', 'es2015', 'es2016', 'es2017', 'es2018', 'es2019',
  'es2020', 'es2021', 'es2022', 'es2023', 'es2024', 'es2025', 'esnext',
]);
const MODULE_VALUES = new Set([
  'none', 'commonjs', 'amd', 'umd', 'system', 'es6', 'es2015', 'es2020',
  'es2022', 'esnext', 'node16', 'node18', 'node20', 'nodenext', 'preserve',
]);

/** Options whose exact value is forwarded by cli-transpiler. */
const FORWARDED_OPTIONS = new Set([
  ...BOOLEAN_FORWARDED_OPTIONS,
  ...STRING_FORWARDED_OPTIONS,
  'lib',
]);

/** Options retained as named terminal unsupported rows elsewhere. */
const QUARANTINED_OPTIONS = new Set([
  'inlinesources',
  'maproot',
  'sourceroot',
]);

/** Harness directive implemented structurally rather than as compiler argv. */
const HARNESS_OPTIONS = new Set([
  'base',
  'noimplicitreferences',
  'notypesandsymbols',
]);

function normalizedEntries(source: Readonly<Record<string, unknown>>): Array<[string, unknown]> {
  return Object.entries(source)
    .map(([key, value]) => [key.toLowerCase(), value]);
}

export type AuthoredOptionDisposition =
  | 'forwarded'
  | 'quarantined'
  | 'harness-only'
  | 'unhandled';

export function authoredOptionDisposition(key: string): AuthoredOptionDisposition {
  const normalized = key.toLowerCase();
  if (FORWARDED_OPTIONS.has(normalized)) return 'forwarded';
  if (QUARANTINED_OPTIONS.has(normalized)) return 'quarantined';
  if (HARNESS_OPTIONS.has(normalized)) return 'harness-only';
  return 'unhandled';
}

/**
 * Resolve every authored option once. Embedded config has lowest precedence,
 * source directives override it, and filename variants select the concrete
 * baseline permutation.
 */
export function resolveAuthoredOptions(sources: AuthoredOptionSources): Map<string, AuthoredOption> {
  const resolved = new Map<string, AuthoredOption>();
  const merge = (source: Readonly<Record<string, unknown>>, provenance: AuthoredOptionSource): void => {
    for (const [key, value] of normalizedEntries(source)) {
      resolved.set(key, { key, value, source: provenance });
    }
  };
  merge(sources.embeddedConfig, 'embedded-config');
  merge(sources.directives, 'directive');
  merge(sources.variant, 'filename-variant');
  return resolved;
}

export function unhandledAuthoredOptions(options: ReadonlyMap<string, AuthoredOption>): AuthoredOption[] {
  return [...options.values()]
    .filter(option => authoredOptionDisposition(option.key) === 'unhandled')
    .sort((left, right) => left.key < right.key ? -1 : left.key > right.key ? 1 : 0);
}

function isExactBoolean(option: AuthoredOption): boolean {
  if (typeof option.value === 'boolean') return true;
  return option.source === 'filename-variant' &&
    (option.value === 'true' || option.value === 'false');
}

function isNonEmptyString(value: unknown): boolean {
  return typeof value === 'string' && value.length > 0;
}

function isValidLibList(option: AuthoredOption): boolean {
  if (option.source === 'embedded-config') {
    return Array.isArray(option.value) &&
      option.value.length > 0 &&
      option.value.every(value =>
        typeof value === 'string' &&
        value.length > 0 &&
        !/^\s|\s$/.test(value) &&
        !value.includes(','));
  }
  if (typeof option.value !== 'string') return false;
  if (option.source === 'filename-variant') return option.value.length > 0 && !option.value.includes(',');
  return option.value.split(',').some(part => part.trim().length > 0);
}

/** Forwarded/harness values that cannot be represented losslessly on argv. */
export function invalidAuthoredOptions(options: ReadonlyMap<string, AuthoredOption>): AuthoredOption[] {
  return [...options.values()].filter(option => {
    const disposition = authoredOptionDisposition(option.key);
    if (disposition === 'unhandled' || disposition === 'quarantined') return false;
    if (
      BOOLEAN_FORWARDED_OPTIONS.has(option.key) ||
      option.key === 'noimplicitreferences' ||
      option.key === 'notypesandsymbols'
    ) {
      if (
        (option.key === 'noimplicitreferences' || option.key === 'notypesandsymbols') &&
        option.source === 'embedded-config'
      ) return true;
      return !isExactBoolean(option);
    }
    if (STRING_FORWARDED_OPTIONS.has(option.key) || option.key === 'base') {
      if (!isNonEmptyString(option.value)) return true;
      const normalized = (option.value as string).toLowerCase();
      if (option.source === 'directive' && normalized.includes(',')) return true;
      if (option.key === 'target') return !TARGET_VALUES.has(normalized);
      if (option.key === 'module') return !MODULE_VALUES.has(normalized);
      return false;
    }
    if (option.key === 'lib') return !isValidLibList(option);
    return true;
  }).sort((left, right) => left.key < right.key ? -1 : left.key > right.key ? 1 : 0);
}

export function authoredOptionFailureReasons(
  options: ReadonlyMap<string, AuthoredOption>,
): string[] {
  return [
    ...invalidAuthoredOptions(options).map(option =>
      `invalid-authored-option:${option.key}(${option.source})`),
    ...unhandledAuthoredOptions(options).map(option =>
      `unhandled-authored-option:${option.key}(${option.source})`),
  ];
}

export function optionValue(
  options: ReadonlyMap<string, AuthoredOption>,
  key: string,
): unknown {
  return options.get(key.toLowerCase())?.value;
}

export function optionBoolean(
  options: ReadonlyMap<string, AuthoredOption>,
  key: string,
): boolean | undefined {
  const option = options.get(key.toLowerCase());
  if (!option) return undefined;
  if (typeof option.value === 'boolean') return option.value;
  if (option.source !== 'filename-variant') return undefined;
  if (option.value === 'true') return true;
  if (option.value === 'false') return false;
  return undefined;
}

export function optionString(
  options: ReadonlyMap<string, AuthoredOption>,
  key: string,
): string | undefined {
  const value = optionValue(options, key);
  if (typeof value !== 'string') return undefined;
  return value;
}

export function optionLibList(
  options: ReadonlyMap<string, AuthoredOption>,
): string[] | undefined {
  const option = options.get('lib');
  if (!option) return undefined;
  if (option.source === 'embedded-config') {
    return Array.isArray(option.value) && option.value.every(value => typeof value === 'string')
      ? [...option.value] as string[]
      : undefined;
  }
  if (typeof option.value !== 'string') return undefined;
  if (option.source === 'filename-variant') return [option.value];
  return option.value.split(',').map(part => part.trim()).filter(part => part.length > 0);
}

export interface EmbeddedConfigResult {
  compilerOptions: Record<string, unknown>;
  reasons: string[];
  configFileNames: string[];
}

function stableJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableJson).join(',')}]`;
  if (value && typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
    return `{${entries.map(([key, child]) => `${JSON.stringify(key)}:${stableJson(child)}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

/** Parse embedded tsconfig JSONC without accepting partial or conflicting options. */
export function parseEmbeddedCompilerOptions(
  sourceFiles: ReadonlyArray<{ name: string; content: string }>,
): EmbeddedConfigResult {
  let selected: { name: string; options: Record<string, unknown>; fingerprint: string } | undefined;
  const reasons: string[] = [];
  const configFileNames: string[] = [];

  for (const sourceFile of sourceFiles) {
    if (!sourceFile.name.toLowerCase().endsWith('tsconfig.json')) continue;
    configFileNames.push(sourceFile.name);
    const errors: ParseError[] = [];
    const parsed = parseJsonc(sourceFile.content, errors, {
      allowTrailingComma: true,
      disallowComments: false,
    }) as unknown;
    if (errors.length > 0) {
      reasons.push(`embedded-tsconfig-jsonc-parse-error:${sourceFile.name}`);
      continue;
    }
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      reasons.push(`embedded-tsconfig-invalid-root:${sourceFile.name}`);
      continue;
    }
    const configObject = parsed as Record<string, unknown>;
    for (const key of Object.keys(configObject)) {
      if (key !== 'compilerOptions' && key !== '$schema') {
        reasons.push(`unhandled-embedded-tsconfig-field:${key}(${sourceFile.name})`);
      }
    }
    const compilerOptions = configObject.compilerOptions;
    if (compilerOptions === undefined) continue;
    if (!compilerOptions || typeof compilerOptions !== 'object' || Array.isArray(compilerOptions)) {
      reasons.push(`embedded-tsconfig-invalid-compiler-options:${sourceFile.name}`);
      continue;
    }

    const options = compilerOptions as Record<string, unknown>;
    const fingerprint = stableJson(options);
    if (!selected) {
      selected = { name: sourceFile.name, options, fingerprint };
    } else if (selected.fingerprint !== fingerprint) {
      reasons.push(`conflicting-embedded-tsconfigs:${selected.name}|${sourceFile.name}`);
    }
  }

  return { compilerOptions: selected?.options ?? {}, reasons, configFileNames };
}
