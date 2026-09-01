import assert from 'node:assert/strict';
import { compareCanonicalProductSets } from './canonical-products.js';
import type { DiagnosticWitness } from './diagnostic-witness.js';
import {
  artifactCandidateTotal,
  artifactHasNonPass,
  artifactSurfaceObservation,
  artifactStatus,
  compareArtifactOutcomes,
  compilerArtifactState,
  emptyArtifactProductCounts,
  emptyArtifactStatusCounts,
  ensureMeasuredArtifact,
  recordArtifactProduct,
  recordArtifactStatus,
  selectArtifactSurfaces,
  type MeasuredArtifactResult,
} from './artifact-state.js';

const success = { exitCode: 0, diagnosticCodes: [] };
const diagnostic1 = { exitCode: 1, diagnosticCodes: ['TS2322'] };
const diagnostic2 = { exitCode: 2, diagnosticCodes: ['TS1005', 'TS1128'] };
const semanticIncomplete = { exitCode: 3, diagnosticCodes: [] };
const crash = { exitCode: 4, diagnosticCodes: [] };
assert.equal(compilerArtifactState(success, success), 'complete');
assert.equal(compilerArtifactState(diagnostic1, diagnostic1), 'complete');
assert.equal(compilerArtifactState(diagnostic2, diagnostic2), 'complete');
assert.equal(compilerArtifactState(success, semanticIncomplete), 'incomplete');
assert.equal(compilerArtifactState(semanticIncomplete, success), 'crash');
assert.equal(compilerArtifactState(success, crash), 'crash');
assert.deepEqual(compareArtifactOutcomes(success, success), { match: true });

const observedCodeOnlyOutcome = (diagnostics: DiagnosticWitness[]) => ({
  exitCode: 1,
  diagnosticCodes: diagnostics.map(diagnostic => diagnostic.code),
});
const observedStructuredOutcome = (diagnostics: DiagnosticWitness[]) => ({
  ...observedCodeOnlyOutcome(diagnostics),
  diagnosticWitnesses: diagnostics,
});
const oracleDiagnostic: DiagnosticWitness = {
  path: 'src/case.ts',
  start: 4,
  length: 3,
  category: 'error',
  code: 'TS2322',
  text: "Type 'Source' is not assignable to type 'Target'.",
  messageChain: [{
    text: "Type 'string' is not assignable to type 'number'.",
    category: 'message',
    code: 'TS2322',
    next: [],
  }],
  relatedInformation: [{
    path: 'src/declaration.ts',
    start: 12,
    length: 5,
    category: 'message',
    code: 'TS6500',
    text: 'The expected type comes from this declaration.',
    messageChain: [],
    relatedInformation: [],
  }],
};
const exactDiagnostic: DiagnosticWitness = {
  ...oracleDiagnostic,
  messageChain: oracleDiagnostic.messageChain.map(message => ({ ...message, next: [] })),
  relatedInformation: oracleDiagnostic.relatedInformation.map(related => ({
    ...related,
    messageChain: [],
    relatedInformation: [],
  })),
};
const sameCodeIdentityCases: Array<[string, DiagnosticWitness]> = [
  ['different path', { ...exactDiagnostic, path: 'src/other.ts' }],
  ['different span', { ...exactDiagnostic, start: 5 }],
  ['different category', { ...exactDiagnostic, category: 'warning' }],
  ['different message chain', {
    ...exactDiagnostic,
    messageChain: [{
      ...exactDiagnostic.messageChain[0],
      text: "Type 'boolean' is not assignable to type 'number'.",
    }],
  }],
  ['different related information', {
    ...exactDiagnostic,
    relatedInformation: [{
      ...exactDiagnostic.relatedInformation[0],
      start: 13,
    }],
  }],
];
const oracleCodeOnly = observedCodeOnlyOutcome([oracleDiagnostic]);
for (const [label, candidate] of [...sameCodeIdentityCases, ['exact code-only identity', exactDiagnostic] as const]) {
  const candidateCodeOnly = observedCodeOnlyOutcome([candidate]);
  assert.deepEqual(
    candidateCodeOnly,
    oracleCodeOnly,
    `${label} is indistinguishable after the current CLI boundary`,
  );
  const outcome = compareArtifactOutcomes(oracleCodeOnly, candidateCodeOnly);
  assert.equal(outcome.match, false, `${label} must not pass on code-only equality`);
  assert.match(outcome.error!, /UNVERIFIED_DIAGNOSTIC_IDENTITY/);
}
assert.deepEqual(
  compareArtifactOutcomes(
    observedStructuredOutcome([oracleDiagnostic]),
    observedStructuredOutcome([exactDiagnostic]),
  ),
  { match: true },
  'an ordinary nonzero result passes only with exact complete ordered identity',
);
for (const [label, candidate] of sameCodeIdentityCases) {
  const outcome = compareArtifactOutcomes(
    observedStructuredOutcome([oracleDiagnostic]),
    observedStructuredOutcome([candidate]),
  );
  assert.equal(outcome.match, false, `${label} remains a structured mismatch`);
  assert.match(outcome.error!, /diagnostic identity mismatch/);
}

