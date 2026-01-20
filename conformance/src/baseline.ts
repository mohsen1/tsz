/**
 * Baseline File Loading and Comparison
 * 
 * Loads and parses TypeScript baseline files (.errors.txt, .types, .symbols)
 * for comparison with our compiler's output.
 */

import * as fs from 'fs';
import * as path from 'path';

/**
 * A parsed error from a .errors.txt baseline file
 */
export interface BaselineError {
  file: string;
  line: number;
  column: number;
  code: number;
  message: string;
}

/**
 * Result of loading a baseline
 */
export interface BaselineResult {
  exists: boolean;
  errors: BaselineError[];
  raw?: string;
}

/**
 * Comparison between actual and expected errors
 */
export interface BaselineComparison {
  hasBaseline: boolean;
  expectedErrors: BaselineError[];
  actualCodes: number[];
  exactMatch: boolean;
  matchingCodes: number[];
  missingCodes: number[];
  extraCodes: number[];
  matchRate: number;
}

/**
 * Parse errors from a .errors.txt baseline file
 */
export function parseErrorsBaseline(content: string): BaselineError[] {
  const errors: BaselineError[] = [];
  const headerRegex = /^(.+?)\((\d+),(\d+)\): error TS(\d+): (.+)$/gm;
  let match;
  
  while ((match = headerRegex.exec(content)) !== null) {
    errors.push({
      file: match[1],
      line: parseInt(match[2], 10),
      column: parseInt(match[3], 10),
      code: parseInt(match[4], 10),
      message: match[5].trim(),
    });
  }
  
  return errors;
}

/**
 * Get the baseline path for a test file
 */
export function getBaselinePath(
  testPath: string, 
  testsBasePath: string,
  baselineType: 'errors.txt' | 'types' | 'symbols' | 'js' | 'd.ts' = 'errors.txt'
): string {
  const baseName = path.basename(testPath, '.ts');
  const baselineDir = path.resolve(testsBasePath, '../../baselines/reference');
  return path.join(baselineDir, `${baseName}.${baselineType}`);
}

/**
 * Load baseline for a test file
 */
export function loadBaseline(testPath: string, testsBasePath: string): BaselineResult {
  const baselinePath = getBaselinePath(testPath, testsBasePath);
  
  if (!fs.existsSync(baselinePath)) {
    return { exists: false, errors: [] };
  }
  
  try {
    const content = fs.readFileSync(baselinePath, 'utf8');
    return {
      exists: true,
      errors: parseErrorsBaseline(content),
      raw: content,
    };
  } catch {
    return { exists: false, errors: [] };
  }
}

/**
 * Compare actual diagnostic codes with baseline
 */
export function compareWithBaseline(
  actualCodes: number[],
  baseline: BaselineResult
): BaselineComparison {
  const expectedCodes = baseline.errors.map(e => e.code);
  
  const expectedFreq = new Map<number, number>();
  for (const code of expectedCodes) {
    expectedFreq.set(code, (expectedFreq.get(code) || 0) + 1);
  }
  
  const actualFreq = new Map<number, number>();
  for (const code of actualCodes) {
    actualFreq.set(code, (actualFreq.get(code) || 0) + 1);
  }
  
  const matchingCodes: number[] = [];
  const missingCodes: number[] = [];
  const extraCodes: number[] = [];
  
  for (const [code, expectedCount] of expectedFreq) {
    const actualCount = actualFreq.get(code) || 0;
    const matchCount = Math.min(expectedCount, actualCount);
    
    for (let i = 0; i < matchCount; i++) matchingCodes.push(code);
    for (let i = 0; i < expectedCount - matchCount; i++) missingCodes.push(code);
  }
  
  for (const [code, actualCount] of actualFreq) {
    const expectedCount = expectedFreq.get(code) || 0;
    for (let i = 0; i < actualCount - expectedCount; i++) extraCodes.push(code);
  }
  
  const totalExpected = expectedCodes.length;
  const exactMatch = missingCodes.length === 0 && extraCodes.length === 0;
  const matchRate = totalExpected > 0 
    ? matchingCodes.length / totalExpected 
    : (actualCodes.length === 0 ? 1.0 : 0.0);
  
  return {
    hasBaseline: baseline.exists,
    expectedErrors: baseline.errors,
    actualCodes,
    exactMatch,
    matchingCodes,
    missingCodes,
    extraCodes,
    matchRate,
  };
}

/**
 * Format baseline comparison for display
 */
export function formatBaselineComparison(comparison: BaselineComparison): string {
  if (comparison.exactMatch) {
    return `Exact match (${comparison.expectedErrors.length} errors)`;
  }
  
  const parts: string[] = [];
  
  if (!comparison.hasBaseline && comparison.actualCodes.length > 0) {
    parts.push(`Expected 0 errors, got ${comparison.actualCodes.length}`);
  } else {
    if (comparison.missingCodes.length > 0) {
      const uniqueMissing = [...new Set(comparison.missingCodes)];
      parts.push(`Missing: TS${uniqueMissing.join(', TS')}`);
    }
    if (comparison.extraCodes.length > 0) {
      const uniqueExtra = [...new Set(comparison.extraCodes)];
      parts.push(`Extra: TS${uniqueExtra.join(', TS')}`);
    }
  }
  
  return parts.join(' | ');
}
