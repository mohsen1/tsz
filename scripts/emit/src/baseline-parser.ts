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
  /**
   * True when the baseline indicates that emit output is intentionally absent
   * in the original (type-checked) emit (e.g., "File X missing from original
   * emit" when --noEmitOnError is set). The js/dts fields still contain the
   * noCheck emit content for comparison in JS-only mode.
   */
  noEmitExpected: boolean;
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
    noEmitExpected: false,
  };

  // Split by file markers: //// [filename]
  const fileMarkerRegex = /^\/\/\/\/ \[([^\]]+)\](?:[^\S\n\r]*[\/]{4})?/gm;
  const segments: { name: string; markerStart: number; start: number; end: number; missingFromOriginalEmit?: boolean }[] = [];

  // Detect "!!!! File X missing from original emit" markers.
  // When tsc's --noEmitOnError is set and the file has type errors, tsc produces
  // no output. The baseline annotates these with the "missing from original emit"
  // marker. We track which output files are marked this way so we can set their
  // expected content to null (no output expected).
  const missingFromOriginalEmitRegex = /^!!!! File (\S+) missing from original emit/gm;
  const missingFromOriginalEmitFiles = new Set<string>();
  let missingMatch: RegExpExecArray | null;
  while ((missingMatch = missingFromOriginalEmitRegex.exec(content)) !== null) {
    missingFromOriginalEmitFiles.add(missingMatch[1]);
  }
  if (missingFromOriginalEmitFiles.size > 0) {
    result.noEmitExpected = true;
  }

  let match: RegExpExecArray | null;
  while ((match = fileMarkerRegex.exec(content)) !== null) {
    const name = match[1];
    segments.push({
      name,
      markerStart: match.index,
      start: match.index + match[0].length,
      end: content.length, // Will be updated
      missingFromOriginalEmit: missingFromOriginalEmitFiles.has(name),
    });
  }

  // Update end positions
  for (let i = 0; i < segments.length - 1; i++) {
    // Next marker start marks the end of the current segment content.
    segments[i].end = segments[i + 1].markerStart;
  }

  const isTsSourceLike = (name: string): boolean => {
    return /\.(ts|tsx|mts|cts)$/.test(name) && !name.endsWith('.d.ts');
  };
  const isInputCodeFile = (name: string): boolean => {
    return /\.(ts|tsx|mts|cts|js|jsx|mjs|cjs|d\.ts)$/.test(name);
  };
  const isAuxiliaryFile = (name: string): boolean => {
    return name.endsWith('package.json') || name.endsWith('tsconfig.json');
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
  const outputSegments: typeof segments = [];
  const outputFileNames: Set<string> = new Set();
  const sourceFileNames: Set<string> = new Set();
  const dtsOutputCandidates: Set<string> = new Set();
  const dtsSourceFiles: Array<{ name: string; content: string }> = [];

  // Pre-pass: find the last auxiliary file (package.json, tsconfig.json).
  // In multi-file baselines with duplicate filenames (e.g., multiple index.js
  // from different subdirectories), auxiliary files serve as a reliable boundary
  // between source and output sections. tsc baselines strip directory paths
  // from filenames, so duplicate code filenames can appear in the source section.
  let lastAuxIndex = -1;
  for (let i = 0; i < segments.length; i++) {
    const name = segments[i].name.trim();
    if (name.startsWith('tests/cases/')) continue;
    if (isAuxiliaryFile(name)) {
      lastAuxIndex = i;
    }
  }

  let outputStart = segments.length;
  const seenNames = new Set<string>();
  const seenTsSources: string[] = [];
  for (let i = 0; i < segments.length; i++) {
    const name = segments[i].name.trim();
    if (name.startsWith('tests/cases/')) {
      continue;
    }
    if (name === 'out.js' || name === 'out.d.ts') {
      outputStart = Math.min(outputStart, i);
      break;
    }
    // Auxiliary files never mark output start — they can repeat in multi-dir tests.
    if (isAuxiliaryFile(name)) {
      continue;
    }
    if (seenNames.has(name)) {
      // When auxiliary files exist after this index, the duplicate is still
      // in the source section (multi-directory test with same-named files).
      if (lastAuxIndex >= i) {
        continue;
      }
      // TypeScript source files (.ts/.tsx/.mts/.cts) and declaration files (.d.ts)
      // are never output files, so duplicates are still in the source section
      // (multi-package tests with same-named files across directories).
      if (isTsSourceLike(name) || name.endsWith('.d.ts')) {
        continue;
      }
      outputStart = Math.min(outputStart, i);
      break;
    }
    if (isJsLikeOutput(name)) {
      const nameBase = toJsOutputBase(name);
      // A .js file is output only if a non-declaration TS source (.ts/.tsx)
      // exists with the same base name. If the matching source is a .d.ts,
      // the JS file is the library's runtime (e.g., tslib.d.ts + tslib.js
      // in node_modules), not compiler output.
      const isTsOutput = seenTsSources.some(src => {
        if (src.endsWith('.d.ts')) return false;
        const base = src.replace(/\.(ts|tsx|mts|cts)$/, '');
        return (
          name === `${base}.js` ||
          name === `${base}.jsx` ||
          name === `${base}.mjs` ||
          name === `${base}.cjs`
        );
      });
      // Also check if a JS/JSX source file shares the same basename
      // (e.g., foo.jsx as input -> foo.js as output in @allowJs tests).
      // Only consider valid JS-to-JS compilation pairs: .jsx -> .js.
      // Do NOT treat .cjs as output of .js (they are separate files in
      // multi-format packages like bundlerNodeModules1).
      const isJsSourceOutput = !isTsOutput && [...seenNames].some(seen => {
        if (!isJsLikeOutput(seen) || toJsOutputBase(seen) !== nameBase || seen === name) return false;
        // Only .jsx -> .js is a valid JS source output pair
        const seenExt = seen.match(/\.(js|jsx|mjs|cjs)$/)?.[1];
        const nameExt = name.match(/\.(js|jsx|mjs|cjs)$/)?.[1];
        return seenExt === 'jsx' && nameExt === 'js';
      });
      if (isTsOutput || isJsSourceOutput) {
        if (lastAuxIndex >= i) {
          continue;
        }
        outputStart = Math.min(outputStart, i);
        break;
      }
    } else if (name.endsWith('.d.ts')) {
      const isDtsOutput = seenTsSources.some(src => {
        if (src.endsWith('.d.ts')) return false;
        const base = src.replace(/\.(ts|tsx|mts|cts)$/, '');
        return name === `${base}.d.ts`;
      });
      if (isDtsOutput) {
        // Some multi-file baselines include companion declaration source files
        // (e.g. a.ts + a.d.ts) before later source files. Those .d.ts files are
        // auxiliary inputs, not the compiler output section.
        const hasLaterTsSource = segments.slice(i + 1).some(seg => {
          const laterName = seg.name.trim();
          return !laterName.startsWith('tests/cases/') && isTsSourceLike(laterName);
        });
        if (hasLaterTsSource) {
          seenNames.add(name);
          continue;
        }
        if (lastAuxIndex >= i) {
          continue;
        }
        outputStart = Math.min(outputStart, i);
        break;
      }
    }
    seenNames.add(name);
    if (isTsSourceLike(name)) {
      seenTsSources.push(name);
    }
  }

  // First pass: collect source and output markers to infer output naming.
  for (let segIndex = 0; segIndex < segments.length; segIndex++) {
    const seg = segments[segIndex];
    const name = seg.name.trim();
    const fileContent = content.slice(seg.start, seg.end).trim();

    if (name.startsWith('tests/cases/')) {
      continue;
    }

    if (segIndex < outputStart && (isInputCodeFile(name) || isAuxiliaryFile(name))) {
      // Deduplicate: for multi-directory tests with same filename
      // (e.g., subfolder/index.js and root/index.js both as [index.js]).
      // For code files, keep the first occurrence — the expected output
      // section corresponds to the first source file's compilation result.
      // For auxiliary files (package.json, tsconfig.json), keep the last
      // occurrence — in nodeModules tests, the subdirectory package.json
      // with "type": "commonjs" appears last and controls the output format.
      // Include empty source files so the CLI receives them as input
      // (they still produce output like "use strict"; in CJS mode).
      const existingIdx = sourceLikeFiles.findIndex(f => f.name === name);
      if (existingIdx >= 0) {
        if (isAuxiliaryFile(name)) {
          sourceLikeFiles[existingIdx] = { name, content: fileContent };
        }
        // For code files, skip — keep the first occurrence.
      } else {
        sourceLikeFiles.push({ name, content: fileContent });
      }
      sourceFileNames.add(name);
    } else if (segIndex >= outputStart) {
      outputSegments.push(seg);
      outputFileNames.add(name);
    }
  }

  for (const sourceName of sourceLikeFiles.map((f) => f.name)) {
    dtsOutputCandidates.add(toSourceDtsOutputName(sourceName));
  }
  for (const seg of outputSegments) {
    const jsName = seg.name.trim();
    if (!isJsLikeOutput(jsName)) continue;
    dtsOutputCandidates.add(jsLikeOutputToDts(jsName));
  }
  dtsOutputCandidates.add('out.d.ts');

  // Extract content for each file
  for (let segIndex = 0; segIndex < segments.length; segIndex++) {
    const seg = segments[segIndex];
    const name = seg.name.trim();
    const fileContent = content.slice(seg.start, seg.end).trim();

    result.files.set(name, fileContent);

    // Identify file types
    if (name.startsWith('tests/cases/')) {
      result.testPath = name;
      continue;
    }

    if (segIndex < outputStart && (isInputCodeFile(name) || isAuxiliaryFile(name))) {
      // Deduplicate: first occurrence for code files, last for auxiliary files.
      const existingResIdx = result.sourceFiles.findIndex(f => f.name === name);
      if (existingResIdx >= 0) {
        if (isAuxiliaryFile(name)) {
          result.sourceFiles[existingResIdx] = { name, content: fileContent };
        }
      } else {
        result.sourceFiles.push({ name, content: fileContent });
      }
      if (!result.source && fileContent.length > 0 && !name.endsWith('.d.ts') && !isAuxiliaryFile(name)) {
        // Keep the first non-empty, non-declaration source file as entry-point.
        result.source = fileContent;
        result.sourceFileName = name;
      }
      continue;
    }

    if (segIndex >= outputStart && isJsLikeOutput(name)) {
      // JavaScript output.
      // When stripped-path baselines contain duplicate output filenames
      // (for example multiple `index.js` outputs from different directories),
      // keep the later segment for that filename to stay aligned with the
      // `files` map, which also preserves the last occurrence.
      //
      // Files marked "missing from original emit" (e.g., --noEmitOnError with
      // type errors) are not expected to be produced by the compiler, so skip them.
      if (!result.js || result.jsFileName === name) {
        result.js = fileContent;
        result.jsFileName = name;
      }
    } else if (segIndex >= outputStart && name.endsWith('.d.ts')) {
      // Declaration segment: classify as emitted output when name matches an emitted d.ts path.
      if (dtsOutputCandidates.has(name)) {
        if (!result.dts || result.dtsFileName === name) {
          result.dts = fileContent;
          result.dtsFileName = name;
        }
      } else {
        dtsSourceFiles.push({ name, content: fileContent });
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
  if (outputFileNames.has('out.d.ts') && result.files.has('out.d.ts')) {
    result.dts = result.files.get('out.d.ts') ?? result.dts;
    result.dtsFileName = 'out.d.ts';
  }

  // Refine JS/DTS selection by preferring files that match the source basename.
  if (result.sourceFileName) {
    const sourceBase = result.sourceFileName.replace(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/, '');
    const sourceExt = result.sourceFileName.match(/\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/)?.[1] ?? 'ts';
    const preferredJsExt =
      sourceExt === 'tsx' || sourceExt === 'jsx' ? 'jsx' :
      sourceExt === 'mts' || sourceExt === 'mjs' ? 'mjs' :
      sourceExt === 'cts' || sourceExt === 'cjs' ? 'cjs' : 'js';
    const preferredJsName = `${sourceBase}.${preferredJsExt}`;
    const preferredDtsName = `${sourceBase}.d.ts`;

    if (!result.js && outputFileNames.has(preferredJsName) && result.files.has(preferredJsName)) {
      result.js = result.files.get(preferredJsName) ?? result.js;
      result.jsFileName = preferredJsName;
    } else if (!result.js) {
      for (const [name, fileContent] of result.files) {
        if (outputFileNames.has(name) && isJsLikeOutput(name)) {
          result.js = fileContent;
          result.jsFileName = name;
          break;
        }
      }
    }

    if (!result.dts && outputFileNames.has(preferredDtsName) && result.files.has(preferredDtsName)) {
      result.dts = result.files.get(preferredDtsName) ?? result.dts;
      result.dtsFileName = preferredDtsName;
    } else if (!result.dts) {
      for (const [name, fileContent] of result.files) {
        if (outputFileNames.has(name) && name.endsWith('.d.ts')) {
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
