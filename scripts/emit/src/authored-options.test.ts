import assert from 'node:assert/strict';
import {
  authoredOptionDisposition,
  authoredOptionFailureReasons,
  extractAuthoredVariantFromFilename,
  invalidAuthoredOptions,
  optionBoolean,
  optionLibList,
  optionString,
  parseEmbeddedCompilerOptions,
  resolveAuthoredOptions,
  unhandledAuthoredOptions,
} from './authored-options.js';

assert.deepEqual(
  extractAuthoredVariantFromFilename('deleteExpressionMustBeOptional(strict=false).js'),
  { base: 'deleteExpressionMustBeOptional', strict: 'false' },
  'the filename-selected strict=false value enters authored precedence intact',
);

const embeddedOnly = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: {},
  embeddedConfig: {
    strict: false,
    allowUnreachableCode: true,
    moduleResolution: 'bundler',
  },
});
assert.equal(optionBoolean(embeddedOnly, 'strict'), false);
assert.equal(optionBoolean(embeddedOnly, 'allowUnreachableCode'), true);
assert.equal(optionString(embeddedOnly, 'moduleResolution'), 'bundler');
assert.equal(embeddedOnly.get('strict')?.source, 'embedded-config');
assert.deepEqual(unhandledAuthoredOptions(embeddedOnly), []);

const precedence = resolveAuthoredOptions({
  embeddedConfig: { strict: true, allowUnreachableCode: false },
  directives: { strict: false, allowunreachablecode: true },
  variant: { base: 'case', strict: 'true' },
});
assert.equal(optionBoolean(precedence, 'strict'), true, 'filename variant overrides directive and config');
assert.equal(precedence.get('strict')?.source, 'filename-variant');
assert.equal(optionBoolean(precedence, 'allowUnreachableCode'), true, 'directive overrides embedded config');
assert.equal(precedence.get('allowunreachablecode')?.source, 'directive');
assert.equal(authoredOptionDisposition('base'), 'harness-only');
assert.equal(authoredOptionDisposition('noImplicitReferences'), 'harness-only');
assert.equal(authoredOptionDisposition('noTypesAndSymbols'), 'harness-only');
assert.equal(authoredOptionDisposition('checkJs'), 'forwarded');

const newlyForwarded = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: {
    checkjs: true,
    noimplicitany: true,
    nounusedlocals: false,
    nounusedparameters: true,
    skiplibcheck: false,
    strictpropertyinitialization: true,
    notypesandsymbols: true,
  },
  embeddedConfig: {},
});
assert.deepEqual(
  unhandledAuthoredOptions(newlyForwarded),
  [],
  'owned compiler options and harness controls are classified without being dropped',
);
assert.equal(optionBoolean(newlyForwarded, 'checkJs'), true);
assert.equal(optionBoolean(newlyForwarded, 'noImplicitAny'), true);
assert.equal(optionBoolean(newlyForwarded, 'noUnusedLocals'), false);
assert.equal(optionBoolean(newlyForwarded, 'noUnusedParameters'), true);
assert.equal(optionBoolean(newlyForwarded, 'skipLibCheck'), false);
assert.equal(optionBoolean(newlyForwarded, 'strictPropertyInitialization'), true);
assert.deepEqual(authoredOptionFailureReasons(newlyForwarded), []);

const unrepresentedRoots = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: { currentdirectory: '/project', noresolve: true },
  embeddedConfig: {},
});
assert.deepEqual(
  authoredOptionFailureReasons(unrepresentedRoots),
  [
    'unhandled-authored-option:currentdirectory(directive)',
    'unhandled-authored-option:noresolve(directive)',
  ],
  'unrepresented root-selection directives are terminal rather than silently ignored',
);
const headerAccounting = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: { checkjs: true, noimplicitreferences: false, notypesandsymbols: true },
  embeddedConfig: {},
});
assert.deepEqual(
  unhandledAuthoredOptions(headerAccounting).map(option => option.key),
  [],
  'every parsed header directive is classified; compiler options are never silently dropped',
);
assert.deepEqual(invalidAuthoredOptions(headerAccounting), []);

const invalidHarnessControl = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: { notypesandsymbols: 'sometimes' },
  embeddedConfig: {},
});
assert.deepEqual(
  invalidAuthoredOptions(invalidHarnessControl).map(option => option.key),
  ['notypesandsymbols'],
  'harness controls require exact boolean values instead of being silently coerced',
);

