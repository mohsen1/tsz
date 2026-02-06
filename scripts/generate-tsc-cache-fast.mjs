#!/usr/bin/env node
/**
 * Fast TSC Cache Generator
 *
 * Uses the TypeScript compiler API directly instead of spawning `tsc` per file.
 * This is ~3-5x faster than the process-per-file approach because:
 *   1. TypeScript is loaded once per worker (not once per file)
 *   2. Lib files (lib.d.ts etc.) are parsed once and cached per worker
 *   3. No process spawning overhead per test file
 *   4. No temp directory creation per file
 *
 * Uses child_process.fork for full process-level parallelism.
 */

import { fork } from 'node:child_process';
import { readFileSync, writeFileSync, statSync, readdirSync } from 'node:fs';
import { join, extname, resolve } from 'node:path';
import { cpus } from 'node:os';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);

// ---------------------------------------------------------------------------
// Shared constants
// ---------------------------------------------------------------------------
const DIRECTIVE_RE = /^\s*\/\/\s*@(\w+)\s*:\s*(\S+)/;

const HARNESS_ONLY_DIRECTIVES = new Set([
  'filename', 'allowNonTsExtensions', 'useCaseSensitiveFileNames',
  'baselineFile', 'noErrorTruncation', 'suppressOutputPathCheck',
  'noImplicitReferences', 'currentDirectory', 'symlink', 'link',
  'noTypesAndSymbols', 'fullEmitPaths', 'noCheck', 'nocheck',
  'reportDiagnostics', 'captureSuggestions', 'typeScriptVersion', 'skip',
]);

const LIST_OPTIONS = new Set([
  'lib', 'types', 'typeRoots', 'rootDirs', 'moduleSuffixes', 'customConditions',
]);

// ---------------------------------------------------------------------------
// Directive parser (mirrors Rust test_parser.rs)
// ---------------------------------------------------------------------------
function parseDirectives(content) {
  const options = {};
  const filenames = [];
  let currentFilename = null;
  let currentLines = [];

  for (const line of content.split('\n')) {
    const m = line.match(DIRECTIVE_RE);
    if (m) {
      const key = m[1];
      const value = m[2];
      if (key.toLowerCase() === 'filename') {
        if (currentFilename !== null) {
          filenames.push({ name: currentFilename, content: currentLines.join('\n') });
        }
        currentFilename = value;
        currentLines = [];
      } else {
        options[key] = value;
      }
    } else {
      currentLines.push(line);
    }
  }

  if (currentFilename !== null) {
    filenames.push({ name: currentFilename, content: currentLines.join('\n') });
  }

  return { options, filenames };
}

function shouldSkip(options) {
  if ('skip' in options) return true;
  if (options.noCheck === 'true' || options.nocheck === 'true') return true;
  return false;
}

function stripDirectiveComments(content) {
  return content.split('\n')
    .filter(line => {
      const trimmed = line.trim();
      return !(trimmed.startsWith('//') && trimmed.includes('@') && trimmed.includes(':'));
    })
    .join('\n');
}

// ---------------------------------------------------------------------------
// Hash computation — blake3 compatible with Rust implementation
// ---------------------------------------------------------------------------
let blake3Module;

async function initHash() {
  try {
    blake3Module = await import('blake3');
    console.log('🔑 Using blake3 hashing (compatible with Rust runner)');
  } catch (err) {
    console.error('ERROR: blake3 npm package is required for cache compatibility with the Rust conformance runner.');
    console.error('Install it with: cd scripts && npm install blake3');
    console.error(`(import error: ${err.message})`);
    process.exit(1);
  }
}

function calculateTestHash(content, options) {
  const sorted = Object.entries(options).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  const hasher = blake3Module.createHash();
  hasher.update(Buffer.from(content, 'utf-8'));
  for (const [k, v] of sorted) {
    hasher.update(Buffer.from(k, 'utf-8'));
    hasher.update(Buffer.from('=', 'utf-8'));
    hasher.update(Buffer.from(v, 'utf-8'));
  }
  return hasher.digest('hex');
}

