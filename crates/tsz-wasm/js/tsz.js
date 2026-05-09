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
const DEFAULT_EXCLUDES = ['node_modules', 'bower_components', 'jspm_packages'];
const TS_SOURCE_EXTENSIONS = ['.ts', '.tsx', '.mts', '.cts'];
const TS_DECLARATION_EXTENSIONS = ['.d.ts', '.d.mts', '.d.cts'];
const JS_FAMILY_EXTENSIONS = ['.js', '.jsx', '.mjs', '.cjs'];

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
function readTsConfig(projectPath) {
  const resolvedPath = path.resolve(projectPath || '.');
  let tsconfigPath = resolvedPath;
  try {
    if (fs.existsSync(resolvedPath) && fs.statSync(resolvedPath).isDirectory()) {
      tsconfigPath = path.join(resolvedPath, 'tsconfig.json');
    }
  } catch {
    return null;
  }

  if (!fs.existsSync(tsconfigPath)) return null;
  try {
    const raw = fs.readFileSync(tsconfigPath, 'utf8');
    const stripped = stripJsonc(raw);
    const normalized = removeTrailingCommas(stripped);
    return JSON.parse(normalized);
  } catch {
    return null;
  }
}

function stripJsonc(input) {
  let out = '';
  let inString = false;
  let inLineComment = false;
  let inBlockComment = false;
  let escaped = false;

  for (let i = 0; i < input.length; i++) {
    const char = input[i];

    if (inLineComment) {
      if (char === '\n') {
        inLineComment = false;
        out += char;
      }
      continue;
    }

    if (inBlockComment) {
      if (char === '*' && input[i + 1] === '/') {
        inBlockComment = false;
        i += 1;
        out += ' ';
      } else if (char === '\n') {
        out += char;
      }
      continue;
    }

    if (inString) {
      out += char;
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === '"') {
        inString = false;
      }
      continue;
    }

    if (char === '"') {
      inString = true;
      out += char;
      continue;
    }

    if (char === '/' && input[i + 1] === '/') {
      inLineComment = true;
      i += 1;
      continue;
    }
    if (char === '/' && input[i + 1] === '*') {
      inBlockComment = true;
      i += 1;
      continue;
    }

    out += char;
  }

  return out;
}

function removeTrailingCommas(input) {
  let out = '';
  let inString = false;
  let escaped = false;

  for (let i = 0; i < input.length; i++) {
    const char = input[i];

    if (inString) {
      out += char;
      if (escaped) {
        escaped = false;
      } else if (char === '\\') {
        escaped = true;
      } else if (char === '"') {
        inString = false;
      }
      continue;
    }

    if (char === '"') {
      inString = true;
      out += char;
      continue;
    }

    if (char === ',') {
      let j = i + 1;
      while (j < input.length && /\s/.test(input[j])) {
        j++;
      }
      if (input[j] === '}' || input[j] === ']') {
        continue;
      }
    }

    out += char;
  }

  return out;
}

function normalizePatterns(patterns) {
  return patterns
    .map((pattern) => {
      const trimmed = String(pattern || '').trim();
      if (!trimmed) return '';
      const normalized = trimmed.replace(/\\/g, '/');
      return normalized.startsWith('./') ? normalized.slice(2) : normalized;
    })
    .filter(Boolean);
}

function hasGlobMeta(pattern) {
  return pattern.includes('*') || pattern.includes('?') || pattern.includes('[') || pattern.includes(']');
}

function includesSupportedExtension(pattern) {
  return TS_SOURCE_EXTENSIONS.concat(JS_FAMILY_EXTENSIONS).some((ext) => pattern.endsWith(ext));
}

function isTerminalWildcardPattern(pattern) {
  const trimmed = pattern.replace(/\/+$/, '');
  return trimmed === '*' || trimmed.endsWith('/*');
}

function expandIncludePatterns(patterns) {
  const expanded = [];
  for (const pattern of patterns) {
    if (includesSupportedExtension(pattern)) {
      expanded.push(pattern);
      continue;
    }
    if (isTerminalWildcardPattern(pattern)) {
      const base = pattern.replace(/\/+$/, '');
      expanded.push(base);
      expanded.push(`${base}/**/*`);
      continue;
    }
    const base = pattern.replace(/\/+$/, '');
    expanded.push(`${base}/**/*`);
  }
  return expanded;
}

function expandExcludePatterns(patterns) {
  const expanded = [];
  for (const pattern of patterns) {
    expanded.push(pattern);
    if (!hasGlobMeta(pattern) && !pattern.endsWith('/**')) {
      const base = pattern.replace(/\/+$/, '');
      expanded.push(`${base}/**`);
      if (!base.includes('/')) {
        expanded.push(`**/${base}`);
        expanded.push(`**/${base}/**`);
      }
    }
  }
  return expanded;
}

