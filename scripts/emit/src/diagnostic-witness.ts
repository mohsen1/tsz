import * as path from 'node:path';
import type { Diagnostic as TypeScriptDiagnostic } from 'typescript/unstable/sync';

export type DiagnosticCategoryWitness = 'warning' | 'error' | 'suggestion' | 'message';

export interface DiagnosticMessageWitness {
  text: string;
  category: DiagnosticCategoryWitness;
  code: string;
  next: DiagnosticMessageWitness[];
}

export interface DiagnosticWitness {
  /** Invocation-relative POSIX path, or null for a global diagnostic. */
  path: string | null;
  /** UTF-16 offset and length, or null for a global diagnostic. */
  start: number | null;
  length: number | null;
  category: DiagnosticCategoryWitness;
  code: string;
  text: string;
  messageChain: DiagnosticMessageWitness[];
  relatedInformation: DiagnosticWitness[];
}

export interface DiagnosticNormalizationScope {
  invocationDirectory: string;
  scopeDirectory: string;
  forbiddenFile?: string;
}

interface TszRelatedInformationJson {
  file: string;
  start: number;
  length: number;
  message_text: string;
  code: number;
  depth: number;
}

interface TszDiagnosticJson {
  file: string;
  start: number;
  length: number;
  message_text: string;
  category: DiagnosticCategoryWitness;
  code: number;
  related_information: TszRelatedInformationJson[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOnlyKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const allowed = new Set(keys);
  return Object.keys(value).every(key => allowed.has(key));
}

function isNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0;
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0;
}

function isCategory(value: unknown): value is DiagnosticCategoryWitness {
  return value === 'warning' || value === 'error' || value === 'suggestion' || value === 'message';
}

function normalizeLineEndings(value: string): string {
  return value.replace(/\r\n?/g, '\n');
}

function replaceAllLiteral(value: string, search: string, replacement: string): string {
  return search.length === 0 ? value : value.split(search).join(replacement);
}

/** Remove per-process staging roots while preserving every other message byte. */
export function normalizeDiagnosticText(value: string, scopeDirectory: string): string {
  const normalizedScope = path.resolve(scopeDirectory);
  const native = normalizeLineEndings(value);
  const withNativeScope = replaceAllLiteral(native, normalizedScope, '<invocation-scope>');
  return replaceAllLiteral(
    withNativeScope,
    normalizedScope.replace(/\\/g, '/'),
    '<invocation-scope>',
  );
}

function resolvedDiagnosticFile(fileName: string, invocationDirectory: string): string {
  return path.isAbsolute(fileName)
    ? path.resolve(fileName)
    : path.resolve(invocationDirectory, fileName);
}

function normalizeDiagnosticPath(
  fileName: string | undefined,
  scope: DiagnosticNormalizationScope,
): string | null | undefined {
  if (fileName === undefined || fileName === '') return null;
  const resolved = resolvedDiagnosticFile(fileName, scope.invocationDirectory);
  if (
    scope.forbiddenFile !== undefined &&
    resolved === path.resolve(scope.forbiddenFile)
  ) {
    return undefined;
  }
  return path.relative(scope.invocationDirectory, resolved).split(path.sep).join('/');
}

function numericCode(code: unknown): string | undefined {
  return isNonNegativeInteger(code) ? `TS${code}` : undefined;
}

function parseTszRelated(value: unknown): TszRelatedInformationJson | undefined {
  if (!isRecord(value)) return undefined;
  const code = numericCode(value.code);
  if (
    !hasOnlyKeys(value, ['file', 'start', 'length', 'message_text', 'code', 'depth']) ||
    typeof value.file !== 'string' ||
    !isNonNegativeInteger(value.start) ||
    !isNonNegativeInteger(value.length) ||
    typeof value.message_text !== 'string' ||
    code === undefined ||
    !isPositiveInteger(value.depth)
  ) {
    return undefined;
  }
  return {
    file: value.file,
    start: value.start,
    length: value.length,
    message_text: value.message_text,
    code: value.code as number,
    depth: value.depth,
  };
}