// ---------------------------------------------------------------------------
// Options conversion (mirrors Rust convert_options_to_tsconfig)
// ---------------------------------------------------------------------------
function convertOptionsToJson(options) {
  const result = {};
  for (const [key, value] of Object.entries(options)) {
    if (HARNESS_ONLY_DIRECTIVES.has(key.toLowerCase()) || HARNESS_ONLY_DIRECTIVES.has(key)) {
      continue;
    }
    if (value === 'true') {
      result[key] = true;
    } else if (value === 'false') {
      result[key] = false;
    } else if (LIST_OPTIONS.has(key.toLowerCase()) || LIST_OPTIONS.has(key)) {
      result[key] = value.split(',').map(s => s.trim());
    } else if (/^\d+$/.test(value)) {
      result[key] = parseInt(value, 10);
    } else {
      result[key] = value;
    }
  }
  return result;
}

// ---------------------------------------------------------------------------
// Check if we're running as a child worker process
// ---------------------------------------------------------------------------
const IS_WORKER = process.argv.includes('--worker-mode');

if (!IS_WORKER) {
  // ===========================================================================
  // MAIN PROCESS
  // ===========================================================================
  const args = parseArgs(process.argv.slice(2));

  console.log(`🔍 Discovering test files in: ${args.testDir}`);
  const testFiles = discoverTests(args.testDir, args.max);
  console.log(`✓ Found ${testFiles.length} test files`);

  const numWorkers = Math.min(args.workers, testFiles.length, cpus().length);
  console.log(`\n🔨 Processing ${testFiles.length} tests with ${numWorkers} workers...`);
  const start = performance.now();

  await initHash();

  // Pre-read all files, parse directives, calculate hashes
  console.log('📖 Reading and parsing files...');
  const fileQueue = [];
  let skippedCount = 0;

  for (const filePath of testFiles) {
    try {
      const content = readFileSync(filePath, 'utf-8');
      const directives = parseDirectives(content);

      if (shouldSkip(directives.options)) {
        skippedCount++;
        continue;
      }

      const stat = statSync(filePath);
      const hash = calculateTestHash(content, directives.options);

      fileQueue.push({
        path: filePath,
        content,
        options: directives.options,
        filenames: directives.filenames,
        hash,
        mtimeMs: Math.floor(stat.mtimeMs),
        size: stat.size,
      });
    } catch (err) {
      if (args.verbose) console.error(`✗ Error reading ${filePath}: ${err.message}`);
    }
  }

  console.log(`✓ Parsed ${fileQueue.length} files (${skippedCount} skipped)`);

  // Work-stealing queue: send CHUNK_SIZE files at a time to each worker
  const CHUNK_SIZE = 5;
  let nextFileIdx = 0;
  const cache = {};
  let processed = 0;
  let errors = 0;
  const totalFiles = fileQueue.length;

  function getNextChunk() {
    if (nextFileIdx >= fileQueue.length) return null;
    const chunk = fileQueue.slice(nextFileIdx, nextFileIdx + CHUNK_SIZE);
    nextFileIdx += chunk.length;
    return chunk;
  }

  // Spawn worker processes
  const workerPromises = [];
  for (let i = 0; i < numWorkers; i++) {
    workerPromises.push(new Promise((resolveWorker) => {
      const child = fork(__filename, ['--worker-mode', '--ts-path', args.tsPath], {
        serialization: 'advanced',
        execArgv: ['--max-old-space-size=512'],
      });

      function sendChunk() {
        const chunk = getNextChunk();
        if (chunk) {
          child.send({ type: 'chunk', files: chunk });
        } else {
          child.send({ type: 'done' });
        }
      }

      child.on('message', (msg) => {
        if (msg.type === 'ready') {
          sendChunk();
        } else if (msg.type === 'results') {
          for (const r of msg.results) {
            if (r.error) {
              errors++;
            } else {
              cache[r.hash] = {
                metadata: { mtime_ms: r.mtimeMs, size: r.size },
                error_codes: r.errorCodes,
              };
            }
            processed++;
          }
          if (processed % 100 < CHUNK_SIZE) {
            const elapsed = (performance.now() - start) / 1000;
            const rate = processed / elapsed;
            const eta = (totalFiles - processed) / rate;
            process.stderr.write(
              `\r[${processed}/${totalFiles}] ${rate.toFixed(0)} tests/sec, ETA ${formatTime(eta)} (${errors} errors)    `
            );
          }
          sendChunk();
        }
      });

      child.on('exit', () => resolveWorker());
      child.on('error', (err) => {
        console.error(`Worker ${i} error:`, err);
        resolveWorker();
      });
    }));
  }

  await Promise.all(workerPromises);

  const elapsed = (performance.now() - start) / 1000;
  console.log(
    `\r✓ Completed in ${formatTime(elapsed)} (${(totalFiles / elapsed).toFixed(0)} tests/sec)                              `
  );
  console.log(`  Processed: ${processed}`);
  console.log(`  Cached: ${Object.keys(cache).length}`);
  console.log(`  Skipped: ${skippedCount}`);
  console.log(`  Errors: ${errors}`);

  console.log(`\n💾 Writing cache to: ${args.output}`);
  writeFileSync(args.output, JSON.stringify(cache, null, 2));
  console.log(`✓ Cache written with ${Object.keys(cache).length} entries`);

} else {
  // ===========================================================================
  // WORKER PROCESS (child_process.fork)
  // ===========================================================================
  const tsPath = process.argv[process.argv.indexOf('--ts-path') + 1];

  // Load TypeScript once
  let ts;
  try {
    ts = (await import(tsPath)).default || (await import(tsPath));
  } catch {
    try {
      ts = (await import('typescript')).default || (await import('typescript'));
    } catch (e) {
      console.error(`Worker: Cannot import typescript: ${e.message}`);
      process.exit(1);
    }
  }

  // Lib source file cache — persists across all files this worker processes
  const libSourceFileCache = new Map();

  // Signal ready
  process.send({ type: 'ready' });

  process.on('message', (msg) => {
    if (msg.type === 'done') {
      process.exit(0);
    }

    if (msg.type === 'chunk') {
      const results = [];
      for (const file of msg.files) {
        try {
          const errorCodes = processFile(ts, file, libSourceFileCache);
          results.push({
            hash: file.hash,
            mtimeMs: file.mtimeMs,
            size: file.size,
            errorCodes,
          });
        } catch (err) {
          results.push({ hash: file.hash, error: err.message });
        }
      }
      process.send({ type: 'results', results });
    }
  });
}

