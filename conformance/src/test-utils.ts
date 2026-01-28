/**
 * Shared test utilities for conformance testing.
 *
 * This module provides common functionality used by both server-mode
 * and WASM-mode conformance runners to ensure consistent behavior.
 */

import * as path from 'path';
import {
  loadLibManifest,
  resolveLibsUniversal,
  getDefaultLibNameForTarget,
  normalizeLibName,
  type LibManifest,
} from './lib-manifest.js';

// ============================================================================
// Test Directive Parsing
// ============================================================================

export interface ParsedDirectives {
  target?: string;
  lib?: string[];
  nolib?: boolean;
  strict?: boolean;
  strictnullchecks?: boolean;
  strictfunctiontypes?: boolean;
  strictpropertyinitialization?: boolean;
  noimplicitany?: boolean;
  noimplicitthis?: boolean;
  noimplicitreturns?: boolean;
  module?: string;
  moduleresolution?: string;
  jsx?: string;
  allowjs?: boolean;
  checkjs?: boolean;
  declaration?: boolean;
  isolatedmodules?: boolean;
  experimentaldecorators?: boolean;
  emitdecoratormetadata?: boolean;
  [key: string]: unknown;
}

export interface TestFile {
  name: string;
  content: string;
}

export interface ParsedTestCase {
  directives: ParsedDirectives;
  isMultiFile: boolean;
  files: TestFile[];
  category: string;
}

/**
 * Parse test directives from TypeScript conformance test file.
 * Extracts @target, @lib, @strict, etc. from comment headers.
 * Also handles @filename directives for multi-file tests.
 */
