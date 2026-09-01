import assert from 'node:assert/strict';
import * as path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const runner = path.join(__dirname, 'runner.js');

const run = (args: string[]) => spawnSync(process.execPath, [runner, ...args], {
  encoding: 'utf8',
  env: {
    ...process.env,
    // These witnesses intentionally never construct or invoke the product adapter.
    TSZ_BIN: process.execPath,
    TSZ_EMIT_BLOB: '0',
  },
});

const typoedFilter = run(['--filter=__definitely_not_a_canonical_emit_row__', '--max=1', '--js-only']);
assert.equal(typoedFilter.status, 2, 'a typoed filter cannot produce a vacuous green run');
assert.match(typoedFilter.stdout, /Found 0 test cases/);
assert.match(typoedFilter.stderr, /No canonical emit test cases selected/);

const exhaustedOffset = run(['--filter=ArrowFunction1', '--offset=99', '--max=1', '--js-only']);
assert.equal(exhaustedOffset.status, 2, 'an offset beyond selected inventory cannot pass');
assert.match(exhaustedOffset.stdout, /Found 0 test cases/);
assert.match(exhaustedOffset.stderr, /No canonical emit test cases selected/);

const conflictingModes = run(['--filter=ArrowFunction1', '--max=1', '--js-only', '--dts-only']);
assert.equal(conflictingModes.status, 2, 'mutually exclusive surface modes cannot erase every measured surface');
assert.match(conflictingModes.stderr, /--js-only and --dts-only are mutually exclusive/);

console.log('emit-runner-nonvacuity: empty selections and vacuous modes fail closed');