function buildIncludePatterns(options) {
  if (options.include) {
    if (options.include.length === 0) return [];
    return expandIncludePatterns(normalizePatterns(options.include));
  }
  if (options.files.length === 0 && !options.filesExplicitlySet) {
    return ['*.ts', '*.tsx', '*.mts', '*.cts', '**/*.ts', '**/*.tsx', '**/*.mts', '**/*.cts'];
  }
  return [];
}

function buildExcludePatterns(options) {
  const raw = options.exclude && options.exclude.length > 0
    ? normalizePatterns(options.exclude)
    : DEFAULT_EXCLUDES.map((name) => name);
  return expandExcludePatterns(raw);
}

function normalizeForMatch(p) {
  return p.replace(/\\/g, '/');
}

function toRegex(pattern) {
  const input = normalizeForMatch(pattern);
  let rx = '';
  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    const next = input[i + 1];

    if (ch === '*' && next === '*') {
      rx += '.*';
      i += 1;
      continue;
    }
    if (ch === '*') {
      rx += '[^/]*';
      continue;
    }
    if (ch === '?') {
      rx += '.';
      continue;
    }
    if (ch === '[') {
      let end = input.indexOf(']', i + 1);
      if (end === -1) {
        rx += '\\[';
        continue;
      }
      const body = input.slice(i + 1, end);
      rx += `[${body}]`;
      i = end;
      continue;
    }
    if ('^$\\.+()|[]{}'.includes(ch)) {
      rx += `\\${ch}`;
      continue;
    }
    if (ch === '/') {
      rx += '/';
      continue;
    }
    rx += ch;
  }
  return new RegExp(`^${rx}$`);
}

function buildMatcher(patterns) {
  return patterns.map((pattern) => ({
    pattern,
    regex: toRegex(pattern),
  }));
}

function matchAny(matchers, subject) {
  return matchers.some((entry) => entry.regex.test(subject));
}

function hasSupportedTsDiscoveryExtension(filePath) {
  return isTsSourceFile(filePath) || isTsDeclarationFile(filePath);
}

function isTsDeclarationFile(filePath) {
  const name = path.basename(filePath).toLowerCase();
  if (TS_DECLARATION_EXTENSIONS.some((ext) => name.endsWith(ext))) {
    return true;
  }
  return name.endsWith('.ts') && name.includes('.d.');
}

function isTsSourceFile(filePath) {
  if (isTsDeclarationFile(filePath)) return false;
  const name = path.basename(filePath).toLowerCase();
  return TS_SOURCE_EXTENSIONS.some((ext) => name.endsWith(ext));
}

function isTsFile(filePath) {
  return isTsDeclarationFile(filePath) || isTsSourceFile(filePath);
}

function isJsFile(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return JS_FAMILY_EXTENSIONS.includes(ext);
}

function isJsonFile(filePath) {
  return path.extname(filePath).toLowerCase() === '.json';
}

function stripTsDeclarationExtension(filePath) {
  const base = path.basename(filePath);
  for (const ext of TS_DECLARATION_EXTENSIONS) {
    if (base.toLowerCase().endsWith(ext)) {
      return path.join(path.dirname(filePath), base.slice(0, base.length - ext.length));
    }
  }
  return null;
}

function stripTsSourceExtension(filePath) {
  if (isTsDeclarationFile(filePath)) return null;
  const base = path.basename(filePath);
  for (const ext of TS_SOURCE_EXTENSIONS) {
    if (base.toLowerCase().endsWith(ext)) {
      return path.join(path.dirname(filePath), base.slice(0, base.length - ext.length));
    }
  }
  return null;
}

function excludeShadowedDeclarationFiles(filePaths) {
  const sourceStems = new Set();
  for (const filePath of filePaths) {
    const stem = stripTsSourceExtension(filePath);
    if (stem) sourceStems.add(stem);
  }

  const out = new Set();
  for (const filePath of filePaths) {
    const declarationStem = stripTsDeclarationExtension(filePath);
    if (declarationStem && sourceStems.has(declarationStem)) continue;
    out.add(filePath);
  }
  return out;
}

function includesPathMatch(targetPath, baseDir, walkRoot, matchers) {
  if (!matchers || matchers.length === 0) return false;
  const absolute = normalizeForMatch(targetPath);
  if (matchAny(matchers, absolute)) return true;
  const fromBase = normalizeForMatch(path.relative(baseDir, targetPath));
  if (fromBase && fromBase !== '.' && matchAny(matchers, fromBase)) return true;
  const fromRoot = normalizeForMatch(path.relative(walkRoot, targetPath));
  if (fromRoot && fromRoot !== '.' && matchAny(matchers, fromRoot)) return true;
  return false;
}

function isAbsolutePattern(pattern) {
  return path.isAbsolute(pattern);
}

