/**
 * Shared TypeScript target/module enum conversions.
 *
 * `runner.ts` parses compiler-option strings from baseline harness comments,
 * while `cli-transpiler.ts` renders numeric enum values back into CLI flags.
 * Both files previously carried their own copies of the same mapping tables,
 * so a new target/module value had to be added in two places and could drift.
 *
 * The parsers accept the same inputs as the historical inline copies:
 * - `parseTarget` uses `includes` for ES-year tokens (so strings like
 *   `"es2015-classes"` still resolve to ES2015) plus an explicit `=== 'es6'`
 *   check to avoid spurious matches from long identifiers that happen to
 *   contain the substring `"es6"`.
 * - `parseModule` uses strict equality, matching the narrower `"commonjs"` /
 *   `"node16"` token space that `module` values come from.
 */
export const TS_TARGET_DEFAULT = 12; // ES2025 — TS6 default
export const TS_MODULE_DEFAULT = 0; // none

const TARGET_CLI_ARGS: Record<number, string> = {
  0: 'es3',
  1: 'es5',
  2: 'es2015',
  3: 'es2016',
  4: 'es2017',
  5: 'es2018',
  6: 'es2019',
  7: 'es2020',
  8: 'es2021',
  9: 'es2022',
  10: 'es2023',
  11: 'es2024',
  12: 'es2025',
  99: 'esnext',
};

const MODULE_CLI_ARGS: Record<number, string> = {
  0: 'none',
  1: 'commonjs',
  2: 'amd',
  3: 'umd',
  4: 'system',
  5: 'es2015',
  6: 'es2020',
  7: 'es2022',
  99: 'esnext',
  100: 'node16',
  101: 'node18',
  102: 'node20',
  199: 'nodenext',
  200: 'preserve',
};

export function parseTarget(targetStr: string): number {
  const lower = targetStr.toLowerCase();
  if (lower.includes('es3')) return 0;
  if (lower.includes('es5')) return 1;
  if (lower.includes('es2015') || lower === 'es6') return 2;
  if (lower.includes('es2016')) return 3;
  if (lower.includes('es2017')) return 4;
  if (lower.includes('es2018')) return 5;
  if (lower.includes('es2019')) return 6;
  if (lower.includes('es2020')) return 7;
  if (lower.includes('es2021')) return 8;
  if (lower.includes('es2022')) return 9;
  if (lower.includes('es2023')) return 10;
  if (lower.includes('es2024')) return 11;
  if (lower.includes('es2025')) return 12;
  if (lower.includes('esnext')) return 99;
  return TS_TARGET_DEFAULT;
}

export function parseModule(moduleStr: string): number {
  const lower = moduleStr.toLowerCase();
  if (lower === 'none') return 0;
  if (lower === 'commonjs') return 1;
  if (lower === 'amd') return 2;
  if (lower === 'umd') return 3;
  if (lower === 'system') return 4;
  if (lower === 'es2015' || lower === 'es6') return 5;
  if (lower === 'es2020') return 6;
  if (lower === 'es2022') return 7;
  if (lower === 'esnext') return 99;
  if (lower === 'node16') return 100;
  if (lower === 'node18') return 101;
  if (lower === 'node20') return 102;
  if (lower === 'nodenext') return 199;
  if (lower === 'preserve') return 200;
  return TS_MODULE_DEFAULT;
}

export function targetToCliArg(target: number): string {
  return TARGET_CLI_ARGS[target] ?? 'esnext';
}

export function moduleToCliArg(module: number): string {
  return MODULE_CLI_ARGS[module] ?? 'none';
}

/**
 * Infer default module kind from target, matching TS6's computed module defaults:
 * - ESNext (99) → ESNext module (99)
 * - >= ES2022 (9) → ES2022 module (7)
 * - >= ES2020 (7) → ES2020 module (6)
 * - >= ES2015 (2) → ES2015 module (5)
 * - else → CommonJS (1)
 */
export function inferDefaultModule(target: number): number {
  if (target === 99) return 99;
  if (target >= 9) return 7;
  if (target >= 7) return 6;
  if (target >= 2) return 5;
  return 1;
}
