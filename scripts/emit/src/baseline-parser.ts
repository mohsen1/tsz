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
  /** All source files in this baseline (in declaration order) */
  sourceFiles: Array<{ name: string; content: string }>;
  /** Expected JavaScript output */
  js: string | null;
  /** Expected JavaScript output file name */
  jsFileName: string | null;
  /** Expected declaration output */
  dts: string | null;
  /** Expected declaration output file name */
  dtsFileName: string | null;
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
    sourceFiles: [],
    js: null,
    jsFileName: null,
    dts: null,
    dtsFileName: null,
    files: new Map(),
  };

  // Split by file markers: //// [filename]
  const fileMarkerRegex = /^\/\/\/\/ \[([^\]]+)\](?:\s*[\/]{4})?/gm;
  const segments: { name: string; markerStart: number; start: number; end: number }[] = [];

  let match: RegExpExecArray | null;
  while ((match = fileMarkerRegex.exec(content)) !== null) {
    segments.push({
      name: match[1],
      markerStart: match.index,
      start: match.index + match[0].length,
      end: content.length, // Will be updated
    });
  }

  // Update end positions
  for (let i = 0; i < segments.length - 1; i++) {
    // Next marker start marks the end of the current segment content.
    segments[i].end = segments[i + 1].markerStart;
  }

  const isSourceLike = (name: string): boolean => {
    return /\.(ts|tsx|mts|cts)$/.test(name) && !name.endsWith('.d.ts');
  };
  const isJsLikeOutput = (name: string): boolean => {
    return /\.(js|jsx|mjs|cjs)$/.test(name);
  };
  const toSourceDtsOutputName = (name: string): string => {
    return name.replace(/\.(ts|tsx|mts|cts)$/, '.d.ts');
  };
  const toJsOutputBase = (name: string): string => {
    return name.replace(/\.(js|jsx|mjs|cjs)$/, '');
  };
  const jsLikeOutputToDts = (name: string): string => {
    const stem = toJsOutputBase(name);
    return `${stem}.d.ts`;
  };

  const sourceLikeFiles: Array<{ name: string; content: string }> = [];
  const jsFileNames: Set<string> = new Set();
  const sourceFileNames: Set<string> = new Set();
  const dtsOutputCandidates: Set<string> = new Set();
  const dtsSourceFiles: Array<{ name: string; content: string }> = [];

  // First pass: collect source and js markers to infer output naming.
  for (const seg of segments) {
    const name = seg.name.trim();
    const fileContent = content.slice(seg.start, seg.end).trim();

    if (isSourceLike(name)) {
      // Some baselines include zero-length or placeholder source segments.
      // Ignore these to avoid injecting empty generated files into emitter input.
      if (fileContent.length > 0) {
        sourceLikeFiles.push({ name, content: fileContent });
      }
      sourceFileNames.add(name);
    } else if (isJsLikeOutput(name)) {
      jsFileNames.add(name);
    }
  }

  for (const sourceName of sourceLikeFiles.map((f) => f.name)) {
    dtsOutputCandidates.add(toSourceDtsOutputName(sourceName));
  }
  for (const jsName of jsFileNames) {
    dtsOutputCandidates.add(jsLikeOutputToDts(jsName));
  }
  dtsOutputCandidates.add('out.d.ts');

  // Extract content for each file
  for (const seg of segments) {
    const name = seg.name.trim();
    const fileContent = content.slice(seg.start, seg.end).trim();

    result.files.set(name, fileContent);

    // Identify file types
    if (name.startsWith('tests/cases/')) {
      result.testPath = name;
    } else if (isJsLikeOutput(name)) {
      // JavaScript output
      if (!result.js) {
        result.js = fileContent;
        result.jsFileName = name;
      }
    } else if (name.endsWith('.d.ts')) {
      // Declaration segment: classify as emitted output when name matches an emitted d.ts path.
      if (dtsOutputCandidates.has(name)) {
        if (!result.dts) {
          result.dts = fileContent;
          result.dtsFileName = name;
        }
      } else {
        dtsSourceFiles.push({ name, content: fileContent });
      }
    }
    if (isSourceLike(name)) {
      if (fileContent.length === 0) {
        continue;
      }
      result.sourceFiles.push({ name, content: fileContent });
      if (!result.source) {
        // Keep the first TypeScript source file as the default entry-point.
        result.source = fileContent;
        result.sourceFileName = name;
      }
    }
  }

  // Add declaration-only input files to the type checker input set.
  result.sourceFiles.push(...dtsSourceFiles);

  // Prefer bundled outputs when present in multifile baselines.
  if (result.files.has('out.js')) {
    result.js = result.files.get('out.js') ?? result.js;
    result.jsFileName = 'out.js';
  }
  if (result.files.has('out.d.ts')) {
    result.dts = result.files.get('out.d.ts') ?? result.dts;
    result.dtsFileName = 'out.d.ts';
  }

  // Refine JS/DTS selection by preferring files that match the source basename.
  if (result.sourceFileName) {
    const sourceBase = result.sourceFileName.replace(/\.(ts|tsx|mts|cts)$/, '');
    const sourceExt = result.sourceFileName.match(/\.(ts|tsx|mts|cts)$/)?.[1] ?? 'ts';
    const preferredJsExt =
      sourceExt === 'tsx' ? 'jsx' :
      sourceExt === 'mts' ? 'mjs' :
      sourceExt === 'cts' ? 'cjs' : 'js';
    const preferredJsName = `${sourceBase}.${preferredJsExt}`;
    const preferredDtsName = `${sourceBase}.d.ts`;

    if (!result.js && result.files.has(preferredJsName)) {
      result.js = result.files.get(preferredJsName) ?? result.js;
      result.jsFileName = preferredJsName;
    } else if (!result.js) {
      for (const [name, fileContent] of result.files) {
        if (isJsLikeOutput(name)) {
          result.js = fileContent;
          result.jsFileName = name;
          break;
        }
      }
    }

    if (!result.dts && result.files.has(preferredDtsName)) {
      result.dts = result.files.get(preferredDtsName) ?? result.dts;
      result.dtsFileName = preferredDtsName;
    } else if (!result.dts) {
      for (const [name, fileContent] of result.files) {
        if (name.endsWith('.d.ts')) {
          result.dts = fileContent;
          result.dtsFileName = name;
          break;
        }
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
import pc from 'picocolors';

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

  const lines = patch.split('\n');
  const colored: string[] = [];
  let lineCount = 0;

  for (const line of lines) {
    if (lineCount >= maxLines) {
      colored.push(pc.dim('... (truncated)'));
      break;
    }

    if (line.startsWith('+++') || line.startsWith('---')) {
      colored.push(pc.bold(line));
    } else if (line.startsWith('+')) {
      colored.push(pc.green(line));
      lineCount++;
    } else if (line.startsWith('-')) {
      colored.push(pc.red(line));
      lineCount++;
    } else if (line.startsWith('@@')) {
      colored.push(pc.cyan(line));
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
