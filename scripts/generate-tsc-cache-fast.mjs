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
 *
 * Cache keys are relative file paths (e.g., "compiler/foo.ts").
 */

import { fork } from 'node:child_process';
import { readFileSync, writeFileSync, statSync, readdirSync } from 'node:fs';
import { join, extname, resolve, relative } from 'node:path';
import { cpus } from 'node:os';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);

// ---------------------------------------------------------------------------
// Shared constants
// ---------------------------------------------------------------------------
const DIRECTIVE_RE = /^\s*\/\/\s*@(\w+)\s*:\s*([^\r\n]*)/;

const HARNESS_ONLY_DIRECTIVES = new Set([
  'filename', 'allownontsextensions', 'usecasesensitivefilenames',
  'baselinefile', 'noerrortruncation', 'suppressoutputpathcheck',
  'noimplicitreferences', 'currentdirectory', 'symlink', 'link',
  'notypesandsymbols', 'fullemitpaths', 'nocheck',
  'reportdiagnostics', 'capturesuggestions', 'typescriptversion', 'skip',
]);

const LIST_OPTIONS = new Set([
  'lib', 'types', 'typeroots', 'rootdirs', 'modulesuffixes', 'customconditions',
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
      const value = m[2].trim();
      const keyLower = key.toLowerCase();
      if (keyLower === 'filename') {
        if (currentFilename !== null) {
          filenames.push({ name: currentFilename, content: currentLines.join('\n') });
        }
        currentFilename = value;
        currentLines = [];
      } else {
        options[keyLower] = value;
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
  if (options.nocheck === 'true') return true;
  return false;
}

function stripDirectiveComments(content) {
  const directiveRe = /^\s*\/\/\s*@\w+\s*:/;
  return content.split('\n')
    .filter(line => !directiveRe.test(line))
    .join('\n');
}

// ---------------------------------------------------------------------------
// Options conversion (mirrors Rust convert_options_to_tsconfig)
// ---------------------------------------------------------------------------

// Build canonical option name mapping from TypeScript's own option declarations.
// The test directive parser lowercases all keys, but ts.convertCompilerOptionsFromJson
// requires proper casing (e.g., "noImplicitAny" not "noimplicitany").
let CANONICAL_OPTION_NAMES = null;

function buildCanonicalOptionNames(ts) {
  if (CANONICAL_OPTION_NAMES) return;
  CANONICAL_OPTION_NAMES = new Map();
  if (ts.optionDeclarations) {
    for (const opt of ts.optionDeclarations) {
      const lower = opt.name.toLowerCase();
      if (lower !== opt.name) {
        CANONICAL_OPTION_NAMES.set(lower, opt.name);
      }
    }
  }
}

function canonicalOptionName(key) {
  if (CANONICAL_OPTION_NAMES && CANONICAL_OPTION_NAMES.has(key)) {
    return CANONICAL_OPTION_NAMES.get(key);
  }
  return key;
}

function convertOptionsToJson(options) {
  const result = {};
  for (const [key, value] of Object.entries(options)) {
    if (HARNESS_ONLY_DIRECTIVES.has(key)) {
      continue;
    }
    const canonKey = canonicalOptionName(key);
    if (value === 'true') {
      result[canonKey] = true;
    } else if (value === 'false') {
      result[canonKey] = false;
    } else if (LIST_OPTIONS.has(key)) {
      result[canonKey] = value.split(',').map(s => s.trim());
    } else {
      // For non-list options, take only the first comma-separated value
      const effectiveValue = value.split(',')[0].trim();
      if (/^\d+$/.test(effectiveValue)) {
        result[canonKey] = parseInt(effectiveValue, 10);
      } else {
        result[canonKey] = effectiveValue;
      }
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

  // Resolve testDir to an absolute path for consistent relative path computation
  const testDirAbs = resolve(args.testDir);

  console.log(`🔍 Discovering test files in: ${args.testDir}`);
  const testFiles = discoverTests(args.testDir, args.max);
  console.log(`✓ Found ${testFiles.length} test files`);

  const numWorkers = Math.min(args.workers, testFiles.length, cpus().length);
  console.log(`\n🔨 Processing ${testFiles.length} tests with ${numWorkers} workers...`);
  const start = performance.now();

  // Pre-read all files, parse directives, compute cache keys (relative paths)
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
      // Cache key is relative file path from test directory
      const key = relative(testDirAbs, resolve(filePath));

      fileQueue.push({
        path: filePath,
        content,
        options: directives.options,
        filenames: directives.filenames,
        key,
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
              cache[r.key] = {
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

  // Build canonical option name mapping from TypeScript's option declarations
  buildCanonicalOptionNames(ts);

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
            key: file.key,
            mtimeMs: file.mtimeMs,
            size: file.size,
            errorCodes,
          });
        } catch (err) {
          results.push({ key: file.key, error: err.message });
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
    const origExt = file.path ? extname(file.path) : '.ts';
    const ext = ['.ts', '.tsx', '.js', '.jsx'].includes(origExt) ? origExt : '.ts';
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
      if (ext !== '.ts' && ext !== '.tsx' && ext !== '.js' && ext !== '.jsx') continue;
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