function parseTszDiagnostic(value: unknown): TszDiagnosticJson | undefined {
  if (!isRecord(value)) return undefined;
  const code = numericCode(value.code);
  if (
    !hasOnlyKeys(value, [
      'file',
      'start',
      'length',
      'message_text',
      'category',
      'code',
      'related_information',
    ]) ||
    typeof value.file !== 'string' ||
    !isNonNegativeInteger(value.start) ||
    !isNonNegativeInteger(value.length) ||
    typeof value.message_text !== 'string' ||
    !isCategory(value.category) ||
    code === undefined ||
    (value.related_information !== undefined && !Array.isArray(value.related_information))
  ) {
    return undefined;
  }
  const related = (value.related_information ?? []).map(parseTszRelated);
  if (related.some(item => item === undefined)) return undefined;
  return {
    file: value.file,
    start: value.start,
    length: value.length,
    message_text: value.message_text,
    category: value.category,
    code: value.code as number,
    related_information: related as TszRelatedInformationJson[],
  };
}

function tszMessageChain(
  entries: readonly TszRelatedInformationJson[],
  scope: DiagnosticNormalizationScope,
  category: DiagnosticCategoryWitness,
): DiagnosticMessageWitness[] | undefined {
  const roots: DiagnosticMessageWitness[] = [];
  const stack: DiagnosticMessageWitness[] = [];
  for (const entry of entries) {
    // The current TSZ public JSON stores TypeScript message-chain continuations
    // as unlocated, depth-indexed related-information records. A located record
    // cannot prove recursive related-information identity through this schema.
    if (entry.file !== '' || entry.start !== 0 || entry.length !== 0) return undefined;
    if (entry.depth > stack.length + 1) return undefined;
    const node: DiagnosticMessageWitness = {
      text: normalizeDiagnosticText(entry.message_text, scope.scopeDirectory),
      // TSZ's message-chain schema omits the redundant category. Message-chain
      // nodes inherit the owning diagnostic category at the compiler boundary.
      category,
      code: `TS${entry.code}`,
      next: [],
    };
    if (entry.depth === 1) {
      roots.push(node);
    } else {
      stack[entry.depth - 2]?.next.push(node);
    }
    stack.length = entry.depth - 1;
    stack.push(node);
  }
  return roots;
}

/** Parse only complete, structurally provable TSZ diagnostic JSON. */
export function canonicalizeTszDiagnosticsJson(
  value: unknown,
  scope: DiagnosticNormalizationScope,
): DiagnosticWitness[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const diagnostics: DiagnosticWitness[] = [];
  for (const raw of value) {
    const diagnostic = parseTszDiagnostic(raw);
    if (diagnostic === undefined) return undefined;
    const diagnosticPath = normalizeDiagnosticPath(diagnostic.file, scope);
    const messageChain = tszMessageChain(
      diagnostic.related_information,
      scope,
      diagnostic.category,
    );
    if (diagnosticPath === undefined || messageChain === undefined) return undefined;
    const global = diagnosticPath === null;
    if (global && (diagnostic.start !== 0 || diagnostic.length !== 0)) return undefined;
    diagnostics.push({
      path: diagnosticPath,
      start: global ? null : diagnostic.start,
      length: global ? null : diagnostic.length,
      category: diagnostic.category,
      code: `TS${diagnostic.code}`,
      text: normalizeDiagnosticText(diagnostic.message_text, scope.scopeDirectory),
      messageChain,
      relatedInformation: [],
    });
  }
  return diagnostics;
}

function typeScriptCategory(value: number): DiagnosticCategoryWitness | undefined {
  switch (value) {
    case 0: return 'warning';
    case 1: return 'error';
    case 2: return 'suggestion';
    case 3: return 'message';
    default: return undefined;
  }
}

interface MessageLocation {
  path: string | null;
  pos: number;
  end: number;
}

