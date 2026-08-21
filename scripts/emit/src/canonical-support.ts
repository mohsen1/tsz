export interface CanonicalSupportInput {
  parserHasSource: boolean;
  parserHasJs: boolean;
  parserHasDts: boolean;
  dtsOnly: boolean;
  sourceReadFailed: boolean;
  target: number;
  module: number;
  moduleResolution: string;
  alwaysStrict?: boolean;
  hasDownlevelIterationOption: boolean;
  esModuleInteropDisabled: boolean;
  hasOutFile: boolean;
  hasBaseUrl: boolean;
  hasMapOption: boolean;
  hasBaselineSidecar: boolean;
}

/**
 * Name every canonical configuration the current external-process harness
 * cannot compare exactly. Discovery retains these rows; execution reports a
 * terminal non-pass with these reasons instead of silently removing them from
 * the denominator.
 */
export function canonicalUnsupportedReasons(input: CanonicalSupportInput): string[] {
  const reasons: string[] = [];
  if (!input.parserHasSource) reasons.push('baseline-parser-missing-source');
  if (!input.parserHasJs) reasons.push('baseline-parser-missing-js-products');
  if (input.dtsOnly && !input.parserHasDts) reasons.push('dts-only-baseline-missing-dts-products');
  if (input.sourceReadFailed) reasons.push('source-read-failed');
  if (input.target <= 1) reasons.push('target-below-es2015');
  if ([0, 2, 3, 4].includes(input.module)) reasons.push(`module-kind-${input.module}`);
  if (['classic', 'node', 'node10'].includes(input.moduleResolution)) {
    reasons.push(`module-resolution-${input.moduleResolution}`);
  }
  if (input.alwaysStrict === false) reasons.push('always-strict-false');
  if (input.hasDownlevelIterationOption) reasons.push('downlevel-iteration-option');
  if (input.esModuleInteropDisabled) reasons.push('es-module-interop-false');
  if (input.hasOutFile) reasons.push('out-file-product-layout');
  if (input.hasBaseUrl) reasons.push('base-url-invocation');
  if (input.hasMapOption || input.hasBaselineSidecar) {
    reasons.push('source-map-products-not-compared');
  }
  return reasons;
}

/** Cap only after the canonical candidate domain is chosen. No parse or
 * capability result is allowed to erase a candidate from the inventory. */
export function retainCanonicalInventory<T>(candidates: readonly T[], max: number): T[] {
  return max < Infinity ? candidates.slice(0, max) : [...candidates];
}

export function hasEmitSidecar(baselineFile: string, entries: ReadonlySet<string>): boolean {
  const sourcemapText = baselineFile.replace(/\.js$/, '.sourcemap.txt');
  return entries.has(`${baselineFile}.map`) || entries.has(sourcemapText);
}
