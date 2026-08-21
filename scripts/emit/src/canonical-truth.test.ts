import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { compareEmit } from './baseline-parser.js';
import {
  compareCanonicalProductSets,
  compareCompilerOutcomes,
  type EmitProduct,
} from './canonical-products.js';
import { CliTranspiler, type TranspileResult } from './cli-transpiler.js';
import {
  extractAuthoredVariantFromFilename,
  optionBoolean,
  optionString,
  resolveAuthoredOptions,
} from './authored-options.js';
import { parseTarget } from './ts-enums.js';

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tsz-emit-oracle-truth-'));
const invocationLog = path.join(tempDir, 'invocations.jsonl');

function fakeCompilerSource(role: 'oracle' | 'tsz'): string {
  return `#!/usr/bin/env node
import * as fs from 'node:fs';
import * as path from 'node:path';

const role = ${JSON.stringify(role)};
const args = process.argv.slice(2);
fs.appendFileSync(process.env.FAKE_COMPILER_LOG, JSON.stringify({ role, args }) + '\\n');
const scenario = process.env[role === 'oracle' ? 'FAKE_ORACLE_SCENARIO' : 'FAKE_TSZ_SCENARIO'] ?? 'exact';
const valueAfter = flag => {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
};
const rootDir = valueAfter('--rootDir');
const outDir = valueAfter('--outDir');
const declarationDir = valueAfter('--declarationDir') ?? outDir;
const inputs = args.filter(arg => path.isAbsolute(arg) && /\\.(?:ts|tsx|mts|cts)$/.test(arg));
const relativeStem = input => {
  const relative = rootDir ? path.relative(rootDir, input) : path.basename(input);
  return relative.replace(/\\.(?:ts|tsx|mts|cts)$/, '');
};
const writeFile = (output, content) => {
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, content);
};
const contentFor = stem => {
  if (scenario === 'ts6-bytes') return 'TS6_BYTES\\n';
  if (scenario === 'ts7-bytes') return 'TS7_BYTES\\n';
  if (scenario === 'content-divergence') return role === 'oracle' ? 'ORACLE_BYTES\\n' : 'TSZ_BYTES\\n';
  return path.basename(stem).toUpperCase() + '_JS\\n';
};
const writeProducts = () => {
  if (args.includes('--noEmit') || scenario === 'checked-no-emit' || scenario === 'missing') return;
  if (scenario === 'root-stage') {
    const root = inputs[0];
    if (!root || !fs.existsSync(path.join(path.dirname(root), 'a.ts'))) {
      process.stderr.write('staged non-root file missing\\n');
      process.exit(3);
    }
  }
  for (const input of inputs) {
    const stem = relativeStem(input);
    if (scenario === 'missing-sibling' && path.basename(stem) === 'b') continue;
    const jsOutput = path.join(outDir ?? path.dirname(input), stem + '.js');
    const dtsOutput = path.join(declarationDir ?? path.dirname(input), stem + '.d.ts');
    if (scenario === 'wrong-path') {
      writeFile(path.join(outDir ?? path.dirname(input), 'wrong', path.basename(stem) + '.js'), contentFor(stem));
    } else if (!args.includes('--emitDeclarationOnly') || scenario === 'declaration-only-leak') {
      writeFile(jsOutput, contentFor(stem));
    }
    if (args.includes('--declaration')) writeFile(dtsOutput, path.basename(stem).toUpperCase() + '_DTS\\n');
  }
  if (scenario === 'extra') writeFile(path.join(outDir, 'extra.js'), 'EXTRA_JS\\n');
};

if (scenario === 'timeout') {
  await new Promise(resolve => setTimeout(resolve, 1_000));
  process.exit(0);
}
if (scenario === 'signal-crash') process.kill(process.pid, 'SIGTERM');
writeProducts();
if (scenario === 'checked-no-emit') {
  process.stderr.write('case.ts(1,1): error TS2322: checked witness\\n');
  process.exit(1);
}
if (scenario === 'exit3') {
  process.stderr.write('compiler crashed without diagnostics\\n');
  process.exit(3);
}
process.exit(0);
`;
}

const oracleCli = path.join(tempDir, 'fake-oracle.mjs');
const tszCli = path.join(tempDir, 'fake-tsz.mjs');
fs.writeFileSync(oracleCli, fakeCompilerSource('oracle'), 'utf8');
fs.writeFileSync(tszCli, fakeCompilerSource('tsz'), 'utf8');
fs.chmodSync(oracleCli, 0o755);
fs.chmodSync(tszCli, 0o755);

