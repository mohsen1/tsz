/**
 * TSC Baseline Cache System
 *
 * Pre-computes and caches TypeScript compiler results keyed by the TypeScript
 * submodule git SHA. When the submodule is updated, baselines are automatically
 * invalidated and regenerated.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export interface CacheEntry {
  codes: number[];
  hash: string;
}

export interface TscCacheData {
  version: number;
  typescriptSha: string;
  generatedAt: string;
  testCount: number;
  entries: Record<string, CacheEntry>;
}

const CACHE_VERSION = 4;

function getCacheDir(): string {
  return path.resolve(__dirname, '../.tsc-cache');
}

function getCacheFile(): string {
  return path.join(getCacheDir(), 'tsc-results.json');
}

export function getTypeScriptSha(rootDir: string): string | null {
  try {
    const tsDir = path.join(rootDir, 'TypeScript');
    const result = execSync('git rev-parse HEAD', {
      cwd: tsDir,
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'pipe'],
    }).trim();
    return result || null;
  } catch {
    try {
      const result = execSync('git ls-tree HEAD TypeScript', {
        cwd: rootDir,
        encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'pipe'],
      }).trim();
      const match = result.match(/^160000\s+commit\s+([a-f0-9]+)/);
      return match ? match[1] : null;
    } catch {
      return null;
    }
  }
}

export function hashContent(content: string): string {
  let hash = 2166136261;
  for (let i = 0; i < content.length; i++) {
    hash ^= content.charCodeAt(i);
    hash = (hash * 16777619) >>> 0;
  }
  return hash.toString(36);
}

export function loadTscCache(rootDir: string): TscCacheData | null {
  const cacheFile = getCacheFile();
  if (!fs.existsSync(cacheFile)) {
    return null;
  }

  try {
    const data = JSON.parse(fs.readFileSync(cacheFile, 'utf8')) as TscCacheData;
    if (data.version !== CACHE_VERSION) {
      console.log('Cache version mismatch (have v' + data.version + ', need v' + CACHE_VERSION + ')');
      return null;
    }

    // Validate SHA if git is available (skip in Docker where git isn't present)
    const currentSha = getTypeScriptSha(rootDir);
    if (currentSha && data.typescriptSha !== currentSha) {
      console.log('TypeScript submodule updated:');
      console.log('  Cached:  ' + data.typescriptSha.slice(0, 12));
      console.log('  Current: ' + currentSha.slice(0, 12));
      return null;
    }

    return data;
  } catch (e) {
    console.error('Failed to load TSC cache:', e);
    return null;
  }
}

export function saveTscCache(rootDir: string, entries: Record<string, CacheEntry>): boolean {
  const currentSha = getTypeScriptSha(rootDir);
  if (!currentSha) {
    console.error('Cannot save cache: TypeScript SHA unknown');
    return false;
  }

  const data: TscCacheData = {
    version: CACHE_VERSION,
    typescriptSha: currentSha,
    generatedAt: new Date().toISOString(),
    testCount: Object.keys(entries).length,
    entries,
  };

  try {
    const cacheDir = getCacheDir();
    if (!fs.existsSync(cacheDir)) {
      fs.mkdirSync(cacheDir, { recursive: true });
    }
    const cacheFile = getCacheFile();
    const tmpFile = cacheFile + '.tmp';
    fs.writeFileSync(tmpFile, JSON.stringify(data));
    fs.renameSync(tmpFile, cacheFile);
    return true;
  } catch (e) {
    console.error('Failed to save TSC cache:', e);
    return false;
  }
}

export function getCachedTscResult(
  cache: TscCacheData,
  relPath: string,
  content: string
): number[] | null {
  const entry = cache.entries[relPath];
  if (!entry) {
    return null;
  }
  const currentHash = hashContent(content);
  if (entry.hash !== currentHash) {
    return null;
  }
  return entry.codes;
}

export function getCacheStatus(rootDir: string): {
  valid: boolean;
  currentSha: string | null;
  cachedSha: string | null;
  testCount: number;
  generatedAt: string | null;
  cacheFile: string;
} {
  const currentSha = getTypeScriptSha(rootDir);
  const cacheFile = getCacheFile();

  if (!fs.existsSync(cacheFile)) {
    return { valid: false, currentSha, cachedSha: null, testCount: 0, generatedAt: null, cacheFile };
  }

  try {
    const data = JSON.parse(fs.readFileSync(cacheFile, 'utf8')) as TscCacheData;
    return {
      valid: data.typescriptSha === currentSha && data.version === CACHE_VERSION,
      currentSha,
      cachedSha: data.typescriptSha,
      testCount: data.testCount,
      generatedAt: data.generatedAt,
      cacheFile,
    };
  } catch {
    return { valid: false, currentSha, cachedSha: null, testCount: 0, generatedAt: null, cacheFile };
  }
}

export function clearTscCache(): boolean {
  try {
    const cacheFile = getCacheFile();
    if (fs.existsSync(cacheFile)) {
      fs.unlinkSync(cacheFile);
    }
    return true;
  } catch {
    return false;
  }
}

interface VersionMapping {
  npm: string;
  note?: string;
}

interface VersionMappings {
  mappings: Record<string, VersionMapping>;
  default: VersionMapping;
}

/**
 * Get the npm TypeScript version to use for a given submodule SHA.
 * Reads from typescript-versions.json mapping file.
 */
export function getTypescriptNpmVersion(submoduleSha: string | null): string {
  const mappingFile = path.resolve(__dirname, '../typescript-versions.json');

  try {
    const data = JSON.parse(fs.readFileSync(mappingFile, 'utf8')) as VersionMappings;

    if (submoduleSha && data.mappings[submoduleSha]) {
      return data.mappings[submoduleSha].npm;
    }

    // Check for partial SHA match (first 12 chars)
    if (submoduleSha) {
      const shortSha = submoduleSha.slice(0, 12);
      for (const [sha, mapping] of Object.entries(data.mappings)) {
        if (sha.startsWith(shortSha) || shortSha.startsWith(sha.slice(0, 12))) {
          return mapping.npm;
        }
      }
    }

    return data.default.npm;
  } catch {
    return '5.9.3'; // Hardcoded fallback
  }
}

/**
 * Check if the installed TypeScript version matches what we need.
 */
export function checkTypescriptVersion(rootDir: string): {
  installed: string | null;
  required: string;
  matches: boolean;
} {
  const submoduleSha = getTypeScriptSha(rootDir);
  const required = getTypescriptNpmVersion(submoduleSha);

  try {
    const pkgPath = path.resolve(__dirname, '../node_modules/typescript/package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    const installed = pkg.version;

    return {
      installed,
      required,
      matches: installed === required,
    };
  } catch {
    return {
      installed: null,
      required,
      matches: false,
    };
  }
}
