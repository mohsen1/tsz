import * as fs from 'node:fs';
import * as path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

interface OracleManifest {
  schemaVersion: number;
  packageName: string;
  platformPackagePrefix: string;
  version: string;
  versionOutput: string;
  gitHead: string;
  wrapperIntegrity: string;
  wrapperPackageJsonSha256: string;
  wrapperBinSha256: string;
  platforms: Record<string, {
    packageIntegrity: string;
    packageJsonSha256: string;
    packageTreeSha256: string;
    binarySha256: string;
  }>;
}

interface PackageMetadata {
  name?: string;
  version?: string;
  gitHead?: string;
  os?: string[];
  cpu?: string[];
}

export interface OracleProvenance {
  schemaVersion: 1;
  packageName: string;
  platformPackageName: string;
  version: string;
  gitHead: string;
  wrapperIntegrity: string;
  platformIntegrity: string;
  wrapperPackageJsonSha256: string;
  wrapperBinSha256: string;
  platformPackageJsonSha256: string;
  platformPackageTreeSha256: string;
  binarySha256: string;
  binaryPath: string;
  fingerprint: string;
}

export interface PinnedOracle {
  binaryPath: string;
  provenance: OracleProvenance;
}

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = path.resolve(__dirname, '../../..');

function sha256Bytes(bytes: Buffer | string): string {
  return createHash('sha256').update(bytes).digest('hex');
}

export function sha256File(filePath: string): string {
  if (!fs.existsSync(filePath)) throw new Error(`PINNED_TS7_ORACLE_MISSING: ${filePath}`);
  return sha256Bytes(fs.readFileSync(filePath));
}

/** Hash every regular file by sorted relative path and exact bytes. */
export function sha256Directory(directory: string): string {
  if (!fs.existsSync(directory)) throw new Error(`PINNED_TS7_ORACLE_MISSING: ${directory}`);
  const hash = createHash('sha256');
  const visit = (current: string): void => {
    const entries = fs.readdirSync(current, { withFileTypes: true })
      .sort((left, right) => left.name < right.name ? -1 : left.name > right.name ? 1 : 0);
    for (const entry of entries) {
      const entryPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        visit(entryPath);
      } else if (entry.isFile()) {
        hash.update(path.relative(directory, entryPath).split(path.sep).join('/'));
        hash.update('\0');
        hash.update(fs.readFileSync(entryPath));
        hash.update('\0');
      } else {
        throw new Error(`PINNED_TS7_ORACLE_UNSUPPORTED_PACKAGE_ENTRY: ${entryPath}`);
      }
    }
  };
  visit(directory);
  return hash.digest('hex');
}

function readJson<T>(filePath: string): T {
  if (!fs.existsSync(filePath)) throw new Error(`PINNED_TS7_ORACLE_MISSING: ${filePath}`);
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf8')) as T;
  } catch (error) {
    throw new Error(`PINNED_TS7_ORACLE_INVALID_JSON: ${filePath}: ${String(error)}`);
  }
}