const priorLog = process.env.FAKE_COMPILER_LOG;
const priorOracleScenario = process.env.FAKE_ORACLE_SCENARIO;
const priorTszScenario = process.env.FAKE_TSZ_SCENARIO;
process.env.FAKE_COMPILER_LOG = invocationLog;

interface Invocation { role: 'oracle' | 'tsz'; args: string[] }
function invocations(): Invocation[] {
  if (!fs.existsSync(invocationLog)) return [];
  return fs.readFileSync(invocationLog, 'utf8').trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
}

function valueAfter(args: string[], flag: string): string | undefined {
  const index = args.indexOf(flag);
  return index < 0 ? undefined : args[index + 1];
}

function freshPair(timeoutMs = 5_000): { oracle: CliTranspiler; tsz: CliTranspiler } {
  return {
    oracle: new CliTranspiler(timeoutMs, { binaryPath: oracleCli, label: 'fake-oracle' }),
    tsz: new CliTranspiler(timeoutMs, { binaryPath: tszCli, label: 'fake-tsz' }),
  };
}

function canonicalMatch(oracle: TranspileResult, tsz: TranspileResult): boolean {
  return compareCompilerOutcomes(oracle.outcome, tsz.outcome).match &&
    compareCanonicalProductSets(oracle.jsProducts, tsz.jsProducts).match &&
    compareCanonicalProductSets(oracle.dtsProducts, tsz.dtsProducts).match;
}

const baseOptions = {
  sourceFiles: [{ name: 'a.ts', content: 'export const a = 1;' }],
  outDir: 'out',
};

