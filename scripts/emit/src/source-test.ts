import * as fs from 'node:fs';
import * as path from 'node:path';
import { parseDirectiveLine, parseFlagDirectiveLine } from './directives.js';
import type { LinkInput } from './cli-transpiler.js';

export interface ParsedSourceTest {
  options: Record<string, unknown>;
  source: string | null;
  sourceFileName: string | null;
  sourceFiles: Array<{ name: string; content: string }>;
  links: LinkInput[];
}

export interface HarnessRootSelection {
  rootFileNames: string[];
  reason: 'all-units' | 'last-unit-no-implicit-references' | 'last-unit-discovery';
  unsupportedReason?: string;
}

interface ExactLine {
  body: string;
  bytes: string;
}

function exactLines(content: string): ExactLine[] {
  const lines: ExactLine[] = [];
  let start = 0;
  const ending = /\r\n|\r|\n/g;
  let match: RegExpExecArray | null;
  while ((match = ending.exec(content)) !== null) {
    lines.push({ body: content.slice(start, match.index), bytes: content.slice(start, ending.lastIndex) });
    start = ending.lastIndex;
  }
  if (start < content.length) {
    lines.push({ body: content.slice(start), bytes: content.slice(start) });
  }
  return lines;
}

/**
 * Decode a checked-in TypeScript harness test. No historical revision or
 * alternate source is elected when the live corpus path cannot be read.
 */
export async function readTypeScriptTestFile(typeScriptRoot: string, testPath: string): Promise<string> {
  const bytes = await fs.promises.readFile(path.join(typeScriptRoot, testPath));
  return decodeTypeScriptTestFile(bytes);
}

export function decodeTypeScriptTestFile(bytes: Buffer): string {
  if (bytes.length >= 2) {
    if (bytes[0] === 0xff && bytes[1] === 0xfe) {
      return bytes.subarray(2).toString('utf16le');
    }
    if (bytes[0] === 0xfe && bytes[1] === 0xff) {
      let text = '';
      for (let index = 2; index + 1 < bytes.length; index += 2) {
        text += String.fromCharCode((bytes[index] << 8) | bytes[index + 1]);
      }
      return text;
    }
  }
  return bytes.toString('utf8');
}

/**
 * Mirror compilerRunner's staged-file/root split. The last harness unit alone
 * is a root for noImplicitReferences or discovery-bearing last units; every
 * other unit remains staged for resolution. Embedded tsconfig root expansion
 * is deliberately quarantined until its fileNames algorithm is modeled.
 */
