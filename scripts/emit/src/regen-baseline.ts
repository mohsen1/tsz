#!/usr/bin/env node
/**
 * Regenerate emit baselines against TypeScript 7.0.2.
 *
 * The baselines checked into the TypeScript submodule are TS 6.0-era, while the
 * conformance oracle is TS 7.0.2. This produces TS7 replacements into
 * `scripts/emit/baselines-ts7/`, which `runner.ts` prefers over the submodule.
 * The submodule itself is read-only (`scripts/githooks/pre-commit` blocks it).
 *
 * DESIGN: splice, do not rebuild.
 *
 * The generator copies the existing baseline verbatim from byte 0 through the
 * end of the input-echo sections, and replaces only the emitted output
 * sections. Re-deriving the echo would mean reproducing the TS harness's own
 * formatting — BOM and UTF-16 inputs, unit ordering, trailing-newline
 * conventions, `@fullEmitPaths` markers — and every one of those is a way to
 * corrupt 11.5k currently-passing rows. Splicing makes them structurally
 * impossible to get wrong.
 *
 * VALIDATION RULE: a baseline whose content TS6 and TS7 agree on must
 * regenerate byte-identically. `--verify` asserts that for the JS sections,
 * which today agree for every slice-1 test. A generator that cannot reproduce
 * the agreement cases exactly tells you nothing about its disagreement cases.
 *
 * Usage:
 *   node scripts/emit/dist/regen-baseline.js --filter=<substr> [--dry-run] [--verify] [--repeat=2]
 */
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

import { parseBaseline } from './baseline-parser.js';
import { parseDirectiveLine } from './directives.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT_DIR = path.resolve(__dirname, '../../..');
const TS_DIR = path.join(ROOT_DIR, 'TypeScript');
const BASELINES_DIR = path.join(TS_DIR, 'tests/baselines/reference');
const OVERLAY_DIR = path.join(__dirname, '../baselines-ts7');
const TSC = path.join(ROOT_DIR, 'scripts/node_modules/typescript/bin/tsc');

/** Output sections are the emitted artifacts; everything else is input echo. */
const OUTPUT_EXT_RE = /\.(js|jsx|mjs|cjs|d\.ts|d\.mts|d\.cts)$/;

interface Section {
  name: string;
  /** Byte offset of the `//// [name]` marker line. */
  markerStart: number;
}

/** Locate every `//// [name]` section marker, in order. */
function findSections(content: string): Section[] {
  const out: Section[] = [];
  const re = /^\/\/\/\/ \[([^\]]+)\]( \/\/\/\/)?\s*$/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) {
    // The first marker carries a trailing ` ////` and names the test path, not a file.
    if (m[2]) continue;
    out.push({ name: m[1], markerStart: m.index });
  }
  return out;
}

/**
 * Split a corpus test into its units, honouring `@filename` directives.
 * Directive lines are dropped from the emitted unit content, matching the
 * harness.
 */
function splitUnits(source: string): { name: string; content: string }[] {
  const units: { name: string; content: string }[] = [];
  let current: { name: string; content: string[] } | null = null;
  for (const line of source.split('\n')) {
    const directive = parseDirectiveLine(line);
    if (directive && directive.key === 'filename') {
      if (current) units.push({ name: current.name, content: current.content.join('\n') });
      current = { name: directive.value, content: [] };
      continue;
    }
    if (directive) continue; // other directives are options, not content
    if (current) current.content.push(line);
    else current = { name: '', content: [line] };
  }
  if (current) units.push({ name: current.name, content: current.content.join('\n') });
  return units.filter(u => u.name !== '' || u.content.trim() !== '');
}

/** Collect `// @key: value` options, lowercased keys, first variant value only. */
function collectOptions(source: string): Map<string, string> {
  const opts = new Map<string, string>();
  for (const line of source.split('\n')) {
    const d = parseDirectiveLine(line);
    if (!d || d.key === 'filename') continue;
    opts.set(d.key, d.value);
  }
  return opts;
}

interface RegenResult {
  baselineName: string;
  outputs: Map<string, string>;
  diagnostics: string;
}