const provenanceInvalid = resolveAuthoredOptions({
  variant: extractAuthoredVariantFromFilename('case(strict= false).js'),
  directives: {},
  embeddedConfig: { alwaysStrict: 'false', lib: 'es2022,dom' },
});
assert.equal(optionBoolean(provenanceInvalid, 'strict'), undefined, 'filename booleans are not trimmed');
assert.equal(optionBoolean(provenanceInvalid, 'alwaysStrict'), undefined, 'JSON strings are not coerced to booleans');
assert.deepEqual(
  invalidAuthoredOptions(provenanceInvalid).map(option => option.key),
  ['alwaysstrict', 'lib', 'strict'],
  'embedded booleans require JSON booleans and embedded lib requires an array',
);

const exactEmbeddedLib = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: {},
  embeddedConfig: { lib: ['ES2022', 'DOM'] },
});
assert.deepEqual(optionLibList(exactEmbeddedLib), ['ES2022', 'DOM']);
assert.deepEqual(invalidAuthoredOptions(exactEmbeddedLib), []);

const malformedBoolean = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: { strict: 'true, false' },
  embeddedConfig: {},
});
assert.deepEqual(
  invalidAuthoredOptions(malformedBoolean).map(option => option.key),
  ['strict'],
  'an unselected multi-value scalar cannot be approximated by its first token',
);

const invalidEnum = resolveAuthoredOptions({
  variant: { base: 'case' },
  directives: { target: 'future-ish', module: 'commonjs, esnext' },
  embeddedConfig: {},
});
assert.deepEqual(
  invalidAuthoredOptions(invalidEnum).map(option => option.key),
  ['module', 'target'],
  'unknown enums and unselected scalar variants cannot fall back to a different argv value',
);

const validEmbedded = parseEmbeddedCompilerOptions([
  {
    name: '/project/tsconfig.json',
    content: '{ // JSONC\n "compilerOptions": { "strict": false, "allowUnreachableCode": true, },\n}',
  },
]);
assert.deepEqual(validEmbedded.reasons, []);
assert.deepEqual(validEmbedded.compilerOptions, { strict: false, allowUnreachableCode: true });
assert.deepEqual(validEmbedded.configFileNames, ['/project/tsconfig.json']);

const malformedEmbedded = parseEmbeddedCompilerOptions([
  { name: 'tsconfig.json', content: '{ "compilerOptions": { "strict": false,, } }' },
]);
assert.deepEqual(
  malformedEmbedded.reasons,
  ['embedded-tsconfig-jsonc-parse-error:tsconfig.json'],
  'JSONC parse errors make the row terminal instead of yielding partial options',
);

const inheritedEmbedded = parseEmbeddedCompilerOptions([
  {
    name: 'tsconfig.json',
    content: '{ "extends": "./base.json", "compilerOptions": { "strict": false } }',
  },
]);
assert.deepEqual(
  inheritedEmbedded.reasons,
  ['unhandled-embedded-tsconfig-field:extends(tsconfig.json)'],
  'config inheritance cannot silently contribute an unaccounted compiler option',
);

const conflictingEmbedded = parseEmbeddedCompilerOptions([
  { name: '/a/tsconfig.json', content: '{ "compilerOptions": { "strict": false } }' },
  { name: '/b/tsconfig.json', content: '{ "compilerOptions": { "strict": true } }' },
]);
assert.deepEqual(
  conflictingEmbedded.reasons,
  ['conflicting-embedded-tsconfigs:/a/tsconfig.json|/b/tsconfig.json'],
  'multiple conflicting embedded configs cannot be merged option-by-option',
);

const identicalEmbedded = parseEmbeddedCompilerOptions([
  { name: '/a/tsconfig.json', content: '{ "compilerOptions": { "strict": false, "target": "es2015" } }' },
  { name: '/b/tsconfig.json', content: '{ "compilerOptions": { "target": "es2015", "strict": false } }' },
]);
assert.deepEqual(identicalEmbedded.reasons, []);
assert.deepEqual(identicalEmbedded.compilerOptions, { strict: false, target: 'es2015' });

console.log('emit-authored-options: precedence, config parsing, and unsupported accounting are exact');