function requireEqual(actual: unknown, expected: unknown, label: string): void {
  if (actual !== expected) {
    throw new Error(`PINNED_TS7_ORACLE_MISMATCH: ${label}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

export function verifyOracleExecutable(
  binaryPath: string,
  expectedVersionOutput: string,
  expectedSha256?: string,
): { versionOutput: string; sha256: string } {
  if (!fs.existsSync(binaryPath)) throw new Error(`PINNED_TS7_ORACLE_MISSING: ${binaryPath}`);
  let versionOutput: string;
  try {
    versionOutput = execFileSync(binaryPath, ['--version'], {
      encoding: 'utf8',
      timeout: 5_000,
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  } catch (error) {
    throw new Error(`PINNED_TS7_ORACLE_VERSION_PROBE_FAILED: ${String(error)}`);
  }
  requireEqual(versionOutput, expectedVersionOutput, 'binary --version');
  const sha256 = sha256File(binaryPath);
  if (expectedSha256 !== undefined) requireEqual(sha256, expectedSha256, 'binary sha256');
  return { versionOutput, sha256 };
}

export function resolvePinnedOracle(rootDir: string = DEFAULT_ROOT): PinnedOracle {
  const manifestPath = path.join(rootDir, 'scripts/emit/oracle-manifest.json');
  const manifest = readJson<OracleManifest>(manifestPath);
  requireEqual(manifest.schemaVersion, 1, 'manifest schemaVersion');
  requireEqual(manifest.packageName, 'typescript', 'manifest packageName');

  const scriptsDir = path.join(rootDir, 'scripts');
  const nodeModules = path.join(scriptsDir, 'node_modules');
  const wrapperDir = path.join(nodeModules, manifest.packageName);
  const wrapperPackagePath = path.join(wrapperDir, 'package.json');
  const wrapperBinPath = path.join(wrapperDir, 'bin', process.platform === 'win32' ? 'tsc.cmd' : 'tsc');
  const wrapperMetadata = readJson<PackageMetadata>(wrapperPackagePath);
  requireEqual(wrapperMetadata.name, manifest.packageName, 'wrapper package name');
  requireEqual(wrapperMetadata.version, manifest.version, 'wrapper package version');
  requireEqual(wrapperMetadata.gitHead, manifest.gitHead, 'wrapper package gitHead');
  requireEqual(sha256File(wrapperPackagePath), manifest.wrapperPackageJsonSha256, 'wrapper package.json sha256');
  requireEqual(sha256File(wrapperBinPath), manifest.wrapperBinSha256, 'wrapper bin sha256');

  const platformSuffix = `${process.platform}-${process.arch}`;
  const trustedPlatform = manifest.platforms[platformSuffix];
  if (!trustedPlatform) {
    throw new Error(`PINNED_TS7_ORACLE_UNSUPPORTED_PLATFORM: ${platformSuffix}`);
  }
  const platformPackageName = `${manifest.platformPackagePrefix}${platformSuffix}`;
  let platformPackagePath: string;
  try {
    platformPackagePath = createRequire(wrapperPackagePath).resolve(`${platformPackageName}/package.json`);
  } catch (error) {
    throw new Error(`PINNED_TS7_ORACLE_PLATFORM_PACKAGE_MISSING: ${platformPackageName}: ${String(error)}`);
  }
  const platformMetadata = readJson<PackageMetadata>(platformPackagePath);
  requireEqual(platformMetadata.name, platformPackageName, 'platform package name');
  requireEqual(platformMetadata.version, manifest.version, 'platform package version');
  requireEqual(platformMetadata.gitHead, manifest.gitHead, 'platform package gitHead');
  requireEqual(sha256File(platformPackagePath), trustedPlatform.packageJsonSha256, 'platform package.json sha256');
  if (!platformMetadata.os?.includes(process.platform)) {
    throw new Error(`PINNED_TS7_ORACLE_MISMATCH: platform package os excludes ${process.platform}`);
  }
  if (!platformMetadata.cpu?.includes(process.arch)) {
    throw new Error(`PINNED_TS7_ORACLE_MISMATCH: platform package cpu excludes ${process.arch}`);
  }

  const platformDir = path.dirname(platformPackagePath);
  requireEqual(sha256Directory(platformDir), trustedPlatform.packageTreeSha256, 'platform package tree sha256');
  const binaryPath = path.join(platformDir, 'lib', process.platform === 'win32' ? 'tsc.exe' : 'tsc');
  const executable = verifyOracleExecutable(binaryPath, manifest.versionOutput, trustedPlatform.binarySha256);
  const provenanceBase = {
    schemaVersion: 1 as const,
    packageName: manifest.packageName,
    platformPackageName,
    version: manifest.version,
    gitHead: manifest.gitHead,
    wrapperIntegrity: manifest.wrapperIntegrity,
    platformIntegrity: trustedPlatform.packageIntegrity,
    wrapperPackageJsonSha256: manifest.wrapperPackageJsonSha256,
    wrapperBinSha256: manifest.wrapperBinSha256,
    platformPackageJsonSha256: trustedPlatform.packageJsonSha256,
    platformPackageTreeSha256: trustedPlatform.packageTreeSha256,
    binarySha256: executable.sha256,
    binaryPath: path.relative(rootDir, binaryPath).split(path.sep).join('/'),
  };
  const fingerprint = `sha256:${sha256Bytes(JSON.stringify(provenanceBase))}`;
  return {
    binaryPath,
    provenance: { ...provenanceBase, fingerprint },
  };
}
