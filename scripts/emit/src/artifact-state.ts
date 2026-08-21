import type { CompilerOutcome } from './canonical-products.js';

export type ArtifactState = 'complete' | 'unsupported' | 'timeout' | 'crash' | 'incomplete';
export type ArtifactStatus =
  | 'pass'
  | 'fail'
  | 'skip'
  | 'unsupported'
  | 'timeout'
  | 'crash'
  | 'incomplete';

function isCrashOutcome(outcome: CompilerOutcome): boolean {
  return outcome.exitCode < 0 ||
    outcome.exitCode > 2 ||
    (outcome.exitCode !== 0 && outcome.diagnosticCodes.length === 0);
}

export function compilerArtifactState(
  oracle: CompilerOutcome,
  product: CompilerOutcome,
): ArtifactState {
  if (oracle.exitCode === 0 && product.exitCode === 0) return 'complete';
  if (isCrashOutcome(oracle) || isCrashOutcome(product)) return 'crash';
  return 'incomplete';
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
  mode: { jsOnly: boolean; dtsOnly: boolean },
): void {
  if (result.jsMatch !== null || result.dtsMatch !== null) return;
  result.artifactState = 'incomplete';
  const error = 'INCOMPLETE_CANONICAL_EMIT: selected row produced no measured surface';
  if (mode.dtsOnly) {
    result.dtsMatch = false;
    result.dtsError = error;
  } else {
    result.jsMatch = false;
    result.jsError = error;
  }
}

export function artifactStatus(state: ArtifactState, match: boolean | null): ArtifactStatus {
  if (match === null) return 'skip';
  if (state !== 'complete') return state;
  return match ? 'pass' : 'fail';
}
