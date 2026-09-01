import type { DiagnosticWitness } from './diagnostic-witness.js';

export interface EmitProduct {
  /** Invocation-relative POSIX path. */
  path: string;
  /** One Latin-1 code unit per emitted byte; this is a lossless byte container. */
  content: string;
}

export interface CompilerOutcome {
  exitCode: number;
  diagnosticCodes: string[];
  /** Complete ordered identity from the original invocation, when provable. */
  diagnosticWitnesses?: DiagnosticWitness[];
}

export interface ProductMismatch {
  kind: 'duplicate-oracle' | 'duplicate-product' | 'missing' | 'extra' | 'content';
  path: string;
  expected?: string;
  actual?: string;
}

export interface ProductSetComparison {
  match: boolean;
  mismatches: ProductMismatch[];
}

export interface OutcomeComparison {
  match: boolean;
  error?: string;
}

function diagnosticWitnessDifference(
  oracle: unknown,
  product: unknown,
  location = 'diagnostics',
): string | undefined {
  if (Object.is(oracle, product)) return undefined;
  if (Array.isArray(oracle) || Array.isArray(product)) {
    if (!Array.isArray(oracle) || !Array.isArray(product)) return location;
    if (oracle.length !== product.length) return `${location}.length`;
    for (let index = 0; index < oracle.length; index++) {
      const difference = diagnosticWitnessDifference(oracle[index], product[index], `${location}[${index}]`);
      if (difference !== undefined) return difference;
    }
    return undefined;
  }
  if (
    typeof oracle === 'object' && oracle !== null &&
    typeof product === 'object' && product !== null
  ) {
    const oracleRecord = oracle as Record<string, unknown>;
    const productRecord = product as Record<string, unknown>;
    const oracleKeys = Object.keys(oracleRecord).sort();
    const productKeys = Object.keys(productRecord).sort();
    if (oracleKeys.join('\0') !== productKeys.join('\0')) return `${location}.keys`;
    for (const key of oracleKeys) {
      const difference = diagnosticWitnessDifference(
        oracleRecord[key],
        productRecord[key],
        `${location}.${key}`,
      );
      if (difference !== undefined) return difference;
    }
    return undefined;
  }
  return location;
}

function diagnosticCodesMatch(oracle: CompilerOutcome, product: CompilerOutcome): boolean {
  return oracle.diagnosticCodes.length === product.diagnosticCodes.length &&
    oracle.diagnosticCodes.every((code, index) => code === product.diagnosticCodes[index]);
}

/** Compare the complete TypeScript 7 and TSZ path-to-bytes product maps. */
export function compareCanonicalProductSets(
  oracle: readonly EmitProduct[],
  product: readonly EmitProduct[],
): ProductSetComparison {
  const mismatches: ProductMismatch[] = [];
  const oracleByPath = new Map<string, string>();
  for (const emitted of oracle) {
    if (oracleByPath.has(emitted.path)) {
      mismatches.push({ kind: 'duplicate-oracle', path: emitted.path });
      continue;
    }
    oracleByPath.set(emitted.path, emitted.content);
  }

  const productByPath = new Map<string, string>();
  for (const emitted of product) {
    if (productByPath.has(emitted.path)) {
      mismatches.push({ kind: 'duplicate-product', path: emitted.path });
      continue;
    }
    productByPath.set(emitted.path, emitted.content);
  }

  for (const [productPath, oracleContent] of oracleByPath) {
    const actualContent = productByPath.get(productPath);
    if (actualContent === undefined) {
      mismatches.push({ kind: 'missing', path: productPath, expected: oracleContent });
    } else if (oracleContent !== actualContent) {
      mismatches.push({
        kind: 'content',
        path: productPath,
        expected: oracleContent,
        actual: actualContent,
      });
    }
  }

  for (const [productPath, actualContent] of productByPath) {
    if (!oracleByPath.has(productPath)) {
      mismatches.push({ kind: 'extra', path: productPath, actual: actualContent });
    }
  }

  return { match: mismatches.length === 0, mismatches };
}

/**
 * A canonical pass requires successful processes, or the same ordinary
 * nonzero outcome with complete ordered structured diagnostic identity.
 */
export function compareCompilerOutcomes(
  oracle: CompilerOutcome,
  product: CompilerOutcome,
): OutcomeComparison {
  if (oracle.exitCode !== 0 && product.exitCode === 0) {
    return {
      match: false,
      error: `TYPESCRIPT_7_NONZERO_OUTCOME: exit=${oracle.exitCode}, diagnostics=${oracle.diagnosticCodes.join(',') || '<none>'}`,
    };
  }
  if (product.exitCode !== 0 && oracle.exitCode === 0) {
    return {
      match: false,
      error: `TSZ_NONZERO_OUTCOME: exit=${product.exitCode}, diagnostics=${product.diagnosticCodes.join(',') || '<none>'}`,
    };
  }
  const sameExit = oracle.exitCode === product.exitCode;
  if (!sameExit) {
    return {
      match: false,
      error: `Compiler outcome mismatch: TypeScript 7 exit=${oracle.exitCode}, TSZ exit=${product.exitCode}`,
    };
  }
  if (!diagnosticCodesMatch(oracle, product)) {
    return {
      match: false,
      error: `Compiler diagnostic mismatch: TypeScript 7=[${oracle.diagnosticCodes.join(', ')}], TSZ=[${product.diagnosticCodes.join(', ')}]`,
    };
  }
  if (oracle.exitCode !== 0 || product.exitCode !== 0) {
    if (oracle.exitCode !== 1 && oracle.exitCode !== 2) {
      return {
        match: false,
        error: `NONSTANDARD_COMPILER_OUTCOME: exit=${oracle.exitCode} is not an ordinary diagnostic exit`,
      };
    }
    if (oracle.diagnosticWitnesses === undefined || product.diagnosticWitnesses === undefined) {
      return {
        match: false,
        error: `UNVERIFIED_DIAGNOSTIC_IDENTITY: ordinary nonzero exit=${oracle.exitCode} has matching codes=[${oracle.diagnosticCodes.join(', ')}], but one or both original invocations lack complete ordered structured diagnostics`,
      };
    }
    const difference = diagnosticWitnessDifference(
      oracle.diagnosticWitnesses,
      product.diagnosticWitnesses,
    );
    if (difference !== undefined) {
      return {
        match: false,
        error: `Compiler diagnostic identity mismatch at ${difference}: codes=[${oracle.diagnosticCodes.join(', ')}]`,
      };
    }
  }
  return { match: true };
}
