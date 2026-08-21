import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseBaseline } from './baseline-parser.js';
import {
  canonicalUnsupportedReasons,
  hasEmitSidecar,
  retainCanonicalInventory,
  type CanonicalSupportInput,
} from './canonical-support.js';

const supported: CanonicalSupportInput = {
  parserHasSource: true,
  parserHasJs: true,
  parserHasDts: true,
  dtsOnly: false,
  sourceReadFailed: false,
  target: 9,
  module: 1,
  moduleResolution: 'bundler',
  alwaysStrict: true,
  hasDownlevelIterationOption: false,
  esModuleInteropDisabled: false,
  hasOutFile: false,
  hasBaseUrl: false,
  hasMapOption: false,
  hasBaselineSidecar: false,
};
assert.deepEqual(canonicalUnsupportedReasons(supported), []);

const families: Array<[Partial<CanonicalSupportInput>, string]> = [
  [{ parserHasSource: false }, 'baseline-parser-missing-source'],
  [{ parserHasJs: false }, 'baseline-parser-missing-js-products'],
  [{ dtsOnly: true, parserHasDts: false }, 'dts-only-baseline-missing-dts-products'],
  [{ sourceReadFailed: true }, 'source-read-failed'],
  [{ target: 1 }, 'target-below-es2015'],
  [{ module: 0 }, 'module-kind-0'],
  [{ module: 2 }, 'module-kind-2'],
  [{ module: 3 }, 'module-kind-3'],
  [{ module: 4 }, 'module-kind-4'],
  [{ moduleResolution: 'classic' }, 'module-resolution-classic'],
  [{ moduleResolution: 'node' }, 'module-resolution-node'],
  [{ moduleResolution: 'node10' }, 'module-resolution-node10'],
  [{ alwaysStrict: false }, 'always-strict-false'],
  [{ hasDownlevelIterationOption: true }, 'downlevel-iteration-option'],
  [{ esModuleInteropDisabled: true }, 'es-module-interop-false'],
  [{ hasOutFile: true }, 'out-file-product-layout'],
  [{ hasBaseUrl: true }, 'base-url-invocation'],
  [{ hasMapOption: true }, 'source-map-products-not-compared'],
  [{ hasBaselineSidecar: true }, 'source-map-products-not-compared'],
];
for (const [change, reason] of families) {
  assert.equal(
    canonicalUnsupportedReasons({ ...supported, ...change }).includes(reason),
    true,
    `${reason} is retained as a named terminal unsupported row`,
  );
}

const inventory = Array.from({ length: 10 }, (_, index) => index);
assert.deepEqual(retainCanonicalInventory(inventory, Infinity), inventory);
assert.deepEqual(retainCanonicalInventory(inventory, 3), [0, 1, 2]);

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '../../..');
const baselineDir = path.join(root, 'TypeScript/tests/baselines/reference');
const entries = new Set(fs.readdirSync(baselineDir));
const jsCandidates = [...entries].filter(name => name.endsWith('.js'));
const jsMaps = [...entries].filter(name => name.endsWith('.js.map'));
const sourcemapTexts = [...entries].filter(name => name.endsWith('.sourcemap.txt'));
assert.equal(jsCandidates.length, 13_806, 'pinned canonical JS inventory is not parse-filtered');
assert.equal(jsMaps.length, 217, 'every pinned .js.map sidecar remains visible');
assert.equal(sourcemapTexts.length, 227, 'every pinned sourcemap baseline remains visible');
let multiSourceCount = 0;
let multiJsProductCount = 0;
let multiDtsProductCount = 0;
let duplicateJsNameCount = 0;
for (const baselineName of jsCandidates) {
  const parsed = parseBaseline(fs.readFileSync(path.join(baselineDir, baselineName), 'utf8'));
  if (parsed.sourceFiles.length > 1) multiSourceCount++;
  if (parsed.jsOutputs.length > 1) multiJsProductCount++;
  if (parsed.dtsOutputs.length > 1) multiDtsProductCount++;
  if (new Set(parsed.jsOutputs.map(product => product.name)).size < parsed.jsOutputs.length) {
    duplicateJsNameCount++;
  }
}
assert.equal(multiSourceCount, 2_424, 'all pinned multi-source rows retain complete product comparison');
assert.equal(multiJsProductCount, 1_622, 'multi-JS product rows remain visible');
assert.equal(multiDtsProductCount, 564, 'multi-DTS product rows remain visible');
assert.equal(duplicateJsNameCount, 139, 'stripped same-name multiplicity remains visible');
for (const sidecar of jsMaps) {
  const baseline = sidecar.slice(0, -'.map'.length);
  assert.equal(entries.has(baseline), true);
  assert.equal(hasEmitSidecar(baseline, entries), true);
}
for (const sidecar of sourcemapTexts) {
  const baseline = `${sidecar.slice(0, -'.sourcemap.txt'.length)}.js`;
  assert.equal(entries.has(baseline), true);
  assert.equal(hasEmitSidecar(baseline, entries), true);
}

console.log('emit-canonical-support: unsupported families and sidecar inventory stay in the denominator');
