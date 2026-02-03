/**
 * Lib Manifest utilities for conformance testing.
 *
 * Reads the generated lib_manifest.json to provide consistent lib resolution
 * between the Rust codebase and the conformance harness.
 */
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
/**
 * Load the lib manifest from lib-assets.
 */
export declare function loadLibManifest(): LibManifest | null;
/**
 * Normalize a lib name (handle aliases).
 */
export declare function normalizeLibName(name: string): string;
/**
 * Get the full lib name for a target (the *.full lib).
 */
export declare function getFullLibNameForTarget(target: string): string;
/**
 * Resolve a lib and all its dependencies in dependency order.
 * Uses the manifest references for resolution.
 */
export declare function resolveLibWithDependencies(name: string, manifest: LibManifest): string[];
/**
 * Get default libs for a target (without DOM).
 */
export declare function getLibsForTarget(target: string, manifest: LibManifest): string[];
/**
 * Get default libs for a target (with DOM).
 */
export declare function getDefaultLibsForTarget(target: string, manifest: LibManifest): string[];
/**
 * Resolve explicit lib names with dependencies.
 */
export declare function resolveExplicitLibs(libNames: string[], manifest: LibManifest): string[];
/**
 * Parse /// <reference lib="..." /> directives from lib file content.
 */
export declare function parseLibReferences(content: string): string[];
/**
 * Find the path to a lib file on disk.
 * Searches in the provided lib directories.
 */
export declare function findLibFilePath(libName: string, libDirs: string[]): string | null;
/**
 * Read lib file content with caching.
 */
export declare function readLibContent(libName: string, libDirs: string[]): string | null;
/**
 * Resolve lib with dependencies by reading files and parsing references.
 * Falls back to file-based resolution when manifest is unavailable.
 */
export declare function resolveLibWithDependenciesFromFiles(libName: string, libDirs: string[]): string[];
/**
 * Universal lib resolver - uses manifest if available, falls back to file parsing.
 */
export declare function resolveLibsUniversal(libNames: string[], libDirs: string[], manifest: LibManifest | null): string[];
/**
 * Get default lib name for a target (without dependencies, just the base name).
 */
export declare function getDefaultLibNameForTarget(target: string): string;
//# sourceMappingURL=lib-manifest.d.ts.map