function isGlobalApiSpan(pos: number, end: number): boolean {
  // The native API currently serializes global diagnostics as 0/0. Accept the
  // documented unlocated sentinel too, but no mixed or positive global span.
  return (pos === 0 && end === 0) || (pos === -1 && end === -1);
}

function canonicalizeTypeScriptMessage(
  diagnostic: TypeScriptDiagnostic,
  scope: DiagnosticNormalizationScope,
  owner: MessageLocation,
): DiagnosticMessageWitness | undefined {
  const category = typeScriptCategory(diagnostic.category);
  const code = numericCode(diagnostic.code);
  const diagnosticPath = normalizeDiagnosticPath(diagnostic.fileName, scope);
  const unlocated = diagnosticPath === null && isGlobalApiSpan(diagnostic.pos, diagnostic.end);
  const repeatsOwnerLocation = diagnosticPath === owner.path &&
    diagnostic.pos === owner.pos && diagnostic.end === owner.end;
  if (
    category === undefined ||
    code === undefined ||
    diagnosticPath === undefined ||
    typeof diagnostic.text !== 'string' ||
    (!unlocated && !repeatsOwnerLocation) ||
    (diagnostic.relatedInformation?.length ?? 0) !== 0
  ) {
    return undefined;
  }
  const next = canonicalizeTypeScriptMessages(diagnostic.messageChain ?? [], scope, owner);
  if (next === undefined) return undefined;
  return {
    text: normalizeDiagnosticText(diagnostic.text, scope.scopeDirectory),
    category,
    code,
    next,
  };
}

function canonicalizeTypeScriptMessages(
  diagnostics: readonly TypeScriptDiagnostic[],
  scope: DiagnosticNormalizationScope,
  owner: MessageLocation,
): DiagnosticMessageWitness[] | undefined {
  const messages: DiagnosticMessageWitness[] = [];
  for (const diagnostic of diagnostics) {
    const message = canonicalizeTypeScriptMessage(diagnostic, scope, owner);
    if (message === undefined) return undefined;
    messages.push(message);
  }
  return messages;
}

function canonicalizeTypeScriptDiagnostic(
  diagnostic: TypeScriptDiagnostic,
  scope: DiagnosticNormalizationScope,
): DiagnosticWitness | undefined {
  const diagnosticPath = normalizeDiagnosticPath(diagnostic.fileName, scope);
  const category = typeScriptCategory(diagnostic.category);
  const code = numericCode(diagnostic.code);
  if (
    diagnosticPath === undefined ||
    category === undefined ||
    code === undefined ||
    typeof diagnostic.text !== 'string' ||
    !Number.isSafeInteger(diagnostic.pos) ||
    !Number.isSafeInteger(diagnostic.end)
  ) {
    return undefined;
  }
  const global = diagnosticPath === null;
  if (global ? !isGlobalApiSpan(diagnostic.pos, diagnostic.end) : diagnostic.pos < 0 || diagnostic.end < diagnostic.pos) {
    return undefined;
  }
  const messageChain = canonicalizeTypeScriptMessages(
    diagnostic.messageChain ?? [],
    scope,
    { path: diagnosticPath, pos: diagnostic.pos, end: diagnostic.end },
  );
  if (messageChain === undefined) return undefined;
  const relatedInformation: DiagnosticWitness[] = [];
  for (const related of diagnostic.relatedInformation ?? []) {
    const witness = canonicalizeTypeScriptDiagnostic(related, scope);
    if (witness === undefined) return undefined;
    relatedInformation.push(witness);
  }
  return {
    path: diagnosticPath,
    start: global ? null : diagnostic.pos,
    length: global ? null : diagnostic.end - diagnostic.pos,
    category,
    code,
    text: normalizeDiagnosticText(diagnostic.text, scope.scopeDirectory),
    messageChain,
    relatedInformation,
  };
}

function compareNullable<T>(left: T | null, right: T | null, compare: (a: T, b: T) => number): number {
  if (left === null) return right === null ? 0 : -1;
  if (right === null) return 1;
  return compare(left, right);
}