/**
 * Run tsc 7.0.2 over a test's units and return its emitted files.
 *
 * `newLine: "crlf"` is mandatory: the harness writes baselines with CRLF, so
 * dropping it makes every emitted line differ. The `--verify` byte-exact JS
 * assertion is what catches it if it is ever lost.
 */
function runTsc7(
  units: { name: string; content: string }[],
  opts: Map<string, string>,
  variantOverrides: Map<string, string>,
): RegenResult['outputs'] & { __diagnostics?: string } {
  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'tsz-regen-'));
  try {
    for (const unit of units) {
      const file = path.join(work, unit.name || 'input.ts');
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, unit.content);
    }
    const co: Record<string, unknown> = {
      newLine: 'crlf',
      outDir: 'out',
      declaration: true,
    };
    const passthrough = [
      'target', 'module', 'lib', 'strict', 'allowjs', 'checkjs', 'declaration',
      'esmoduleinterop', 'jsx', 'moduleresolution', 'noimplicitany', 'emitdeclarationonly',
      'downleveliteration', 'usedefineforclassfields', 'experimentaldecorators', 'alwaysstrict',
    ];
    const camel: Record<string, string> = {
      allowjs: 'allowJs', checkjs: 'checkJs', esmoduleinterop: 'esModuleInterop',
      moduleresolution: 'moduleResolution', noimplicitany: 'noImplicitAny',
      emitdeclarationonly: 'emitDeclarationOnly', downleveliteration: 'downlevelIteration',
      usedefineforclassfields: 'useDefineForClassFields',
      experimentaldecorators: 'experimentalDecorators', alwaysstrict: 'alwaysStrict',
    };
    for (const key of passthrough) {
      const raw = variantOverrides.get(key) ?? opts.get(key);
      if (raw === undefined) continue;
      const name = camel[key] ?? key;
      const first = raw.split(',')[0].trim();
      if (key === 'lib') co[name] = raw.split(',').map(s => s.trim()).filter(Boolean);
      else if (first === 'true' || first === 'false') co[name] = first === 'true';
      else co[name] = first;
    }
    fs.writeFileSync(
      path.join(work, 'tsconfig.json'),
      JSON.stringify({ compilerOptions: co, include: ['**/*'] }, null, 2),
    );
    let diagnostics = '';
    try {
      execFileSync(process.execPath, [TSC, '--project', work, '--pretty', 'false'], {
        encoding: 'utf-8', stdio: ['ignore', 'pipe', 'pipe'],
      });
    } catch (err) {
      // tsc exits nonzero when the program has errors; it still emits.
      diagnostics = String((err as { stdout?: string }).stdout ?? '');
    }
    const outputs = new Map<string, string>();
    const outDir = path.join(work, 'out');
    if (fs.existsSync(outDir)) {
      const walk = (dir: string) => {
        for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
          const p = path.join(dir, e.name);
          if (e.isDirectory()) walk(p);
          else outputs.set(path.relative(outDir, p), fs.readFileSync(p, 'utf-8'));
        }
      };
      walk(outDir);
    }
    (outputs as RegenResult['outputs'] & { __diagnostics?: string }).__diagnostics = diagnostics;
    return outputs as RegenResult['outputs'] & { __diagnostics?: string };
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

/** Variant options encoded in a baseline filename, e.g. `foo(target=es2015).js`. */
function variantFromName(baselineName: string): Map<string, string> {
  const out = new Map<string, string>();
  const m = /\(([^)]*)\)\.js$/.exec(baselineName);
  if (!m) return out;
  for (const part of m[1].split(',')) {
    const [k, v] = part.split('=');
    if (k && v) out.set(k.trim().toLowerCase(), v.trim());
  }
  return out;
}

