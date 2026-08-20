import assert from 'node:assert/strict';
import fs from 'node:fs';
import { parseDirectiveLine, parseFlagDirectiveLine, splitListValues, firstListValue } from './directives.js';

// Locks the emit-harness directive recognizers to the canonical grammar
// vectors shared with crates/conformance/src/test_directives.rs and
// scripts/conformance/test_corpus_coverage.py (issue #13127).
// Run with: node dist/directives-spec.test.js (after building the runner).

interface SpecVectors {
  directive_lines: Array<{ line: string; key: string | null; value?: string }>;
  flag_lines: Array<{ line: string; name: string | null }>;
  list_values: Array<{ value: string; list: string[]; first: string }>;
  bool_values: Array<{ value: string; bool: boolean | null }>;
}

const vectorsUrl = new URL('../../test-directives/spec-vectors.json', import.meta.url);
const vectors: SpecVectors = JSON.parse(fs.readFileSync(vectorsUrl, 'utf-8'));

for (const { line, key, value } of vectors.directive_lines) {
  const parsed = parseDirectiveLine(line);
  if (key === null) {
    assert.equal(parsed, undefined, `expected no directive in ${JSON.stringify(line)}`);
  } else {
    assert.ok(parsed, `expected a directive in ${JSON.stringify(line)}`);
    assert.equal(parsed.key, key, `key for ${JSON.stringify(line)}`);
    assert.equal(parsed.value, value, `value for ${JSON.stringify(line)}`);
  }
}

for (const { line, name } of vectors.flag_lines) {
  assert.equal(
    parseFlagDirectiveLine(line) ?? null,
    name,
    `flag for ${JSON.stringify(line)}`,
  );
}

for (const { value, list, first } of vectors.list_values) {
  assert.deepEqual(splitListValues(value), list, `list for ${JSON.stringify(value)}`);
  assert.equal(firstListValue(value), first, `first for ${JSON.stringify(value)}`);
}

console.log('directives-spec: all spec vectors passed');
