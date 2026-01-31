/**
 * Lib Manifest utilities for conformance testing.
 *
 * Reads the generated lib_manifest.json to provide consistent lib resolution
 * between the Rust codebase and the conformance harness.
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export interface LibEntry {
  fileName: string;
  canonicalFileName: string;
  references: string[];
  size: number;
}

export interface LibManifest {
  version: string;
  source: string;
  generatedAt: string;
  libs: Record<string, LibEntry>;
}

let cachedManifest: LibManifest | null = null;

/**
 * Load the lib manifest from lib-assets.
 */
export function loadLibManifest(): LibManifest | null {
  if (cachedManifest) {
    return cachedManifest;
  }

  const manifestPaths = [
    path.resolve(__dirname, '../../../lib-assets/lib_manifest.json'),
    path.resolve(__dirname, '../../../src/lib-assets/lib_manifest.json'),
  ];

  for (const manifestPath of manifestPaths) {
    try {
      if (fs.existsSync(manifestPath)) {
        const content = fs.readFileSync(manifestPath, 'utf8');
        cachedManifest = JSON.parse(content) as LibManifest;
        return cachedManifest;
      }
    } catch {
      // Continue to next path
    }
  }

  return null;
}

/**
 * Normalize a lib name (handle aliases).
 */
export function normalizeLibName(name: string): string {
  const lower = name.toLowerCase().trim();
  switch (lower) {
    case 'lib':
      return 'es5';
    case 'es6':
      return 'es2015';
    default:
      return lower;
  }
}

/**
 * Get the full lib name for a target (the *.full lib).
 */
export function getFullLibNameForTarget(target: string): string {
  const normalized = target.toLowerCase();
  switch (normalized) {
    case 'es3':
    case 'es5':
      return 'es5.full';
    case 'es6':
    case 'es2015':
      return 'es2015.full';
    default:
      if (normalized.startsWith('es20')) {
        return `${normalized}.full`;
      }
      return 'esnext.full';
  }
}

/**
 * Resolve a lib and all its dependencies in dependency order.
 * Uses the manifest references for resolution.
 */
export function resolveLibWithDependencies(
  name: string,
  manifest: LibManifest
): string[] {
  const resolved: string[] = [];
  const seen = new Set<string>();

  function resolveRecursive(libName: string): void {
    const normalized = normalizeLibName(libName);
    if (seen.has(normalized)) {
      return;
    }
    seen.add(normalized);

    const entry = manifest.libs[normalized];
    if (!entry) {
      return;
    }

    // Resolve dependencies first
    for (const dep of entry.references) {
      resolveRecursive(dep);
    }

    resolved.push(normalized);
  }

  resolveRecursive(name);
  return resolved;
}

/**
 * Get default libs for a target (without DOM).
 */
export function getLibsForTarget(
  target: string,
  manifest: LibManifest
): string[] {
  const baseLib = normalizeLibName(target);
  return resolveLibWithDependencies(baseLib, manifest);
}

/**
 * Get default libs for a target (with DOM).
 */
export function getDefaultLibsForTarget(
  target: string,
  manifest: LibManifest
): string[] {
  const fullLibName = getFullLibNameForTarget(target);
  return resolveLibWithDependencies(fullLibName, manifest);
}

/**
 * Resolve explicit lib names with dependencies.
 */
export function resolveExplicitLibs(
  libNames: string[],
  manifest: LibManifest
): string[] {
  const resolved: string[] = [];
  const seen = new Set<string>();

  for (const name of libNames) {
    const deps = resolveLibWithDependencies(name, manifest);
    for (const dep of deps) {
      if (!seen.has(dep)) {
        seen.add(dep);
        resolved.push(dep);
      }
    }
  }

  return resolved;
}

// ============================================================================
// File-based lib resolution (fallback when manifest not available)
// ============================================================================

const LIB_REFERENCE_RE = /\/\/\/\s*<reference\s+lib=["']([^"']+)["']\s*\/>/g;