export function selectHarnessRootFiles(
  sourceFiles: ReadonlyArray<{ name: string; content: string }>,
  noImplicitReferences: boolean | undefined,
): HarnessRootSelection {
  const configNames = sourceFiles
    .filter(file => file.name.toLowerCase().endsWith('tsconfig.json'))
    .map(file => file.name);
  const harnessUnits = sourceFiles.filter(file => !file.name.toLowerCase().endsWith('tsconfig.json'));
  const rootable = (name: string): boolean => !name.toLowerCase().endsWith('.json');
  const seenNames = new Set<string>();
  const duplicateName = sourceFiles.find(file => {
    if (seenNames.has(file.name)) return true;
    seenNames.add(file.name);
    return false;
  })?.name;
  if (duplicateName) {
    return {
      rootFileNames: [],
      reason: 'all-units',
      unsupportedReason: `duplicate-harness-unit-name:${duplicateName}`,
    };
  }
  if (configNames.length > 0) {
    return {
      rootFileNames: harnessUnits.filter(file => rootable(file.name)).map(file => file.name),
      reason: 'all-units',
      unsupportedReason: `embedded-tsconfig-root-selection-not-modeled:${configNames.join('|')}`,
    };
  }

  const lastUnit = harnessUnits.at(-1);
  const lastUnitDiscovery = lastUnit !== undefined &&
    (/require\(/.test(lastUnit.content) || /reference\spath/.test(lastUnit.content));
  const selectLast = noImplicitReferences === true || lastUnitDiscovery;
  if (selectLast && lastUnit) {
    if (!rootable(lastUnit.name)) {
      return {
        rootFileNames: [],
        reason: noImplicitReferences === true
          ? 'last-unit-no-implicit-references'
          : 'last-unit-discovery',
        unsupportedReason: `last-harness-root-is-json:${lastUnit.name}`,
      };
    }
    return {
      rootFileNames: [lastUnit.name],
      reason: noImplicitReferences === true
        ? 'last-unit-no-implicit-references'
        : 'last-unit-discovery',
    };
  }

  const rootFileNames = harnessUnits.filter(file => rootable(file.name)).map(file => file.name);
  return {
    rootFileNames,
    reason: 'all-units',
    ...(rootFileNames.length === 0 ? { unsupportedReason: 'no-compilable-harness-roots' } : {}),
  };
}

/** Parse harness structure while preserving every source byte that remains. */
export function parseSourceTest(content: string, defaultSourceFileName?: string): ParsedSourceTest {
  const options: Record<string, unknown> = {};
  const sourceFiles: Array<{ name: string; content: string }> = [];
  const links: LinkInput[] = [];
  const stripped = content.startsWith('\uFEFF') ? content.slice(1) : content;
  const lines = exactLines(stripped);
  let currentFileName: string | null = null;
  let currentContent: string[] = [];

  const flushCurrentFile = (): void => {
    if (!currentFileName) return;
    sourceFiles.push({ name: currentFileName, content: currentContent.join('') });
    currentFileName = null;
    currentContent = [];
  };

  for (const line of lines) {
    const directive = parseDirectiveLine(line.body);
    if (directive) {
      const { key: lowKey, value } = directive;
      if (lowKey === 'filename') {
        flushCurrentFile();
        currentFileName = value;
        continue;
      }
      if (lowKey === 'link') {
        const [target, link] = value.split('->').map(part => part.trim());
        if (target && link) links.push({ target, link });
        continue;
      }
      if (value.toLowerCase() === 'true') options[lowKey] = true;
      else if (value.toLowerCase() === 'false') options[lowKey] = false;
      else options[lowKey] = value;
      continue;
    }

    const flagDirective = parseFlagDirectiveLine(line.body);
    if (flagDirective) {
      const lowKey = flagDirective.toLowerCase();
      if (lowKey === 'ts-check') options.checkjs = true;
      else if (lowKey === 'ts-nocheck') options.checkjs = false;
      else if (!currentFileName && lowKey !== 'internal') options[lowKey] = true;
      if (currentFileName) currentContent.push(line.bytes);
      continue;
    }
    if (currentFileName) currentContent.push(line.bytes);
  }
  flushCurrentFile();

  if (sourceFiles.length === 0 && defaultSourceFileName) {
    // The leading harness header is the only removed source container region.
    // Blank lines and all bytes after the first real source line remain exact.
    const directiveShape = /^\/\/\s*@[\w-]+(?:\s*:\s*[^\r\n]*)?$/i;
    const sourceBytes: string[] = [];
    let inHeader = true;
    for (const line of lines) {
      const trimmed = line.body.trim();
      if (inHeader) {
        const sourceComment = /^\/\/\s*@(internal|ts-check|ts-nocheck)\s*$/i.test(trimmed);
        if (directiveShape.test(trimmed) && !sourceComment) continue;
        if (trimmed === '') {
          sourceBytes.push(line.bytes);
          continue;
        }
        inHeader = false;
      }
      sourceBytes.push(line.bytes);
    }
    sourceFiles.push({ name: defaultSourceFileName, content: sourceBytes.join('') });
  }

  const isEntryCandidate = (file: { name: string; content: string }): boolean => (
    file.content.length > 0 &&
    !file.name.endsWith('.d.ts') &&
    !file.name.endsWith('package.json') &&
    !file.name.endsWith('tsconfig.json')
  );
  const entrySourceFile = sourceFiles.find(isEntryCandidate);

  return {
    options,
    source: entrySourceFile?.content ?? null,
    sourceFileName: entrySourceFile?.name ?? null,
    sourceFiles,
    links,
  };
}
