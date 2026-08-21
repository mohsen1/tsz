export interface EmitProduct {
  /** Invocation-relative POSIX path. */
  path: string;
  /** One Latin-1 code unit per emitted byte; this is a lossless byte container. */
  content: string;
}

export interface CompilerOutcome {
  exitCode: number;
  diagnosticCodes: string[];
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

/** A canonical pass requires two successful compiler processes. */
export function compareCompilerOutcomes(
  oracle: CompilerOutcome,
  product: CompilerOutcome,
): OutcomeComparison {
  if (oracle.exitCode !== 0) {
    return {
      match: false,
      error: `TYPESCRIPT_7_NONZERO_OUTCOME: exit=${oracle.exitCode}, diagnostics=${oracle.diagnosticCodes.join(',') || '<none>'}`,
    };
  }
  if (product.exitCode !== 0) {
    return {
      match: false,
      error: `TSZ_NONZERO_OUTCOME: exit=${product.exitCode}, diagnostics=${product.diagnosticCodes.join(',') || '<none>'}`,
    };
  }
  if (
    oracle.diagnosticCodes.length !== product.diagnosticCodes.length ||
    oracle.diagnosticCodes.some((code, index) => code !== product.diagnosticCodes[index])
  ) {
    return {
      match: false,
      error: `Compiler diagnostic mismatch: TypeScript 7=[${oracle.diagnosticCodes.join(', ')}], TSZ=[${product.diagnosticCodes.join(', ')}]`,
    };
  }
  return { match: true };
}