const secondDiagnostic: DiagnosticWitness = {
  ...exactDiagnostic,
  path: 'src/second.ts',
  start: 20,
  code: 'TS1128',
  text: 'Declaration or statement expected.',
  messageChain: [],
  relatedInformation: [],
};
const sameCodeFirst: DiagnosticWitness = {
  ...exactDiagnostic,
  path: 'src/first.ts',
  start: 1,
};
const sameCodeSecond: DiagnosticWitness = {
  ...exactDiagnostic,
  path: 'src/second.ts',
  start: 2,
};
const reorderedSameCode = compareArtifactOutcomes(
  observedCodeOnlyOutcome([sameCodeFirst, sameCodeSecond]),
  observedCodeOnlyOutcome([sameCodeSecond, sameCodeFirst]),
);
assert.equal(reorderedSameCode.match, false, 'same-code diagnostic order must not pass');
assert.match(reorderedSameCode.error!, /UNVERIFIED_DIAGNOSTIC_IDENTITY/);
const structuredReorder = compareArtifactOutcomes(
  observedStructuredOutcome([sameCodeFirst, sameCodeSecond]),
  observedStructuredOutcome([sameCodeSecond, sameCodeFirst]),
);
assert.equal(structuredReorder.match, false, 'structured same-code order remains identity');
assert.match(structuredReorder.error!, /diagnostics\[0\]/);
const duplicateMismatch = compareArtifactOutcomes(
  observedStructuredOutcome([sameCodeFirst]),
  {
    exitCode: 1,
    diagnosticCodes: ['TS2322'],
    diagnosticWitnesses: [sameCodeFirst, sameCodeFirst],
  },
);
assert.equal(duplicateMismatch.match, false, 'duplicate witness sequences cannot pass');
assert.match(duplicateMismatch.error!, /diagnostics\.length/);

const globalDiagnostic: DiagnosticWitness = {
  path: null,
  start: null,
  length: null,
  category: 'error',
  code: 'TS2318',
  text: "Cannot find global type 'Array'.",
  messageChain: [],
  relatedInformation: [],
};
assert.deepEqual(
  compareArtifactOutcomes(
    observedStructuredOutcome([globalDiagnostic]),
    observedStructuredOutcome([{ ...globalDiagnostic }]),
  ),
  { match: true },
  'global diagnostics retain explicit null path/span identity',
);

const reorderedCodes = compareArtifactOutcomes(
  observedCodeOnlyOutcome([oracleDiagnostic, secondDiagnostic]),
  observedCodeOnlyOutcome([secondDiagnostic, oracleDiagnostic]),
);
assert.equal(reorderedCodes.match, false);
assert.match(reorderedCodes.error!, /diagnostic mismatch/);
const extraDiagnostic = compareArtifactOutcomes(
  observedCodeOnlyOutcome([oracleDiagnostic]),
  observedCodeOnlyOutcome([oracleDiagnostic, secondDiagnostic]),
);
assert.equal(extraDiagnostic.match, false);
assert.match(extraDiagnostic.error!, /diagnostic mismatch/);