try {
  process.env.FAKE_ORACLE_SCENARIO = 'exact';
  process.env.FAKE_TSZ_SCENARIO = 'exact';
  let pair = freshPair();
  const multiOptions = {
    sourceFiles: [
      { name: '/pkg/src/a.ts', content: 'export const a = 1;' },
      { name: '/pkg/src/b.ts', content: 'export const b = 1;' },
    ],
    outDir: '/pkg/dist',
    rootDir: '/pkg/src',
  };
  const [exactOracle, exactTsz] = await Promise.all([
    pair.oracle.transpile('', 9, 1, multiOptions),
    pair.tsz.transpile('', 9, 1, multiOptions),
  ]);
  assert.equal(canonicalMatch(exactOracle, exactTsz), true);
  assert.deepEqual(exactOracle.jsProducts.map(product => product.path), ['pkg/dist/a.js', 'pkg/dist/b.js']);
  pair.oracle.terminate(); pair.tsz.terminate();

  process.env.FAKE_TSZ_SCENARIO = 'missing-sibling';
  pair = freshPair();
  const [missingOracle, missingTsz] = await Promise.all([
    pair.oracle.transpile('', 9, 1, multiOptions),
    pair.tsz.transpile('', 9, 1, multiOptions),
  ]);
  assert.equal(compareCanonicalProductSets(missingOracle.jsProducts, missingTsz.jsProducts).match, false);
  assert.equal(
    compareCanonicalProductSets(missingOracle.jsProducts, missingTsz.jsProducts)
      .mismatches.some(mismatch => mismatch.kind === 'missing'),
    true,
  );
  pair.oracle.terminate(); pair.tsz.terminate();

  for (const scenario of ['extra', 'wrong-path', 'content-divergence'] as const) {
    process.env.FAKE_TSZ_SCENARIO = scenario;
    pair = freshPair();
    const [oracle, tsz] = await Promise.all([
      pair.oracle.transpile('', 9, 1, baseOptions),
      pair.tsz.transpile('', 9, 1, baseOptions),
    ]);
    assert.equal(canonicalMatch(oracle, tsz), false, `${scenario} remains red`);
    pair.oracle.terminate(); pair.tsz.terminate();
  }

  fs.writeFileSync(invocationLog, '', 'utf8');
  process.env.FAKE_ORACLE_SCENARIO = 'checked-no-emit';
  process.env.FAKE_TSZ_SCENARIO = 'checked-no-emit';
  pair = freshPair();
  const checkedOptions = { ...baseOptions, noEmitOnError: true };
  const [checkedOracle, checkedTsz] = await Promise.all([
    pair.oracle.transpile('', 9, 1, checkedOptions),
    pair.tsz.transpile('', 9, 1, checkedOptions),
  ]);
  assert.equal(canonicalMatch(checkedOracle, checkedTsz), false, 'matching diagnostics and absence remain nonpassing');
  assert.match(compareCompilerOutcomes(checkedOracle.outcome, checkedTsz.outcome).error!, /NONZERO_OUTCOME/);
  assert.deepEqual(checkedOracle.jsProducts, []);
  assert.deepEqual(checkedTsz.jsProducts, []);
  const checkedInvocations = invocations();
  assert.equal(checkedInvocations.length, 2, 'each compiler is invoked exactly once');
  for (const invocation of checkedInvocations) {
    assert.equal(invocation.args.includes('--noCheck'), false, 'noCheck is never synthesized');
    assert.equal(invocation.args.includes('--noLib'), false, 'noLib is never synthesized');
  }
  pair.oracle.terminate(); pair.tsz.terminate();

  fs.writeFileSync(invocationLog, '', 'utf8');
  process.env.FAKE_ORACLE_SCENARIO = 'root-stage';
  process.env.FAKE_TSZ_SCENARIO = 'root-stage';
  pair = freshPair();
  const stagedRootOptions = {
    sourceFiles: [
      { name: '/src/a.ts', content: 'export const a = 1;\n' },
      { name: '/src/b.ts', content: 'export const b = 1;\n' },
    ],
    rootFileNames: ['/src/b.ts'],
    outDir: '/out',
  };
  const [rootOracle, rootTsz] = await Promise.all([
    pair.oracle.transpile('', 9, 1, stagedRootOptions),
    pair.tsz.transpile('', 9, 1, stagedRootOptions),
  ]);
  assert.equal(canonicalMatch(rootOracle, rootTsz), true);
  assert.deepEqual(rootOracle.jsProducts.map(product => path.posix.basename(product.path)), ['b.js']);
  for (const invocation of invocations()) {
    const rootArgs = invocation.args.filter(arg => path.isAbsolute(arg) && /\.ts$/.test(arg));
    assert.deepEqual(rootArgs.map(arg => path.basename(arg)), ['b.ts'], 'only the modeled root is passed on argv');
  }
  pair.oracle.terminate(); pair.tsz.terminate();

  fs.writeFileSync(invocationLog, '', 'utf8');
  process.env.FAKE_ORACLE_SCENARIO = 'exact';
  process.env.FAKE_TSZ_SCENARIO = 'exact';
  pair = freshPair();
  await Promise.all([
    pair.oracle.transpile('', undefined, undefined, {
      sourceFiles: [{ name: 'input.js', content: 'module.exports = 1;\n' }],
      allowJs: false,
      declaration: false,
    }),
    pair.tsz.transpile('', undefined, undefined, {
      sourceFiles: [{ name: 'input.js', content: 'module.exports = 1;\n' }],
      allowJs: false,
      declaration: false,
    }),
  ]);
  for (const invocation of invocations()) {
    assert.equal(valueAfter(invocation.args, '--allowJs'), 'false', 'inferred JS input never overrides authored false');
    assert.equal(valueAfter(invocation.args, '--declaration'), 'false', 'product-domain inference never overrides authored false');
    assert.equal(invocation.args.includes('--target'), false);
    assert.equal(invocation.args.includes('--module'), false);
  }
  pair.oracle.terminate(); pair.tsz.terminate();

  fs.writeFileSync(invocationLog, '', 'utf8');
  process.env.FAKE_ORACLE_SCENARIO = 'exact';
  process.env.FAKE_TSZ_SCENARIO = 'exact';
  pair = freshPair();
  const resolvedInvocation = resolveAuthoredOptions({
    embeddedConfig: {
      strict: true,
      allowUnreachableCode: true,
      moduleResolution: 'bundler',
    },
    directives: { strict: true },
    variant: extractAuthoredVariantFromFilename('case(target=es2015,strict=false).js'),
  });
  await Promise.all([
    pair.oracle.transpile('', parseTarget(optionString(resolvedInvocation, 'target')!), undefined, {
      ...baseOptions,
      strict: optionBoolean(resolvedInvocation, 'strict'),
      allowUnreachableCode: optionBoolean(resolvedInvocation, 'allowUnreachableCode'),
      moduleResolution: optionString(resolvedInvocation, 'moduleResolution'),
    }),
    pair.tsz.transpile('', parseTarget(optionString(resolvedInvocation, 'target')!), undefined, {
      ...baseOptions,
      strict: optionBoolean(resolvedInvocation, 'strict'),
      allowUnreachableCode: optionBoolean(resolvedInvocation, 'allowUnreachableCode'),
      moduleResolution: optionString(resolvedInvocation, 'moduleResolution'),
    }),
  ]);
  for (const invocation of invocations()) {
    assert.equal(valueAfter(invocation.args, '--strict'), 'false');
    assert.equal(valueAfter(invocation.args, '--allowUnreachableCode'), 'true');
    assert.equal(valueAfter(invocation.args, '--moduleResolution'), 'bundler');
    assert.equal(invocation.args.includes('--strictNullChecks'), false, 'strict is never approximated');
    assert.equal(valueAfter(invocation.args, '--target'), 'es2015');
    assert.equal(invocation.args.includes('--module'), false, 'unauthored module remains absent');
  }
  pair.oracle.terminate(); pair.tsz.terminate();

  fs.writeFileSync(invocationLog, '', 'utf8');
  process.env.FAKE_ORACLE_SCENARIO = 'exact';
  process.env.FAKE_TSZ_SCENARIO = 'exact';
  pair = freshPair();
  await Promise.all([
    pair.oracle.transpile('', 9, 1, { ...baseOptions, noCheck: true, noLib: true }),
    pair.tsz.transpile('', 9, 1, { ...baseOptions, noCheck: true, noLib: true }),
  ]);
  assert.equal(invocations().length, 2);
  for (const invocation of invocations()) {
    assert.equal(invocation.args.includes('--noCheck'), true, 'authored noCheck is forwarded');
    assert.equal(invocation.args.includes('--noLib'), true, 'authored noLib is forwarded');
  }
  pair.oracle.terminate(); pair.tsz.terminate();

  fs.writeFileSync(invocationLog, '', 'utf8');
  pair = freshPair();
  const [noEmitOracle, noEmitTsz] = await Promise.all([
    pair.oracle.transpile('', 9, 1, { ...baseOptions, noEmit: true }),
    pair.tsz.transpile('', 9, 1, { ...baseOptions, noEmit: true }),
  ]);
  assert.equal(canonicalMatch(noEmitOracle, noEmitTsz), true);
  assert.deepEqual(noEmitOracle.jsProducts, []);
  assert.deepEqual(noEmitTsz.jsProducts, []);
  for (const invocation of invocations()) {
    assert.equal(invocation.args.includes('--noEmit'), true, 'authored noEmit is forwarded unchanged');
  }
  pair.oracle.terminate(); pair.tsz.terminate();

  process.env.FAKE_TSZ_SCENARIO = 'declaration-only-leak';
  pair = freshPair();
  const declarationOnly = { ...baseOptions, declaration: true, emitDeclarationOnly: true };
  const [declarationOracle, declarationTsz] = await Promise.all([
    pair.oracle.transpile('', 9, 1, declarationOnly),
    pair.tsz.transpile('', 9, 1, declarationOnly),
  ]);
  assert.equal(declarationOracle.jsProducts.length, 0);
  assert.equal(declarationTsz.jsProducts.length, 1);
  assert.equal(canonicalMatch(declarationOracle, declarationTsz), false, 'declaration-only JS leak remains red');
  pair.oracle.terminate(); pair.tsz.terminate();

  process.env.FAKE_ORACLE_SCENARIO = 'ts7-bytes';
  process.env.FAKE_TSZ_SCENARIO = 'ts6-bytes';
  pair = freshPair();
  const [ts7Result, legacyResult] = await Promise.all([
    pair.oracle.transpile('', 9, 1, baseOptions),
    pair.tsz.transpile('', 9, 1, baseOptions),
  ]);
  const legacyBaselineBytes = 'TS6_BYTES\n';
  assert.equal(legacyResult.jsProducts[0].content, legacyBaselineBytes);
  assert.equal(canonicalMatch(ts7Result, legacyResult), false, 'matching TS6 baseline bytes cannot create a pass');
  pair.oracle.terminate(); pair.tsz.terminate();

  process.env.FAKE_ORACLE_SCENARIO = 'exit3';
  process.env.FAKE_TSZ_SCENARIO = 'exit3';
  pair = freshPair();
  const [invalidOracle, invalidTsz] = await Promise.all([
    pair.oracle.transpile('', 9, 1, baseOptions),
    pair.tsz.transpile('', 9, 1, baseOptions),
  ]);
  assert.equal(compareCompilerOutcomes(invalidOracle.outcome, invalidTsz.outcome).match, false);
  pair.oracle.terminate(); pair.tsz.terminate();

  process.env.FAKE_TSZ_SCENARIO = 'timeout';
  pair = freshPair(50);
  await assert.rejects(pair.tsz.transpile('', 9, 1, { ...baseOptions, noEmitOnError: true }), /TIMEOUT:fake-tsz/);
  pair.oracle.terminate(); pair.tsz.terminate();

  process.env.FAKE_TSZ_SCENARIO = 'signal-crash';
  pair = freshPair();
  await assert.rejects(pair.tsz.transpile('', 9, 1, baseOptions), /CRASH:fake-tsz:SIGTERM/);
  pair.oracle.terminate(); pair.tsz.terminate();

  process.env.FAKE_ORACLE_SCENARIO = 'exact';
  process.env.FAKE_TSZ_SCENARIO = 'exact';
  fs.writeFileSync(invocationLog, '', 'utf8');
  pair = freshPair();
  const stagedInput = await pair.tsz.transpile('', 9, 1, {
    sourceFiles: [{ name: 'input.js', content: 'STAGED_SOURCE\n' }],
  });
  assert.deepEqual(stagedInput.jsProducts, [], 'staged JS input is never credited as emit');
  const repeatFirst = await pair.tsz.transpile('', 9, 1, baseOptions);
  const repeatSecond = await pair.tsz.transpile('', 9, 1, baseOptions);
  assert.deepEqual(repeatSecond, repeatFirst, 'warm and uncached canonical observations agree');
  assert.equal(invocations().filter(invocation => invocation.role === 'tsz').length, 3, 'results are never cached');
  pair.oracle.terminate(); pair.tsz.terminate();

  const sameNameOracle: EmitProduct[] = [
    { path: 'out/first/foo.js', content: 'FOO_JS\n' },
    { path: 'out/second/foo.js', content: 'FOO_JS\n' },
  ];
  assert.equal(compareCanonicalProductSets(sameNameOracle, sameNameOracle).match, true);
  assert.equal(compareCanonicalProductSets(sameNameOracle, sameNameOracle.slice(0, 1)).match, false);
  assert.equal(
    compareCanonicalProductSets(
      [{ path: 'out/value.js', content: 'value;\r\n' }],
      [{ path: 'out/value.js', content: 'value;\n' }],
    ).match,
    false,
    'CRLF and LF are distinct canonical product bytes',
  );

  assert.equal(compareEmit('line\r\nnext\r', 'line\nnext\n'), false, 'line-ending bytes remain canonical');
  assert.equal(compareEmit('value;\n', 'value;'), false, 'trailing whitespace remains product bytes');
  assert.equal(compareEmit('"use strict";\nvalue;', 'value;'), false, 'missing strict stays red');
  assert.equal(compareEmit('"use strict";\n"use strict";\nvalue;', '"use strict";\nvalue;'), false);
  assert.equal(compareEmit('/// <reference path="a" />\nvalue;', '/// <reference path="a" />\n\nvalue;'), false);
  assert.equal(compareEmit('value; // preserved\n', 'value;\n'), false, 'comment mismatch stays red');
  assert.equal(compareEmit('if (x) {\n  y();\n}\n', 'if (x) { y(); }\n'), false, 'whitespace mismatch stays red');
} finally {
  if (priorLog === undefined) delete process.env.FAKE_COMPILER_LOG;
  else process.env.FAKE_COMPILER_LOG = priorLog;
  if (priorOracleScenario === undefined) delete process.env.FAKE_ORACLE_SCENARIO;
  else process.env.FAKE_ORACLE_SCENARIO = priorOracleScenario;
  if (priorTszScenario === undefined) delete process.env.FAKE_TSZ_SCENARIO;
  else process.env.FAKE_TSZ_SCENARIO = priorTszScenario;
  fs.rmSync(tempDir, { recursive: true, force: true });
}

console.log('emit-canonical-truth: pinned-oracle boundary witnesses remain exact');
