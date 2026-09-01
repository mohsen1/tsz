import assert from 'node:assert/strict';
import { parseBaseline } from './baseline-parser.js';

const partialMissingDtsBaseline = `
//// [tests/cases/compiler/partialMissingDts.ts] ////
//// [index.ts]
export * from "./exporter";
//// [exporter.ts]
export const value = 1;
//// [index.js]
export * from "./exporter";
//// [index.d.ts]
export * from "./exporter";
!!!! File exporter.d.ts missing from original emit, but present in noCheck emit
//// [exporter.d.ts]
export declare const value = 1;
`;

const partial = parseBaseline(partialMissingDtsBaseline);
assert.equal(partial.dtsFileName, 'index.d.ts');
assert.equal(partial.dts, 'export * from "./exporter";\n');
assert.equal(partial.noDtsEmitExpected, false);
assert.deepEqual(partial.dtsOutputs, [
  { name: 'index.d.ts' },
  { name: 'exporter.d.ts' },
]);

const strippedBasenameCollisionBaseline = `
//// [tests/cases/compiler/strippedBasenameCollision.ts] ////
//// [foo.ts]
export const value = 1;
//// [foo.js]
export const value = 1;
//// [foo.d.ts]
export declare const value = 1;
!!!! File out/foo.d.ts missing from original emit, but present in noCheck emit
//// [foo.d.ts]
export declare const value: 1;
`;

const collision = parseBaseline(strippedBasenameCollisionBaseline);
assert.equal(collision.dtsFileName, 'foo.d.ts');
assert.equal(collision.dts, 'export declare const value = 1;\n');
assert.equal(collision.noDtsEmitExpected, false);

const multipleProductsBaseline = `
//// [tests/cases/compiler/multipleProducts.ts] ////
//// [a.ts]
export const a = 1;
//// [b.ts]
export const b = 1;
//// [a.js]
export const a = 1;
//// [b.js]
export const b = 1;
`;
const multiple = parseBaseline(multipleProductsBaseline);
assert.deepEqual(multiple.jsOutputs, [
  { name: 'a.js' },
  { name: 'b.js' },
]);
