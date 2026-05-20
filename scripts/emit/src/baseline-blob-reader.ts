/**
 * Runtime reader for the baseline blob produced by build-baseline-blob.ts.
 *
 * Holds:
 *   - A single open file descriptor for `.bin`
 *   - The parsed index in memory
 *
 * Exposes `readBaseline(name) -> Promise<Buffer>` that slices into the
 * blob via `fs.read(fd, ...)` with offset. This is the closest Node
 * primitive to mmap that doesn't require a native binding.
 *
 * If construction fails (missing blob / stale / parse error), use the
 * fallback path in the caller — this module never throws on missing
 * blob, it returns `null` from `tryLoad`.
 */
import * as fs from 'node:fs/promises';
import { open, FileHandle } from 'node:fs/promises';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  isBlobFresh,
  readBlobMeta,
  BlobIndex,
} from './build-baseline-blob.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const OUT_DIR = path.join(__dirname, '..', '.baseline-blob-cache');
const OUT_BIN = path.join(OUT_DIR, 'baselines.bin');
const OUT_IDX = path.join(OUT_DIR, 'baselines.idx.json');
const OUT_META = path.join(OUT_DIR, 'baselines.meta.json');

export class BaselineBlobReader {
  private fh: FileHandle;
  private index: Record<string, { offset: number; length: number }>;

  private constructor(fh: FileHandle, index: BlobIndex) {
    this.fh = fh;
    this.index = index.entries;
  }

  static async tryLoad(baselinesDir: string): Promise<BaselineBlobReader | null> {
    if (!await isBlobFresh(baselinesDir, OUT_META)) return null;
    try {
      const [idxRaw, fh] = await Promise.all([
        fs.readFile(OUT_IDX, 'utf-8'),
        open(OUT_BIN, 'r'),
      ]);
      const index = JSON.parse(idxRaw) as BlobIndex;
      return new BaselineBlobReader(fh, index);
    } catch {
      return null;
    }
  }

  has(name: string): boolean {
    return name in this.index;
  }

  async readBaseline(name: string): Promise<Buffer | null> {
    const entry = this.index[name];
    if (!entry) return null;
    const buf = Buffer.allocUnsafe(entry.length);
    const { bytesRead } = await this.fh.read(buf, 0, entry.length, entry.offset);
    if (bytesRead !== entry.length) return null;
    return buf;
  }

  async close(): Promise<void> {
    await this.fh.close();
  }

  async stats(): Promise<{ entries: number; meta: ReturnType<typeof readBlobMeta> extends Promise<infer T> ? T : never }> {
    const meta = await readBlobMeta(OUT_META);
    return { entries: Object.keys(this.index).length, meta: meta as Awaited<ReturnType<typeof readBlobMeta>> };
  }
}
