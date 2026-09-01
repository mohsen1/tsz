import assert from 'node:assert/strict';
import * as path from 'node:path';
import type { Diagnostic as TypeScriptDiagnostic } from 'typescript/unstable/sync';
import {
  canonicalizeTszDiagnosticsJson,
  canonicalizeTypeScriptDiagnostics,
  normalizeDiagnosticText,
  witnessCodesMatch,
} from './diagnostic-witness.js';

const scopeDirectory = path.resolve('/tmp/emit-scope');
const invocationDirectory = path.join(scopeDirectory, 'cwd');
const scope = { invocationDirectory, scopeDirectory };

const tszJson = [{
  file: path.join(invocationDirectory, 'src', 'case.ts'),
  start: 4,
  length: 3,
  message_text: `First line\r\n${path.join(scopeDirectory, 'cwd', 'src', 'case.ts')}`,
  category: 'error',
  code: 2322,
  related_information: [{
    file: '',
    start: 0,
    length: 0,
    message_text: 'Types of property are incompatible.',
    code: 2322,
    depth: 1,
  }, {
    file: '',
    start: 0,
    length: 0,
    message_text: 'Nested cause.',
    code: 2322,
    depth: 2,
  }],
}, {
  file: '',
  start: 0,
  length: 0,
  message_text: "Cannot find global type 'Array'.",
  category: 'error',
  code: 2318,
  related_information: [],
}];
const tszWitnesses = canonicalizeTszDiagnosticsJson(tszJson, scope);
assert.ok(tszWitnesses);
assert.deepEqual(tszWitnesses[0], {
  path: 'src/case.ts',
  start: 4,
  length: 3,
  category: 'error',
  code: 'TS2322',
  text: 'First line\n<invocation-scope>/cwd/src/case.ts',
  messageChain: [{
    text: 'Types of property are incompatible.',
    category: 'error',
    code: 'TS2322',
    next: [{
      text: 'Nested cause.',
      category: 'error',
      code: 'TS2322',
      next: [],
    }],
  }],
  relatedInformation: [],
});
assert.deepEqual(
  tszWitnesses[1],
  {
    path: null,
    start: null,
    length: null,
    category: 'error',
    code: 'TS2318',
    text: "Cannot find global type 'Array'.",
    messageChain: [],
    relatedInformation: [],
  },
  'global TSZ diagnostics have no fabricated file or span',
);
assert.equal(witnessCodesMatch(tszWitnesses, ['TS2322', 'TS2318']), true);
assert.equal(witnessCodesMatch(tszWitnesses, ['TS2318', 'TS2322']), false);
assert.equal(
  canonicalizeTszDiagnosticsJson([tszJson[0], tszJson[0]], scope)?.length,
  2,
  'the original TSZ invocation order and duplicates are retained',
);
assert.equal(
  canonicalizeTszDiagnosticsJson([{
    ...tszJson[0],
    related_information: [{
      ...tszJson[0].related_information[0],
      file: path.join(invocationDirectory, 'related.ts'),
      start: 1,
      length: 1,
    }],
  }], scope),
  undefined,
  'the current flat TSZ schema fails closed for located recursive related information',
);
assert.equal(
  canonicalizeTszDiagnosticsJson([{ ...tszJson[0], future_identity: [] }], scope),
  undefined,
  'unknown TSZ identity fields fail closed instead of being silently discarded',
);

const apiDiagnostic = (overrides: Record<string, unknown> = {}): TypeScriptDiagnostic => ({
  fileName: path.join(invocationDirectory, 'src', 'case.ts'),
  pos: 4,
  end: 7,
  category: 1,
  code: 2322,
  text: 'Top\r\nmessage',
  messageChain: [{
    fileName: path.join(invocationDirectory, 'src', 'case.ts'),
    pos: 4,
    end: 7,
    category: 1,
    code: 2200,
    text: 'Nested message',
    messageChain: [{
      fileName: path.join(invocationDirectory, 'src', 'case.ts'),
      pos: 4,
      end: 7,
      category: 1,
      code: 2322,
      text: 'Nested leaf',
    }],
  }],
  relatedInformation: [{
    fileName: path.join(invocationDirectory, 'src', 'declaration.ts'),
    pos: 12,
    end: 17,
    category: 3,
    code: 6500,
    text: 'Declared here.',
    relatedInformation: [{
      fileName: path.join(invocationDirectory, 'src', 'origin.ts'),
      pos: 2,
      end: 3,
      category: 3,
      code: 6501,
      text: 'Originated here.',
    }],
  }],
  ...overrides,
} as unknown as TypeScriptDiagnostic);

const apiWitnesses = canonicalizeTypeScriptDiagnostics([apiDiagnostic()], scope);
assert.ok(apiWitnesses);
assert.equal(apiWitnesses[0].path, 'src/case.ts');
assert.equal(apiWitnesses[0].text, 'Top\nmessage');
assert.equal(apiWitnesses[0].messageChain[0].next[0].text, 'Nested leaf');
assert.equal(apiWitnesses[0].relatedInformation[0].path, 'src/declaration.ts');
assert.equal(apiWitnesses[0].relatedInformation[0].relatedInformation[0].path, 'src/origin.ts');

const globalApi = apiDiagnostic({
  fileName: undefined,
  pos: 0,
  end: 0,
  code: 2318,
  text: "Cannot find global type 'Array'.",
  messageChain: undefined,
  relatedInformation: undefined,
});
const ordered = canonicalizeTypeScriptDiagnostics([
  apiDiagnostic({
    fileName: path.join(invocationDirectory, 'z.ts'),
    pos: 0,
    end: 1,
    messageChain: undefined,
    relatedInformation: undefined,
  }),
  globalApi,
  apiDiagnostic({
    fileName: path.join(invocationDirectory, 'a.ts'),
    pos: 0,
    end: 1,
    messageChain: undefined,
    relatedInformation: undefined,
  }),
  apiDiagnostic({
    fileName: path.join(invocationDirectory, 'a.ts'),
    pos: 0,
    end: 1,
    messageChain: undefined,
    relatedInformation: undefined,
  }),
], scope);
assert.ok(ordered);
assert.deepEqual(
  ordered.map(diagnostic => diagnostic.path),
  [null, 'a.ts', 'z.ts'],
  'API phases are canonically sorted and exact duplicates are removed once',
);
const physicalPathOrder = canonicalizeTypeScriptDiagnostics([
  apiDiagnostic({
    fileName: path.join(scopeDirectory, 'z.ts'),
    pos: 0,
    end: 1,
    messageChain: undefined,
    relatedInformation: undefined,
  }),
  apiDiagnostic({
    fileName: path.join(invocationDirectory, 'a.ts'),
    pos: 0,
    end: 1,
    messageChain: undefined,
    relatedInformation: undefined,
  }),
], scope);
assert.deepEqual(
  physicalPathOrder?.map(diagnostic => diagnostic.path),
  ['a.ts', '../z.ts'],
  'canonical order follows original physical paths before invocation-relative normalization',
);
assert.equal(
  canonicalizeTypeScriptDiagnostics(
    [apiDiagnostic({ fileName: path.join(scopeDirectory, '.typescript-api', 'tsconfig.json') })],
    { ...scope, forbiddenFile: path.join(scopeDirectory, '.typescript-api', 'tsconfig.json') },
  ),
  undefined,
  'synthetic-config locations can never stand in for original CLI identity',
);

assert.equal(normalizeDiagnosticText('left\r\nright\r', scopeDirectory), 'left\nright\n');

console.log('emit-diagnostic-witness: structured identity is normalized, recursive, ordered, and fail-closed');