for (const diagnostic of [diagnostic1, diagnostic2]) {
  const codeOnlyOutcome = compareArtifactOutcomes(diagnostic, diagnostic);
  assert.equal(codeOnlyOutcome.match, false, `ordinary exit ${diagnostic.exitCode} stays red`);
  assert.match(codeOnlyOutcome.error!, /UNVERIFIED_DIAGNOSTIC_IDENTITY/);
  assert.deepEqual(
    artifactSurfaceObservation(
      compilerArtifactState(diagnostic, diagnostic),
      true,
      codeOnlyOutcome.match,
      true,
    ),
    {
      selected: true,
      match: false,
      productMatch: true,
      status: 'fail',
    },
  );
}

const codeMismatch = compareArtifactOutcomes(
  diagnostic1,
  { exitCode: 1, diagnosticCodes: ['TS2345'] },
);
assert.equal(codeMismatch.match, false);
assert.match(codeMismatch.error!, /diagnostic mismatch/);
const exitMismatch = compareArtifactOutcomes(diagnostic1, diagnostic2);
assert.equal(exitMismatch.match, false);
assert.match(exitMismatch.error!, /outcome mismatch/);
const outcomeOnlyFailure = artifactSurfaceObservation('complete', true, false, true);
assert.equal(outcomeOnlyFailure.status, 'fail');
assert.equal(outcomeOnlyFailure.productMatch, true, 'outcome mismatch does not erase product parity');

const incompleteOutcome = compareArtifactOutcomes(success, semanticIncomplete);
const incompleteObservation = artifactSurfaceObservation(
  compilerArtifactState(success, semanticIncomplete),
  true,
  incompleteOutcome.match,
  true,
);
assert.equal(incompleteOutcome.match, false);
assert.equal(incompleteObservation.status, 'incomplete');
assert.equal(incompleteObservation.productMatch, true);

const expected = [{ path: 'out.js', content: 'expected' }];
for (const [kind, oracleProducts, productProducts] of [
  ['missing', expected, []],
  ['extra', [], expected],
  ['content', expected, [{ path: 'out.js', content: 'actual' }]],
] as const) {
  const comparison = compareCanonicalProductSets(oracleProducts, productProducts);
  const observation = artifactSurfaceObservation('incomplete', true, false, comparison.match);
  assert.equal(comparison.match, false);
  assert.equal(comparison.mismatches[0]?.kind, kind);
  assert.equal(observation.status, 'incomplete', `${kind} keeps the typed terminal state`);
  assert.equal(observation.productMatch, false, `${kind} remains a raw product mismatch`);
}

assert.equal(artifactStatus('unsupported', false), 'unsupported');
assert.equal(artifactStatus('timeout', false), 'timeout');
assert.equal(artifactStatus('crash', false), 'crash');
assert.equal(artifactStatus('incomplete', false), 'incomplete');
assert.equal(artifactStatus('complete', false), 'fail');
assert.equal(artifactStatus('complete', true), 'pass');
assert.equal(artifactStatus('unsupported', null), 'skip');

const counts = emptyArtifactStatusCounts();
for (const status of [
  'pass',
  'fail',
  'skip',
  'unsupported',
  'timeout',
  'crash',
  'incomplete',
] as const) {
  recordArtifactStatus(counts, status);
}
assert.deepEqual(counts, {
  pass: 1,
  fail: 1,
  skip: 1,
  unsupported: 1,
  timeout: 1,
  crash: 1,
  incomplete: 1,
});
assert.equal(artifactCandidateTotal(counts), 6, 'only an unselected surface leaves the domain');
assert.equal(artifactHasNonPass(counts), true, 'every typed terminal state remains fail-closed');