function compareString(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function compareNumber(left: number, right: number): number {
  return left - right;
}

function compareTypeScriptMessageLists(
  left: readonly TypeScriptDiagnostic[],
  right: readonly TypeScriptDiagnostic[],
): number {
  const sharedLength = Math.min(left.length, right.length);
  for (let index = 0; index < sharedLength; index++) {
    const difference = compareTypeScriptMessage(left[index], right[index]);
    if (difference !== 0) return difference;
  }
  return left.length - right.length;
}

function compareTypeScriptMessage(
  left: TypeScriptDiagnostic,
  right: TypeScriptDiagnostic,
): number {
  return compareString(left.text, right.text) ||
    compareNumber(left.code, right.code) ||
    compareNumber(left.category, right.category) ||
    compareTypeScriptMessageLists(left.messageChain ?? [], right.messageChain ?? []);
}

function compareTypeScriptDiagnosticLists(
  left: readonly TypeScriptDiagnostic[],
  right: readonly TypeScriptDiagnostic[],
  scope: DiagnosticNormalizationScope,
): number {
  const sharedLength = Math.min(left.length, right.length);
  for (let index = 0; index < sharedLength; index++) {
    const difference = compareTypeScriptDiagnostic(left[index], right[index], scope);
    if (difference !== 0) return difference;
  }
  return left.length - right.length;
}

function compareTypeScriptDiagnostic(
  left: TypeScriptDiagnostic,
  right: TypeScriptDiagnostic,
  scope: DiagnosticNormalizationScope,
): number {
  const leftPath = left.fileName === undefined
    ? null
    : resolvedDiagnosticFile(left.fileName, scope.invocationDirectory);
  const rightPath = right.fileName === undefined
    ? null
    : resolvedDiagnosticFile(right.fileName, scope.invocationDirectory);
  return compareNullable(leftPath, rightPath, compareString) ||
    compareNumber(left.pos, right.pos) ||
    compareNumber(left.end - left.pos, right.end - right.pos) ||
    compareNumber(left.code, right.code) ||
    compareString(left.text, right.text) ||
    compareTypeScriptMessageLists(left.messageChain ?? [], right.messageChain ?? []) ||
    // TypeScript sorts a more elaborated otherwise-equal diagnostic first.
    compareNumber(
      right.relatedInformation?.length ?? 0,
      left.relatedInformation?.length ?? 0,
    ) ||
    compareTypeScriptDiagnosticLists(
      left.relatedInformation ?? [],
      right.relatedInformation ?? [],
      scope,
    ) ||
    compareNumber(left.category, right.category);
}

/** Reproduce the compiler boundary's canonical sort/dedup over API phases. */
export function canonicalizeTypeScriptDiagnostics(
  diagnostics: readonly TypeScriptDiagnostic[],
  scope: DiagnosticNormalizationScope,
): DiagnosticWitness[] | undefined {
  const entries: Array<{
    diagnostic: TypeScriptDiagnostic;
    witness: DiagnosticWitness;
  }> = [];
  for (const diagnostic of diagnostics) {
    const witness = canonicalizeTypeScriptDiagnostic(diagnostic, scope);
    if (witness === undefined) return undefined;
    entries.push({ diagnostic, witness });
  }
  // Sort on the API's original physical locations and message bytes. Sorting
  // after path/text normalization can invert parent-relative roots or collapse
  // two otherwise distinct compiler-order keys.
  entries.sort((left, right) =>
    compareTypeScriptDiagnostic(left.diagnostic, right.diagnostic, scope)
  );
  const witnesses = entries.map(entry => entry.witness);
  return witnesses.filter((witness, index) =>
    index === 0 || JSON.stringify(witness) !== JSON.stringify(witnesses[index - 1])
  );
}

export function witnessCodesMatch(
  witnesses: readonly DiagnosticWitness[],
  diagnosticCodes: readonly string[],
): boolean {
  return witnesses.length === diagnosticCodes.length &&
    witnesses.every((witness, index) => witness.code === diagnosticCodes[index]);
}