/**
 * Parse /// <reference lib="..." /> directives from lib file content.
 */
export function parseLibReferences(content: string): string[] {
  const refs: string[] = [];
  for (const match of content.matchAll(LIB_REFERENCE_RE)) {
    if (match[1]) {
      refs.push(normalizeLibName(match[1]));
    }
  }
  return refs;
}

// Caches for file-based resolution
const libContentCache = new Map<string, string>();
const libPathCache = new Map<string, string | null>();

/**
 * Find the path to a lib file on disk.
 * Searches in the provided lib directories.
 */
export function findLibFilePath(libName: string, libDirs: string[]): string | null {
  const normalized = normalizeLibName(libName);
  const cacheKey = `${normalized}:${libDirs.join(',')}`;

  if (libPathCache.has(cacheKey)) {
    return libPathCache.get(cacheKey)!;
  }

  for (const libDir of libDirs) {
    const candidates = [
      path.join(libDir, `lib.${normalized}.d.ts`),
      path.join(libDir, `${normalized}.d.ts`),
      path.join(libDir, `${normalized}.generated.d.ts`),
    ];

    for (const candidate of candidates) {
      if (fs.existsSync(candidate)) {
        libPathCache.set(cacheKey, candidate);
        return candidate;
      }
    }
  }

  libPathCache.set(cacheKey, null);
  return null;
}

/**
 * Read lib file content with caching.
 */
export function readLibContent(libName: string, libDirs: string[]): string | null {
  const normalized = normalizeLibName(libName);

  if (libContentCache.has(normalized)) {
    return libContentCache.get(normalized)!;
  }

  const libPath = findLibFilePath(normalized, libDirs);
  if (!libPath) {
    return null;
  }

  try {
    const content = fs.readFileSync(libPath, 'utf8');
    libContentCache.set(normalized, content);
    return content;
  } catch {
    return null;
  }
}

/**
 * Resolve lib with dependencies by reading files and parsing references.
 * Falls back to file-based resolution when manifest is unavailable.
 */
export function resolveLibWithDependenciesFromFiles(
  libName: string,
  libDirs: string[]
): string[] {
  const resolved: string[] = [];
  const seen = new Set<string>();

  function resolveRecursive(name: string): void {
    const normalized = normalizeLibName(name);
    if (seen.has(normalized)) {
      return;
    }
    seen.add(normalized);

    const content = readLibContent(normalized, libDirs);
    if (!content) {
      // Still add to resolved list - it may be loaded from embedded libs
      resolved.push(normalized);
      return;
    }

    // Resolve dependencies first (depth-first)
    const refs = parseLibReferences(content);
    for (const ref of refs) {
      resolveRecursive(ref);
    }

    resolved.push(normalized);
  }

  resolveRecursive(libName);
  return resolved;
}

/**
 * Universal lib resolver - uses manifest if available, falls back to file parsing.
 */
export function resolveLibsUniversal(
  libNames: string[],
  libDirs: string[],
  manifest: LibManifest | null
): string[] {
  const resolved: string[] = [];
  const seen = new Set<string>();

  for (const name of libNames) {
    let deps: string[];

    if (manifest) {
      // Prefer manifest for accurate dependency info
      deps = resolveLibWithDependencies(name, manifest);
    } else {
      // Fall back to file-based resolution
      deps = resolveLibWithDependenciesFromFiles(name, libDirs);
    }

    for (const dep of deps) {
      if (!seen.has(dep)) {
        seen.add(dep);
        resolved.push(dep);
      }
    }
  }

  return resolved;
}

/**
 * Get default lib name for a target (without dependencies, just the base name).
 */
export function getDefaultLibNameForTarget(target: string): string {
  const t = target.toLowerCase();
  switch (t) {
    case 'es3':
    case 'es5':
      return 'es5';
    case 'es6':
      return 'es2015';
    default:
      if (t.startsWith('es20') || t === 'esnext') {
        return t;
      }
      return 'es5';
  }
}
