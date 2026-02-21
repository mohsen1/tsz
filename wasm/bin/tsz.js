#!/usr/bin/env node
// tsz — type-checker CLI using the @mohsen-azimi/tsz-dev WASM module.
// Usage: tsz [options] [file1.ts file2.ts ...]
//   --strict          Enable strict mode
//   --noEmit          Type-check only (implied, tsz never emits)
//   --project <path>  Read tsconfig.json from <path> (default: tsconfig.json)
//   --help            Show this message

'use strict';

const path = require('path');
const fs = require('fs');

// Resolve the Node.js WASM build relative to this script's location.
// pkg/
//   bin/tsz.js        ← this file
//   node/tsz_wasm.js  ← CJS WASM module
const pkgDir = path.resolve(__dirname, '..');
const { TsProgram } = require(path.join(pkgDir, 'node', 'tsz_wasm.js'));

// ─── CLI argument parsing ─────────────────────────────────────────────────────
const args = process.argv.slice(2);
const files = [];
const options = {};
let i = 0;

while (i < args.length) {
  const a = args[i];
  if (a === '--help' || a === '-h') {
    console.log([
      'Usage: tsz [options] [file...]',
      '',
      'Options:',
      '  --strict              Enable strict mode',
      '  --project <path>      Path to tsconfig.json directory (default: .)',
      '  --help                Show this help',
    ].join('\n'));
    process.exit(0);
  } else if (a === '--strict') {
    options.strict = true;
  } else if (a === '--project' || a === '-p') {
    options.project = args[++i];
  } else if (!a.startsWith('-')) {
    files.push(path.resolve(a));
  }
  i++;
}

// ─── Collect input files ──────────────────────────────────────────────────────
function readTsConfig(dir) {
  const tsconfigPath = path.join(dir, 'tsconfig.json');
  if (!fs.existsSync(tsconfigPath)) return null;
  try {
    const raw = fs.readFileSync(tsconfigPath, 'utf8');
    // Strip single-line comments (tsconfig allows them)
    const stripped = raw.replace(/\/\/[^\n]*/g, '');
    return JSON.parse(stripped);
  } catch {
    return null;
  }
}

function collectFiles(rootDir, tsconfig) {
  const include = (tsconfig && tsconfig.include) || ['**/*.ts', '**/*.tsx'];
  const exclude = new Set((tsconfig && tsconfig.exclude) || ['node_modules', 'dist', 'build']);
  const collected = [];

  function walk(dir) {
    let entries;
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      if (exclude.has(e.name)) continue;
      const full = path.join(dir, e.name);
      if (e.isDirectory()) {
        walk(full);
      } else if (e.isFile() && /\.(ts|tsx)$/.test(e.name) && !e.name.endsWith('.d.ts')) {
        collected.push(full);
      }
    }
  }

  walk(rootDir);
  return collected;
}

let inputFiles = files;
if (inputFiles.length === 0) {
  const projectDir = path.resolve(options.project || '.');
  const tsconfig = readTsConfig(projectDir);
  inputFiles = collectFiles(projectDir, tsconfig);
}

if (inputFiles.length === 0) {
  console.error('tsz: no input files');
  process.exit(1);
}

// ─── Run type checker ─────────────────────────────────────────────────────────
const program = new TsProgram();
program.setCompilerOptions(JSON.stringify(options));

// ─── Load TypeScript lib files ────────────────────────────────────────────────
// Lib .d.ts files (lib.es5.d.ts, lib.dom.d.ts, etc.) provide global type
// definitions (Array, String, Promise, console, document, etc.).
// They are bundled in the package under lib-assets/ with a manifest.
const libDir = path.join(pkgDir, 'lib-assets');
const manifestPath = path.join(libDir, 'lib_manifest.json');

if (fs.existsSync(manifestPath)) {
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const libs = manifest.libs || {};

  // Resolve the default root lib based on target (matches tsc's getDefaultLibFileName).
  // Default target is ES5 → root lib is "es5.full" (equivalent to tsc's "lib.d.ts").
  const targetLibMap = {
    es5: 'es5.full', es2015: 'es6', es2016: 'es2016.full',
    es2017: 'es2017.full', es2018: 'es2018.full', es2019: 'es2019.full',
    es2020: 'es2020.full', es2021: 'es2021.full', es2022: 'es2022.full',
    es2023: 'esnext.full', es2024: 'esnext.full', esnext: 'esnext.full',
  };
  const rootLib = targetLibMap[(options.target || 'es5').toLowerCase()] || 'es5.full';

  // BFS to resolve all transitive lib references
  const visited = new Set();
  const queue = [rootLib];
  while (queue.length > 0) {
    const name = queue.shift();
    if (!name || visited.has(name)) continue;
    visited.add(name);
    const entry = libs[name];
    if (!entry) continue;
    const filePath = path.join(libDir, entry.fileName);
    try {
      const content = fs.readFileSync(filePath, 'utf8');
      // Use canonical name (lib.es5.d.ts) so tsc-compatible lookups work
      program.addLibFile(entry.canonicalFileName || entry.fileName, content);
    } catch { /* skip missing files */ }
    // Follow references
    if (entry.references) {
      for (const ref of entry.references) queue.push(ref);
    }
  }
}

for (const file of inputFiles) {
  try {
    const text = fs.readFileSync(file, 'utf8');
    program.addSourceFile(file, text);
  } catch (err) {
    console.error(`tsz: cannot read ${file}: ${err.message}`);
  }
}

let diagnostics;
try {
  diagnostics = JSON.parse(program.getSemanticDiagnosticsJson(undefined));
} catch (err) {
  console.error(`tsz: internal error: ${err.message}`);
  process.exit(2);
}

// ─── Format and print diagnostics ─────────────────────────────────────────────
// Match tsc output format: path(line,col): error TS####: message
let errorCount = 0;
let warningCount = 0;

for (const d of diagnostics) {
  const category = String(d.category || 'error').toLowerCase();
  if (category === 'error') errorCount++;
  else if (category === 'warning') warningCount++;

  const file = d.file || '<unknown>';
  const relFile = path.relative(process.cwd(), file);
  const line = (d.line != null ? d.line + 1 : '?');
  const col  = (d.character != null ? d.character + 1 : '?');
  const code = d.code ? `TS${d.code}` : '';

  console.error(`${relFile}(${line},${col}): ${category} ${code}: ${d.messageText || d.message || ''}`);
}

if (diagnostics.length === 0) {
  console.log('tsz: no errors found');
  process.exit(0);
} else {
  if (errorCount > 0) {
    console.error(`\nFound ${errorCount} error${errorCount === 1 ? '' : 's'}${warningCount > 0 ? ` and ${warningCount} warning${warningCount === 1 ? '' : 's'}` : ''}.`);
    process.exit(1);
  } else {
    console.error(`\nFound ${warningCount} warning${warningCount === 1 ? '' : 's'}.`);
    process.exit(0);
  }
}