function fixedPatternPrefix(pattern) {
  const normalized = path.normalize(pattern).replace(/\\/g, '/');
  const root = path.parse(normalized).root;
  const rest = normalized.slice(root.length);
  const parts = rest.split('/').filter(Boolean);
  const prefixParts = [];
  for (const part of parts) {
    if (part.includes('*') || part.includes('?') || part.includes('[') || part.includes(']')) {
      break;
    }
    prefixParts.push(part);
  }

  if (prefixParts.length === 0) {
    return root || path.resolve('.');
  }

  return path.join(root, ...prefixParts);
}

function includeWalkRoots(baseDir, includePatterns) {
  const roots = new Set();
  for (const pattern of includePatterns) {
    if (isAbsolutePattern(pattern)) {
      roots.add(fixedPatternPrefix(pattern));
    } else {
      roots.add(baseDir);
    }
  }
  return Array.from(roots);
}

function collectFiles(rootDir, tsconfig) {
  const filesFromConfig = (tsconfig && Array.isArray(tsconfig.files)) ? tsconfig.files : [];
  const includeFromConfig = (tsconfig && Array.isArray(tsconfig.include)) ? tsconfig.include : null;
  const excludeFromConfig = (tsconfig && Array.isArray(tsconfig.exclude)) ? tsconfig.exclude : null;
  const filesExplicitlySet = tsconfig != null && Object.prototype.hasOwnProperty.call(tsconfig, 'files');

  const options = {
    baseDir: rootDir,
    files: filesFromConfig,
    filesExplicitlySet,
    include: includeFromConfig,
    exclude: excludeFromConfig,
    allowJs: false,
    resolveJsonModule: false,
  };

  let discovered = new Set();
  for (const file of options.files) {
    const filePath = path.resolve(rootDir, file);
    if (!fs.existsSync(filePath)) continue;
    const stats = fs.lstatSync(filePath);
    if (!stats.isFile()) continue;

    if (isTsFile(filePath) || isJsFile(filePath) || (options.resolveJsonModule && isJsonFile(filePath))) {
      discovered.add(filePath);
    }
  }

  const includePatterns = buildIncludePatterns(options);
  if (includePatterns.length > 0) {
    const includeMatch = buildMatcher(includePatterns);
    const excludePatterns = buildExcludePatterns(options);
    const excludeMatch = excludePatterns.length === 0 ? [] : buildMatcher(excludePatterns);

    for (const walkRoot of includeWalkRoots(rootDir, includePatterns)) {
      const walker = [walkRoot];
      while (walker.length > 0) {
        const current = walker.pop();
        let entries;
        try {
          entries = fs.readdirSync(current, { withFileTypes: true });
        } catch {
          continue;
        }

        for (const entry of entries) {
          const next = path.join(current, entry.name);
          if (includesPathMatch(next, options.baseDir, walkRoot, excludeMatch)) {
            continue;
          }

          if (entry.isDirectory()) {
            walker.push(next);
            continue;
          }

          if (!entry.isFile()) continue;
          if (!(hasSupportedTsDiscoveryExtension(next) || (options.allowJs && isJsFile(next)))) continue;
          if (!includesPathMatch(next, options.baseDir, walkRoot, includeMatch)) continue;
          if (excludeMatch.length > 0 && includesPathMatch(next, options.baseDir, walkRoot, excludeMatch)) continue;
          discovered.add(next);
        }
      }
    }
  }

  discovered = excludeShadowedDeclarationFiles(discovered);
  const output = Array.from(discovered);
  output.sort((a, b) => a.localeCompare(b));
  return output;
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

// Map from absolute path → source text, used for offset→line/col conversion.
const sourceTexts = new Map();

for (const file of inputFiles) {
  try {
    const text = fs.readFileSync(file, 'utf8');
    program.addSourceFile(file, text);
    sourceTexts.set(file, text);
  } catch (err) {
    console.error(`tsz: cannot read ${file}: ${err.message}`);
  }
}

/**
 * Convert a 0-based UTF-16 character offset to { line, character } (0-based).
 * This matches the position model used by tsc.
 */
function offsetToLineChar(text, offset) {
  const clamped = Math.max(0, Math.min(offset, text.length));
  let line = 0;
  let lineStart = 0;
  for (let i = 0; i < clamped; i++) {
    if (text[i] === '\n') {
      line++;
      lineStart = i + 1;
    }
  }
  return { line, character: clamped - lineStart };
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

// tsc diagnostic category codes: 0=warning, 1=error, 2=suggestion, 3=message
const CATEGORY_NAMES = { 0: 'warning', 1: 'error', 2: 'suggestion', 3: 'message' };

for (const d of diagnostics) {
  const category = CATEGORY_NAMES[d.category] || 'error';
  if (category === 'error') errorCount++;
  else if (category === 'warning') warningCount++;

  const file = d.file || '<unknown>';
  const relFile = path.relative(process.cwd(), file);

  // Compute 1-based line/character from byte offset if available
  let line = '?', col = '?';
  if (typeof d.start === 'number') {
    const src = sourceTexts.get(file);
    if (src != null) {
      const pos = offsetToLineChar(src, d.start);
      line = pos.line + 1;
      col  = pos.character + 1;
    }
  }

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
