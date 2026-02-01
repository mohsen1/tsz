/**
 * TypeScript Baseline Parser
 *
 * Parses TypeScript's emit baseline files which contain:
 * - Source TypeScript code
 * - Expected JavaScript output
 * - Expected Declaration output (.d.ts)
 *
 * Format:
 *   //// [tests/cases/path/to/test.ts] ////
 *
 *   //// [test.ts]
 *   (source code)
 *
 *   //// [test.js]
 *   (JavaScript output)
 *
 *   //// [test.d.ts]
 *   (Declaration output)
 */

export interface BaselineContent {
  /** Path to the test file */
  testPath: string | null;
  /** Source TypeScript code */
  source: string | null;
  /** Source file name */
  sourceFileName: string | null;
  /** Expected JavaScript output */
  js: string | null;
  /** Expected declaration output */
  dts: string | null;
  /** All files in the baseline */
  files: Map<string, string>;
}

/**
 * Parse a TypeScript baseline file
 */
export function parseBaseline(content: string): BaselineContent {
  const result: BaselineContent = {
    testPath: null,
    source: null,
    sourceFileName: null,
    js: null,
    dts: null,
    files: new Map(),
  };

  // Split by file markers: //// [filename]
  const fileMarkerRegex = /^\/\/\/\/ \[([^\]]+)\]/gm;
  const segments: { name: string; start: number; end: number }[] = [];

  let match: RegExpExecArray | null;
  while ((match = fileMarkerRegex.exec(content)) !== null) {
    segments.push({
      name: match[1],
      start: match.index + match[0].length,
      end: content.length, // Will be updated
    });
  }

  // Update end positions
  for (let i = 0; i < segments.length - 1; i++) {
    segments[i].end = segments[i + 1].start - segments[i + 1].name.length - 7; // 7 = "//// [".length (6) + "]".length (1)
  }

  // Extract content for each file
  for (const seg of segments) {
    const name = seg.name.trim();
    const fileContent = content.slice(seg.start, seg.end).trim();

    result.files.set(name, fileContent);

    // Identify file types
    if (name.startsWith('tests/cases/')) {
      result.testPath = name;
    } else if (name.endsWith('.ts') && !name.endsWith('.d.ts')) {
      // Source TypeScript file
      if (!result.source) {
        result.source = fileContent;
        result.sourceFileName = name;
      }
    } else if (name.endsWith('.js')) {
      // JavaScript output
      if (!result.js) {
        result.js = fileContent;
      }
    } else if (name.endsWith('.d.ts')) {
      // Declaration output
      if (!result.dts) {
        result.dts = fileContent;
      }
    }
  }

  return result;
}

/**
 * Prepare emit output for comparison.
 * Only normalizes line endings for cross-platform consistency.
 * NO other normalization - tsz output should match tsc exactly.
 */
export function normalizeEmit(code: string): string {
  return code
    // Normalize line endings only
    .replace(/\r\n/g, '\n')
    .trim();
}

/**
 * Compare two emit outputs with normalization
 */
export function compareEmit(expected: string, actual: string): boolean {
  return normalizeEmit(expected) === normalizeEmit(actual);
}

import * as Diff from 'diff';

/**
 * Get a pretty-printed unified diff between expected and actual emit
 */
export function getEmitDiff(expected: string, actual: string, maxLines: number = 30): string {
  const normExpected = normalizeEmit(expected);
  const normActual = normalizeEmit(actual);

  if (normExpected === normActual) {
    return '';
  }

  const patch = Diff.createPatch('output', normExpected, normActual, 'expected', 'actual');
  
  // Color the diff output
  const lines = patch.split('\n');
  const colored: string[] = [];
  let lineCount = 0;
  
  for (const line of lines) {
    if (lineCount >= maxLines) {
      colored.push('\x1b[2m... (truncated)\x1b[0m');
      break;
    }
    
    if (line.startsWith('+++') || line.startsWith('---')) {
      colored.push(`\x1b[1m${line}\x1b[0m`);
    } else if (line.startsWith('+')) {
      colored.push(`\x1b[32m${line}\x1b[0m`);
      lineCount++;
    } else if (line.startsWith('-')) {
      colored.push(`\x1b[31m${line}\x1b[0m`);
      lineCount++;
    } else if (line.startsWith('@@')) {
      colored.push(`\x1b[36m${line}\x1b[0m`);
    } else {
      colored.push(line);
    }
  }
  
  return colored.join('\n');
}

/**
 * Get a summary of differences (for non-verbose mode)
 */
export function getEmitDiffSummary(expected: string, actual: string): string {
  const normExpected = normalizeEmit(expected);
  const normActual = normalizeEmit(actual);

  if (normExpected === normActual) {
    return '';
  }

  const changes = Diff.diffLines(normExpected, normActual);
  let added = 0, removed = 0;
  
  for (const change of changes) {
    if (change.added) added += change.count ?? 0;
    if (change.removed) removed += change.count ?? 0;
  }

  return `+${added}/-${removed} lines`;
}
