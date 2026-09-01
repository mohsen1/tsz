import {
  compareCompilerOutcomes,
  type CompilerOutcome,
} from './canonical-products.js';

export type ArtifactState = 'complete' | 'unsupported' | 'timeout' | 'crash' | 'incomplete';
export type ArtifactStatus =
  | 'pass'
  | 'fail'
  | 'skip'
  | 'unsupported'
  | 'timeout'
  | 'crash'
  | 'incomplete';

export type ArtifactStatusCounts = Record<ArtifactStatus, number>;

export interface ArtifactSurfaceSelection {
  js: boolean;
  dts: boolean;
}

export interface ArtifactSurfaceObservation {
  selected: boolean;
  /** Full selected-surface result: invocation outcome and emitted products. */
  match: boolean | null;
  /** Raw path-to-bytes parity, independent of process completion. */
  productMatch: boolean | null;
  status: ArtifactStatus;
}

export interface ArtifactProductCounts {
  match: number;
  mismatch: number;
  unmeasured: number;
}

export interface ArtifactOutcomeComparison {
  match: boolean;
  error?: string;
}

const TSZ_SEMANTIC_INCOMPLETE_EXIT_CODE = 3;

function isCrashOutcome(outcome: CompilerOutcome): boolean {
  return outcome.exitCode < 0 ||
    outcome.exitCode > 2 ||
    (outcome.exitCode !== 0 && outcome.diagnosticCodes.length === 0);
}

function isProductCrashOutcome(outcome: CompilerOutcome): boolean {
  if (outcome.exitCode === TSZ_SEMANTIC_INCOMPLETE_EXIT_CODE) return false;
  return isCrashOutcome(outcome);
}

/**
 * A selected emit surface can pass only when the original compiler invocations
 * prove the complete outcome. Ordinary nonzero equality requires both original
 * invocations to carry the same complete ordered structured diagnostics;
 * code-only equality remains triage evidence and stays red.
 */
export function compareArtifactOutcomes(
  oracle: CompilerOutcome,
  product: CompilerOutcome,
): ArtifactOutcomeComparison {
  return compareCompilerOutcomes(oracle, product);
}

export function compilerArtifactState(
  oracle: CompilerOutcome,
  product: CompilerOutcome,
): ArtifactState {
  if (isCrashOutcome(oracle)) return 'crash';
  if (product.exitCode === TSZ_SEMANTIC_INCOMPLETE_EXIT_CODE) return 'incomplete';
  if (isProductCrashOutcome(product)) return 'crash';
  return 'complete';
}

/** Decide the requested product domain once and reuse it on every exit path. */
export function selectArtifactSurfaces(
  mode: { jsOnly: boolean; dtsOnly: boolean },
  emitsDeclarations: boolean,
): ArtifactSurfaceSelection {
  return {
    js: !mode.dtsOnly,
    dts: !mode.jsOnly && (mode.dtsOnly || emitsDeclarations),
  };
}

export function artifactSurfaceObservation(
  state: ArtifactState,
  selected: boolean,
  outcomeMatch: boolean | null,
  productMatch: boolean | null,
): ArtifactSurfaceObservation {
  if (!selected) {
    return { selected: false, match: null, productMatch: null, status: 'skip' };
  }
  const match = outcomeMatch === true && productMatch === true;
  return {
    selected: true,
    match,
    productMatch,
    status: artifactStatus(state, match),
  };
}

export interface MeasuredArtifactResult {
  artifactState: ArtifactState;
  jsMatch: boolean | null;
  dtsMatch: boolean | null;
  jsError?: string;
  dtsError?: string;
}

/** Turn an impossible all-null selected row into an explicit terminal result. */
export function ensureMeasuredArtifact(
  result: MeasuredArtifactResult,
  selected: ArtifactSurfaceSelection,
): void {
  const error = 'INCOMPLETE_CANONICAL_EMIT: selected row produced no measured surface';
  let missing = false;
  if (selected.js && result.jsMatch === null) {
    missing = true;
    result.jsMatch = false;
    result.jsError = error;
  }
  if (selected.dts && result.dtsMatch === null) {
    missing = true;
    result.dtsMatch = false;
    result.dtsError = error;
  }
  if (missing && result.artifactState === 'complete') result.artifactState = 'incomplete';
}

export function artifactStatus(state: ArtifactState, match: boolean | null): ArtifactStatus {
  if (match === null) return 'skip';
  if (state !== 'complete') return state;
  return match ? 'pass' : 'fail';
}

export function emptyArtifactStatusCounts(): ArtifactStatusCounts {
  return {
    pass: 0,
    fail: 0,
    skip: 0,
    unsupported: 0,
    timeout: 0,
    crash: 0,
    incomplete: 0,
  };
}

export function emptyArtifactProductCounts(): ArtifactProductCounts {
  return { match: 0, mismatch: 0, unmeasured: 0 };
}

export function recordArtifactStatus(
  counts: ArtifactStatusCounts,
  status: ArtifactStatus,
): void {
  counts[status]++;
}

export function recordArtifactProduct(
  counts: ArtifactProductCounts,
  selected: boolean,
  productMatch: boolean | null,
): void {
  if (!selected) return;
  if (productMatch === true) counts.match++;
  else if (productMatch === false) counts.mismatch++;
  else counts.unmeasured++;
}

/** Candidate-domain total. Only an unselected surface is omitted. */
export function artifactCandidateTotal(counts: ArtifactStatusCounts): number {
  return Object.values(counts).reduce((total, count) => total + count, 0) - counts.skip;
}

/** Fail closed without collapsing typed terminal states into product mismatches. */
export function artifactHasNonPass(counts: ArtifactStatusCounts): boolean {
  return artifactCandidateTotal(counts) !== counts.pass;
}
