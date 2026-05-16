#!/usr/bin/env node
/**
 * Bake the TypeScript baseline corpus (~13,800 .js + .d.ts files,
 * ~477 MB on disk) into a single binary blob + manifest so the emit
 * runner can open once and slice into a single buffer instead of
 * doing per-file `fs.readFile` calls.
 *
 * Output:
 *   scripts/emit/.baseline-blob-cache/baselines.bin   (concatenated bytes)
 *   scripts/emit/.baseline-blob-cache/baselines.idx.json   (name → {offset, length})
 *   scripts/emit/.baseline-blob-cache/baselines.meta.json  (sha + count + bytes)
 *
 * The runner checks the manifest's directory hash against the live
 * baselines directory's `readdir` output on startup; if it doesn't
 * match, it falls back to per-file reads.
 *
 * Invoke explicitly:
 *   node scripts/emit/dist/build-baseline-blob.js
 *
 * Or via the runner: the runner triggers a rebuild itself when the
 * blob is missing or stale.
 *
 * Time / size on first build of the current TypeScript baseline corpus
 * (M4 Max, warm cache): ~2-4 s wall, ~480 MB on disk.
 */
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const TS_DIR = path.join(__dirname, '..', '..', '..', 'TypeScript');
const BASELINES_DIR = path.join(TS_DIR, 'tests', 'baselines', 'reference');
const OUT_DIR = path.join(__dirname, '..', '.baseline-blob-cache');
const OUT_BIN = path.join(OUT_DIR, 'baselines.bin');
const OUT_IDX = path.join(OUT_DIR, 'baselines.idx.json');
const OUT_META = path.join(OUT_DIR, 'baselines.meta.json');

const BLOB_FORMAT_VERSION = 1;

export interface BlobIndex {
  version: number;
  entries: Record<string, { offset: number; length: number }>;
}

export interface BlobMeta {
  version: number;
  fileCount: number;
  bytes: number;
  directoryHash: string;
  builtAt: string;
}

/**
 * Hash the sorted list of baseline file names. Cheap proxy for "the
 * directory hasn't been re-pinned to a different TypeScript commit".
 * Does NOT cover file content changes — for that the consumer needs
 * to invalidate explicitly (e.g. on TypeScript submodule update).
 */
export function directoryHash(entries: readonly string[]): string {
  const h = crypto.createHash('sha256');
  for (const name of entries) {
    h.update(name);
    h.update('\0');
  }
  return h.digest('hex');
}

export async function listBaselineEntries(baselinesDir: string): Promise<string[]> {
  const all = await fs.readdir(baselinesDir);
  return all
    .filter((name) => name.endsWith('.js') || name.endsWith('.d.ts'))
    .sort();
}

export async function buildBlob(
  baselinesDir: string,
  outBin: string,
  outIdx: string,
  outMeta: string,
): Promise<{ entries: number; bytes: number; durationMs: number }> {
  const t0 = Date.now();
  await fs.mkdir(path.dirname(outBin), { recursive: true });

  const entries = await listBaselineEntries(baselinesDir);
  const chunks: Buffer[] = [];
  const index: BlobIndex = { version: BLOB_FORMAT_VERSION, entries: {} };
  let offset = 0;
  for (const name of entries) {
    const data = await fs.readFile(path.join(baselinesDir, name));
    index.entries[name] = { offset, length: data.length };
    chunks.push(data);
    offset += data.length;
  }

  await fs.writeFile(outBin, Buffer.concat(chunks));
  await fs.writeFile(outIdx, JSON.stringify(index));
  const meta: BlobMeta = {
    version: BLOB_FORMAT_VERSION,
    fileCount: entries.length,
    bytes: offset,
    directoryHash: directoryHash(entries),
    builtAt: new Date().toISOString(),
  };
  await fs.writeFile(outMeta, JSON.stringify(meta, null, 2));

  return { entries: entries.length, bytes: offset, durationMs: Date.now() - t0 };
}

export async function readBlobMeta(outMeta: string): Promise<BlobMeta | null> {
  try {
    const raw = await fs.readFile(outMeta, 'utf-8');
    const parsed = JSON.parse(raw) as BlobMeta;
    if (parsed.version !== BLOB_FORMAT_VERSION) return null;
    return parsed;
  } catch {
    return null;
  }
}

export async function isBlobFresh(baselinesDir: string, outMeta: string): Promise<boolean> {
  const meta = await readBlobMeta(outMeta);
  if (!meta) return false;
  const entries = await listBaselineEntries(baselinesDir);
  return meta.directoryHash === directoryHash(entries);
}

async function main(): Promise<void> {
  if (!await fs.stat(BASELINES_DIR).then(() => true, () => false)) {
    console.error(`Baselines directory not found: ${BASELINES_DIR}`);
    process.exit(1);
  }

  if (await isBlobFresh(BASELINES_DIR, OUT_META)) {
    const meta = await readBlobMeta(OUT_META);
    console.log(
      `Baseline blob already fresh (${meta?.fileCount ?? '?'} files, ` +
      `${meta?.bytes ?? '?'} bytes, built ${meta?.builtAt ?? '?'})`,
    );
    return;
  }

  const result = await buildBlob(BASELINES_DIR, OUT_BIN, OUT_IDX, OUT_META);
  console.log(
    `Built baseline blob: ${result.entries} entries, ` +
    `${(result.bytes / 1024 / 1024).toFixed(1)} MB, ${result.durationMs} ms`,
  );
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === __filename;
if (invokedDirectly) {
  main().catch((err) => {
    console.error('build-baseline-blob failed:', err);
    process.exit(2);
  });
}