// ---------------------------------------------------------------------------
// File processing (runs in worker)
// ---------------------------------------------------------------------------
function processFile(ts, file, libCache) {
  const { content, options, filenames } = file;

  const jsonOptions = convertOptionsToJson(options);
  const { options: compilerOptions } =
    ts.convertCompilerOptionsFromJson(jsonOptions, '/virtual');

  compilerOptions.noEmit = true;

  // Build virtual file system
  const virtualFiles = new Map();

  if (!filenames || filenames.length === 0) {
    const stripped = stripDirectiveComments(content);
    const ext = (file.path && file.path.endsWith('.tsx')) ? '.tsx' : '.ts';
    virtualFiles.set('/virtual/test' + ext, stripped);
  } else {
    for (const { name, content: fileContent } of filenames) {
      let sanitized = name.replace(/\.\./g, '_');
      sanitized = sanitized.replace(/\\/g, '/').replace(/^\//, '');
      virtualFiles.set('/virtual/' + sanitized, fileContent);
    }
  }

  const rootFiles = [...virtualFiles.keys()];

  const host = {
    getSourceFile(fileName, languageVersion) {
      if (virtualFiles.has(fileName)) {
        return ts.createSourceFile(fileName, virtualFiles.get(fileName), languageVersion, true);
      }
      if (libCache.has(fileName)) {
        return libCache.get(fileName);
      }
      try {
        const text = ts.sys.readFile(fileName);
        if (text !== undefined) {
          const sf = ts.createSourceFile(fileName, text, languageVersion, true);
          if (fileName.includes('lib.') && (fileName.endsWith('.d.ts') || fileName.endsWith('.d.mts'))) {
            libCache.set(fileName, sf);
          }
          return sf;
        }
      } catch {}
      return undefined;
    },
    getDefaultLibFileName: (opts) => ts.getDefaultLibFilePath(opts),
    writeFile: () => {},
    getCurrentDirectory: () => '/virtual',
    getCanonicalFileName: (f) => f,
    useCaseSensitiveFileNames: () => true,
    getNewLine: () => '\n',
    fileExists(fileName) {
      if (virtualFiles.has(fileName)) return true;
      return ts.sys.fileExists(fileName);
    },
    readFile(fileName) {
      if (virtualFiles.has(fileName)) return virtualFiles.get(fileName);
      return ts.sys.readFile(fileName);
    },
    directoryExists(dirName) {
      if (dirName === '/virtual' || dirName.startsWith('/virtual/')) return true;
      return ts.sys.directoryExists ? ts.sys.directoryExists(dirName) : false;
    },
    getDirectories(dirName) {
      if (dirName === '/virtual' || dirName.startsWith('/virtual/')) return [];
      return ts.sys.getDirectories ? ts.sys.getDirectories(dirName) : [];
    },
    realpath(path) {
      return ts.sys.realpath ? ts.sys.realpath(path) : path;
    },
  };

  const program = ts.createProgram(rootFiles, compilerOptions, host);

  const diagnostics = [
    ...program.getSyntacticDiagnostics(),
    ...program.getSemanticDiagnostics(),
    ...program.getGlobalDiagnostics(),
  ];

  return [...new Set(diagnostics.map(d => d.code))].sort((a, b) => a - b);
}

// ---------------------------------------------------------------------------
// Test discovery
// ---------------------------------------------------------------------------
function discoverTests(testDir, max) {
  const files = [];
  walkDir(testDir, files);
  files.sort();
  if (max > 0 && files.length > max) files.length = max;
  return files;
}

function walkDir(dir, files) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch { return; }
  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'fourslash') continue;
      walkDir(fullPath, files);
    } else if (entry.isFile()) {
      const ext = extname(entry.name);
      if (ext !== '.ts' && ext !== '.tsx') continue;
      if (entry.name.endsWith('.d.ts')) continue;
      files.push(fullPath);
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function formatTime(secs) {
  if (secs < 60) return `${secs.toFixed(1)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.floor(secs % 60);
  return `${m}m${s}s`;
}

function parseArgs(argv) {
  const args = {
    testDir: './TypeScript/tests/cases',
    output: './tsc-cache-full.json',
    workers: Math.min(Math.max(1, cpus().length - 1), 8),
    max: 0,
    verbose: false,
    tsPath: null,
  };

  for (let i = 0; i < argv.length; i++) {
    switch (argv[i]) {
      case '--test-dir': args.testDir = argv[++i]; break;
      case '--output': args.output = argv[++i]; break;
      case '--workers': args.workers = parseInt(argv[++i], 10); break;
      case '--max': args.max = parseInt(argv[++i], 10); break;
      case '--verbose': case '-v': args.verbose = true; break;
      case '--ts-path': args.tsPath = argv[++i]; break;
    }
  }

  if (!args.tsPath) {
    const candidates = [
      resolve('./scripts/emit/node_modules/typescript/lib/typescript.js'),
      resolve('./node_modules/typescript/lib/typescript.js'),
      'typescript',
    ];
    for (const c of candidates) {
      try {
        if (c === 'typescript') { args.tsPath = c; break; }
        statSync(c);
        args.tsPath = c;
        break;
      } catch {}
    }
    if (!args.tsPath) args.tsPath = 'typescript';
  }

  console.log(`📍 TypeScript path: ${args.tsPath}`);
  return args;
}