const green = emptyArtifactStatusCounts();
recordArtifactStatus(green, 'pass');
recordArtifactStatus(green, 'skip');
assert.equal(artifactCandidateTotal(green), 1);
assert.equal(artifactHasNonPass(green), false);

assert.deepEqual(selectArtifactSurfaces({ jsOnly: false, dtsOnly: false }, false), {
  js: true,
  dts: false,
});
assert.deepEqual(selectArtifactSurfaces({ jsOnly: false, dtsOnly: false }, true), {
  js: true,
  dts: true,
});
assert.deepEqual(selectArtifactSurfaces({ jsOnly: true, dtsOnly: false }, true), {
  js: true,
  dts: false,
});
assert.deepEqual(selectArtifactSurfaces({ jsOnly: false, dtsOnly: true }, false), {
  js: false,
  dts: true,
});

const jsOnlySelection = selectArtifactSurfaces({ jsOnly: false, dtsOnly: false }, false);
const jsOnlyJs = artifactSurfaceObservation('complete', jsOnlySelection.js, true, true);
const jsOnlyDts = artifactSurfaceObservation('complete', jsOnlySelection.dts, true, true);
const jsOnlyJsCounts = emptyArtifactStatusCounts();
const jsOnlyDtsCounts = emptyArtifactStatusCounts();
recordArtifactStatus(jsOnlyJsCounts, jsOnlyJs.status);
recordArtifactStatus(jsOnlyDtsCounts, jsOnlyDts.status);
assert.equal(artifactCandidateTotal(jsOnlyJsCounts), 1);
assert.equal(artifactCandidateTotal(jsOnlyDtsCounts), 0, 'JS-only row stays out of DTS denominator');

const dtsOnlySelection = selectArtifactSurfaces({ jsOnly: false, dtsOnly: true }, false);
const dtsOnlyJsCounts = emptyArtifactStatusCounts();
const dtsOnlyDtsCounts = emptyArtifactStatusCounts();
recordArtifactStatus(
  dtsOnlyJsCounts,
  artifactSurfaceObservation('complete', dtsOnlySelection.js, true, true).status,
);
recordArtifactStatus(
  dtsOnlyDtsCounts,
  artifactSurfaceObservation('complete', dtsOnlySelection.dts, true, true).status,
);
assert.equal(artifactCandidateTotal(dtsOnlyJsCounts), 0);
assert.equal(artifactCandidateTotal(dtsOnlyDtsCounts), 1, 'DTS-only row stays in DTS denominator');

const bothSelection = selectArtifactSurfaces({ jsOnly: false, dtsOnly: false }, true);
const bothJsCounts = emptyArtifactStatusCounts();
const bothDtsCounts = emptyArtifactStatusCounts();
recordArtifactStatus(
  bothJsCounts,
  artifactSurfaceObservation('complete', bothSelection.js, true, true).status,
);
recordArtifactStatus(
  bothDtsCounts,
  artifactSurfaceObservation('complete', bothSelection.dts, true, true).status,
);
assert.equal(artifactCandidateTotal(bothJsCounts), 1);
assert.equal(artifactCandidateTotal(bothDtsCounts), 1);

const products = emptyArtifactProductCounts();
recordArtifactProduct(products, true, true);
recordArtifactProduct(products, true, false);
recordArtifactProduct(products, true, null);
recordArtifactProduct(products, false, false);
assert.deepEqual(products, { match: 1, mismatch: 1, unmeasured: 1 });

const vacuous: MeasuredArtifactResult = {
  artifactState: 'complete',
  jsMatch: null,
  dtsMatch: null,
};
ensureMeasuredArtifact(vacuous, { js: true, dts: false });
assert.equal(vacuous.artifactState, 'incomplete');
assert.equal(vacuous.jsMatch, false);
assert.match(vacuous.jsError!, /INCOMPLETE_CANONICAL_EMIT/);

console.log('emit-artifact-state: nonzero, crash, timeout, unsupported, and vacuous states are explicit');
