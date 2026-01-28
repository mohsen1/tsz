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
    path.resolve(__dirname, '../../lib-assets/lib_manifest.json'),
    path.resolve(__dirname, '../../src/lib-assets/lib_manifest.json'),
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