export function parseTestCase(code: string, filePath: string): ParsedTestCase {
  const lines = code.split('\n');
  const directives: ParsedDirectives = {};
  let isMultiFile = false;
  const files: TestFile[] = [];
  let currentFileName: string | null = null;
  let currentFileLines: string[] = [];
  const cleanLines: string[] = [];

  for (const line of lines) {
    const trimmed = line.trim();

    // Handle @filename directive for multi-file tests
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

    // Handle other @option directives
    const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
    if (optionMatch) {
      const [, key, value] = optionMatch;
      const lowKey = key.toLowerCase();
      if (value.toLowerCase() === 'true') directives[lowKey] = true;
      else if (value.toLowerCase() === 'false') directives[lowKey] = false;
      else if (!isNaN(Number(value))) directives[lowKey] = Number(value);
      else directives[lowKey] = value;
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
  if (!isMultiFile) {
    files.push({ name: path.basename(filePath), content: cleanLines.join('\n') });
  }

  // Extract category from path
  const relativePath = filePath.replace(/.*tests\/cases\//, '');
  const category = relativePath.split(path.sep)[0] || 'unknown';

  return { directives, isMultiFile, files, category };
}

/**
 * Parse just the directives (simpler version for server mode).
 */
export function parseDirectivesOnly(content: string): ParsedDirectives {
  const directives: ParsedDirectives = {};
  const lines = content.split('\n');

  for (const line of lines) {
    const trimmed = line.trim();
    // Stop parsing when we hit non-directive content
    if (!trimmed.startsWith('//') && trimmed.length > 0) {
      break;
    }

    const optionMatch = trimmed.match(/^\/\/\s*@(\w+):\s*(.+)$/i);
    if (optionMatch) {
      const [, key, value] = optionMatch;
      const lowKey = key.toLowerCase();
      if (value.toLowerCase() === 'true') directives[lowKey] = true;
      else if (value.toLowerCase() === 'false') directives[lowKey] = false;
      else if (!isNaN(Number(value))) directives[lowKey] = Number(value);
      else directives[lowKey] = value;
    }
  }

  return directives;
}

// ============================================================================
// Compiler Options Conversion
// ============================================================================

export interface CheckOptions {
  target?: string;
  lib?: string[];
  noLib?: boolean;
  strict?: boolean;
  strictNullChecks?: boolean;
  strictFunctionTypes?: boolean;
  strictPropertyInitialization?: boolean;
  noImplicitAny?: boolean;
  noImplicitThis?: boolean;
  noImplicitReturns?: boolean;
  module?: string;
  moduleResolution?: string;
  jsx?: string;
  allowJs?: boolean;
  checkJs?: boolean;
  declaration?: boolean;
  isolatedModules?: boolean;
  experimentalDecorators?: boolean;
  emitDecoratorMetadata?: boolean;
}

// Cached manifest
let cachedManifest: LibManifest | null | undefined = undefined;

function getManifest(): LibManifest | null {
  if (cachedManifest === undefined) {
    try {
      cachedManifest = loadLibManifest();
    } catch {
      cachedManifest = null;
    }
  }
  return cachedManifest;
}

/**
 * Convert parsed directives to CheckOptions for tsz-server or WASM.
 * Resolves lib dependencies automatically.
 */
export function directivesToCheckOptions(
  directives: ParsedDirectives,
  libDirs: string[] = []
): CheckOptions {
  const options: CheckOptions = {};
  const manifest = getManifest();

  // Target
  if (directives.target !== undefined) {
    options.target = String(directives.target);
  }

  // noLib
  if (directives.nolib !== undefined) {
    options.noLib = Boolean(directives.nolib);
  }

  // Resolve libs with dependencies
  if (!options.noLib) {
    let libNames: string[];

    if (directives.lib !== undefined) {
      // Explicit lib option
      const libVal = directives.lib;
      if (typeof libVal === 'string') {
        libNames = (libVal as string).split(',').map(s => s.trim().toLowerCase()).filter(Boolean);
      } else if (Array.isArray(libVal)) {
        libNames = libVal.map(s => String(s).trim().toLowerCase()).filter(Boolean);
      } else {
        libNames = [];
      }
    } else {
      // Derive from target
      const defaultLib = getDefaultLibNameForTarget(options.target || 'es5');
      libNames = [defaultLib];
    }

    // Use universal resolver (manifest or file-based fallback)
    options.lib = resolveLibsUniversal(libNames, libDirs, manifest);
  }

  // Strict mode flags
  if (directives.strict !== undefined) {
    options.strict = Boolean(directives.strict);
  }
  if (directives.strictnullchecks !== undefined) {
    options.strictNullChecks = Boolean(directives.strictnullchecks);
  }
  if (directives.strictfunctiontypes !== undefined) {
    options.strictFunctionTypes = Boolean(directives.strictfunctiontypes);
  }
  if (directives.strictpropertyinitialization !== undefined) {
    options.strictPropertyInitialization = Boolean(directives.strictpropertyinitialization);
  }
  if (directives.noimplicitany !== undefined) {
    options.noImplicitAny = Boolean(directives.noimplicitany);
  }
  if (directives.noimplicitthis !== undefined) {
    options.noImplicitThis = Boolean(directives.noimplicitthis);
  }
  if (directives.noimplicitreturns !== undefined) {
    options.noImplicitReturns = Boolean(directives.noimplicitreturns);
  }

  // Module options
  if (directives.module !== undefined) {
    options.module = String(directives.module);
  }
  if (directives.moduleresolution !== undefined) {
    options.moduleResolution = String(directives.moduleresolution);
  }

  // JSX
  if (directives.jsx !== undefined) {
    options.jsx = String(directives.jsx);
  }

  // JavaScript support
  if (directives.allowjs !== undefined) {
    options.allowJs = Boolean(directives.allowjs);
  }
  if (directives.checkjs !== undefined) {
    options.checkJs = Boolean(directives.checkjs);
  }

  // Declaration
  if (directives.declaration !== undefined) {
    options.declaration = Boolean(directives.declaration);
  }

  // Isolated modules
  if (directives.isolatedmodules !== undefined) {
    options.isolatedModules = Boolean(directives.isolatedmodules);
  }

  // Decorators
  if (directives.experimentaldecorators !== undefined) {
    options.experimentalDecorators = Boolean(directives.experimentaldecorators);
  }
  if (directives.emitdecoratormetadata !== undefined) {
    options.emitDecoratorMetadata = Boolean(directives.emitdecoratormetadata);
  }

  return options;
}

/**
 * Get lib names for a test case (used by both modes).
 */
export function getLibNamesForDirectives(
  directives: ParsedDirectives,
  libDirs: string[] = []
): string[] {
  if (directives.nolib) {
    return [];
  }

  const manifest = getManifest();
  let libNames: string[];

  if (directives.lib !== undefined) {
    const libVal = directives.lib;
    if (typeof libVal === 'string') {
      libNames = (libVal as string).split(',').map(s => normalizeLibName(s)).filter(Boolean);
    } else if (Array.isArray(libVal)) {
      libNames = libVal.map(s => normalizeLibName(String(s))).filter(Boolean);
    } else {
      libNames = [];
    }
  } else {
    const target = directives.target ? String(directives.target) : 'es5';
    const defaultLib = getDefaultLibNameForTarget(target);
    libNames = [defaultLib];
  }

  return resolveLibsUniversal(libNames, libDirs, manifest);
}
