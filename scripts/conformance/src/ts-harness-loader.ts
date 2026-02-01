/**
 * TypeScript Harness Loader
 *
 * This module loads TypeScript's test harness from the built submodule.
 * It provides access to TestCaseParser and other harness utilities.
 *
 * Prerequisites:
 * - TypeScript submodule must be built: cd TypeScript && npm ci && npx hereby tests --no-bundle
 *
 * Usage:
 *   import { loadHarness } from './ts-harness-loader.js';
 *   const { Harness, ts } = loadHarness();
 *   const parsed = Harness.TestCaseParser.makeUnitsFromTest(code, fileName);
 */

import * as path from 'path';
import * as fs from 'fs';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

// Path to TypeScript submodule
const TS_ROOT = path.resolve(__dirname, '../../../TypeScript');
const TS_BUILT = path.join(TS_ROOT, 'built/local');

/**
 * TypeScript's TestCaseParser types (from harnessIO.ts)
 */
export interface CompilerSettings {
  [name: string]: string;
}

export interface TestUnitData {
  content: string;
  name: string;
  fileOptions: CompilerSettings;
  originalFilePath: string;
  references: string[];
}

export interface TestCaseContent {
  settings: CompilerSettings;
  testUnitData: TestUnitData[];
  tsConfig: any; // ts.ParsedCommandLine
  tsConfigFileUnitData: TestUnitData | undefined;
  symlinks?: any; // vfs.FileSet
}

export interface TestCaseParser {
  extractCompilerSettings(content: string): CompilerSettings;
  makeUnitsFromTest(code: string, fileName: string, settings?: CompilerSettings): TestCaseContent;
  parseSymlinkFromTest(line: string, symlinks: any, absoluteRootDir?: string): any;
}

export interface HarnessModule {
  TestCaseParser: TestCaseParser;
  Compiler: any;
  Baseline: any;
  IO: any;
  // Add other exports as needed
}

/**
 * Check if TypeScript's harness is built
 */
export function isHarnessBuilt(): boolean {
  const harnessFile = path.join(TS_BUILT, 'harness/_namespaces/Harness.js');
  return fs.existsSync(harnessFile);
}

/**
 * Get the path to TypeScript's built harness
 */
export function getHarnessPath(): string {
  return path.join(TS_BUILT, 'harness/_namespaces/Harness.js');
}

/**
 * Get the path to TypeScript's built compiler
 */
export function getTsPath(): string {
  return path.join(TS_BUILT, 'harness/_namespaces/ts.js');
}

/**
 * Load TypeScript's harness modules.
 * Throws if the harness is not built.
 */
export function loadHarness(): { Harness: HarnessModule; ts: typeof import('typescript') } {
  if (!isHarnessBuilt()) {
    throw new Error(
      `TypeScript harness not built. Run:\n` +
      `  cd ${TS_ROOT}\n` +
      `  npm ci\n` +
      `  npx hereby tests --no-bundle`
    );
  }

  // Load the TypeScript compiler
  const ts = require(getTsPath());

  // Load the harness (includes TestCaseParser)
  const Harness = require(getHarnessPath()) as HarnessModule;

  return { Harness, ts };
}

/**
 * Lazily loaded harness - loads on first use
 */
let cachedHarness: { Harness: HarnessModule; ts: typeof import('typescript') } | null = null;

export function getHarness(): { Harness: HarnessModule; ts: typeof import('typescript') } {
  if (!cachedHarness) {
    cachedHarness = loadHarness();
  }
  return cachedHarness;
}

/**
 * Parse a test file using TypeScript's TestCaseParser.
 * This is a convenience wrapper around makeUnitsFromTest.
 */
export function parseTestFile(code: string, fileName: string): TestCaseContent {
  const { Harness } = getHarness();
  return Harness.TestCaseParser.makeUnitsFromTest(code, fileName);
}

/**
 * Extract compiler settings from test file content.
 * This is a convenience wrapper around extractCompilerSettings.
 */
export function extractSettings(content: string): CompilerSettings {
  const { Harness } = getHarness();
  return Harness.TestCaseParser.extractCompilerSettings(content);
}

// ============================================================================
// Harness directive utilities (not part of TypeScript's harness, but useful)
// ============================================================================

/** List of harness-specific directive names (not compiler options) */
const HARNESS_DIRECTIVES = new Set([
  'filename',
  'allownontextensions',
  'allownonttsextensions',
  'usecasesensitivefilenames',
  'nocheck',
  'typescriptversion',
  'baselinefile',
  'noerrortruncation',
  'suppressoutputpathcheck',
  'notypesandsymbols',
  'fullemitpaths',
  'reportdiagnostics',
  'capturesuggestions',
  'noimplicitreferences',
  'currentdirectory',
  'symlink',
  'link',
  'libfiles',
  'includebuiltfile',
]);

/**
 * Check if a setting name is a harness directive (not a compiler option).
 */
export function isHarnessDirective(name: string): boolean {
  return HARNESS_DIRECTIVES.has(name.toLowerCase());
}

/**
 * Separate harness directives from compiler options.
 */
export function separateSettings(settings: CompilerSettings): {
  harness: CompilerSettings;
  compiler: CompilerSettings;
} {
  const harness: CompilerSettings = {};
  const compiler: CompilerSettings = {};

  for (const [key, value] of Object.entries(settings)) {
    if (isHarnessDirective(key)) {
      harness[key] = value;
    } else {
      compiler[key] = value;
    }
  }

  return { harness, compiler };
}

/**
 * Check if a test should be skipped based on harness settings.
 */
export function shouldSkipTest(
  settings: CompilerSettings,
  targetTsVersion: string = '5.5.0'
): { skip: boolean; reason?: string } {
  // Skip @noCheck tests
  const noCheck = settings.noCheck || settings.nocheck;
  if (noCheck && noCheck.toLowerCase() === 'true') {
    return { skip: true, reason: 'noCheck' };
  }

  // Skip tests requiring newer TypeScript versions
  const tsVersion = settings.typeScriptVersion || settings.typescriptversion;
  if (tsVersion) {
    const required = parseVersion(tsVersion);
    const target = parseVersion(targetTsVersion);

    if (required && target) {
      if (
        required.major > target.major ||
        (required.major === target.major && required.minor > target.minor)
      ) {
        return { skip: true, reason: `requires TS ${tsVersion}` };
      }
    }
  }

  return { skip: false };
}

function parseVersion(v: string): { major: number; minor: number } | null {
  const cleaned = v.replace(/^[>=<~^]+\s*/, '').trim();
  const match = cleaned.match(/^(\d+)(?:\.(\d+))?/);
  if (!match) return null;
  return {
    major: parseInt(match[1], 10),
    minor: parseInt(match[2] || '0', 10),
  };
}