function main(): void {
  const args = process.argv.slice(2);
  const filter = (args.find(a => a.startsWith('--filter=')) ?? '').slice('--filter='.length);
  const dryRun = args.includes('--dry-run');
  const verify = args.includes('--verify');
  if (!filter) {
    console.error('usage: regen-baseline --filter=<substring> [--dry-run] [--verify]');
    process.exit(2);
  }

  const names = fs.readdirSync(BASELINES_DIR)
    .filter(n => n.endsWith('.js') && n.toLowerCase().includes(filter.toLowerCase()))
    .sort();

  let changed = 0;
  let identical = 0;
  let refused = 0;

  for (const name of names) {
    const existing = fs.readFileSync(path.join(BASELINES_DIR, name), 'utf-8');
    const parsed = parseBaseline(existing);
    if (!parsed.testPath) { refused++; continue; }
    const testFile = path.join(TS_DIR, parsed.testPath);
    if (!fs.existsSync(testFile)) { refused++; continue; }

    const sections = findSections(existing);
    const firstOutput = sections.find(s => OUTPUT_EXT_RE.test(s.name)
      && !parsed.sourceFiles.some(f => f.name === s.name));
    if (!firstOutput) { refused++; continue; }

    const source = fs.readFileSync(testFile, 'utf-8').replace(/^﻿/, '');
    const units = splitUnits(source);
    // A test with no `@filename` directive is a single unit named after the
    // test file itself. Emitting it as `input.ts` would make every output name
    // fail to match the baseline's section names.
    if (units.length > 0 && units[0].name === '') {
      units[0].name = path.basename(parsed.testPath);
    }
    const opts = collectOptions(source);
    const outputs = runTsc7(units, opts, variantFromName(name));

    // Splice: keep everything before the first output section verbatim, then
    // substitute each output section's CONTENT in place while preserving the
    // original marker lines and inter-section separators byte-for-byte.
    //
    // Reconstructing those separators is not worth attempting: the harness
    // writes input echo with the source file's own line endings but output
    // sections with CRLF, and the blank-line runs between sections vary. Any
    // mismatch there would corrupt every regenerated baseline while looking
    // like a real diff. Preserving the original bytes makes it impossible.
    const outSections = sections.filter(s => s.markerStart >= firstOutput.markerStart);
    let result = existing.slice(0, firstOutput.markerStart);
    let missing = false;
    for (let i = 0; i < outSections.length; i++) {
      const s = outSections[i];
      const emitted = outputs.get(s.name);
      if (emitted === undefined) { missing = true; break; }

      const markerLineEnd = existing.indexOf('\n', s.markerStart) + 1;
      const regionEnd = i + 1 < outSections.length
        ? outSections[i + 1].markerStart
        : existing.length;
      const region = existing.slice(markerLineEnd, regionEnd);
      // Whatever terminates the original region — its own newline plus any
      // blank-line separator before the next marker — is reused verbatim.
      const trailing = /(?:\r?\n)*$/.exec(region)?.[0] ?? '';

      result += existing.slice(s.markerStart, markerLineEnd);
      result += emitted.replace(/(?:\r?\n)*$/, '') + trailing;
    }
    if (missing) {
      // tsc did not produce one of the sections the baseline declares. Losing a
      // section would change the pass-count denominator, so refuse rather than
      // write a partial baseline.
      console.log(`  refuse  ${name} (tsc did not emit every declared section)`);
      refused++;
      continue;
    }

    const regenerated = result;
    if (regenerated === existing) {
      identical++;
      if (verify) console.log(`  same    ${name}`);
      continue;
    }
    changed++;
    console.log(`  CHANGED ${name}`);
    if (verify) {
      // The JS sections must agree; only .d.ts is expected to move in slice 1.
      const before = parseBaseline(existing);
      const after = parseBaseline(regenerated);
      if (before.js !== after.js) {
        console.log(`    !! JS section moved — investigate before trusting the .d.ts diff`);
      } else {
        console.log(`    JS byte-identical; .d.ts differs (expected)`);
      }
    }
    if (!dryRun) {
      fs.mkdirSync(OVERLAY_DIR, { recursive: true });
      fs.writeFileSync(path.join(OVERLAY_DIR, name), regenerated);
    }
  }

  console.log(`\n${changed} changed, ${identical} identical, ${refused} refused`);
}

main();
