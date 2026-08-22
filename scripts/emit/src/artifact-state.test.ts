import assert from 'node:assert/strict';
import {
  artifactStatus,
  compilerArtifactState,
  ensureMeasuredArtifact,
  type MeasuredArtifactResult,
} from './artifact-state.js';

const success = { exitCode: 0, diagnosticCodes: [] };
const diagnostic = { exitCode: 1, diagnosticCodes: ['TS2322'] };
const semanticIncomplete = { exitCode: 3, diagnosticCodes: [] };
const crash = { exitCode: 4, diagnosticCodes: [] };
assert.equal(compilerArtifactState(success, success), 'complete');
assert.equal(compilerArtifactState(diagnostic, diagnostic), 'incomplete');
assert.equal(compilerArtifactState(success, semanticIncomplete), 'incomplete');
assert.equal(compilerArtifactState(semanticIncomplete, success), 'crash');
assert.equal(compilerArtifactState(success, crash), 'crash');
assert.equal(artifactStatus('unsupported', false), 'unsupported');
assert.equal(artifactStatus('timeout', false), 'timeout');
assert.equal(artifactStatus('crash', false), 'crash');
assert.equal(artifactStatus('incomplete', false), 'incomplete');
assert.equal(artifactStatus('complete', false), 'fail');
assert.equal(artifactStatus('complete', true), 'pass');
assert.equal(artifactStatus('unsupported', null), 'skip');

const vacuous: MeasuredArtifactResult = {
  artifactState: 'complete',
  jsMatch: null,
  dtsMatch: null,
};
ensureMeasuredArtifact(vacuous, { jsOnly: true, dtsOnly: false });
assert.equal(vacuous.artifactState, 'incomplete');
assert.equal(vacuous.jsMatch, false);
assert.match(vacuous.jsError!, /INCOMPLETE_CANONICAL_EMIT/);

console.log('emit-artifact-state: nonzero, crash, timeout, unsupported, and vacuous states are explicit');
