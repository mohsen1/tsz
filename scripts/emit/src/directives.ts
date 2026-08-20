/**
 * Canonical recognizers for TypeScript test-file directives
 * (`// @option: value`).
 *
 * This is the emit-harness binding of the canonical grammar owned by
 * `crates/conformance/src/test_directives.rs`; both are locked to
 * `scripts/test-directives/spec-vectors.json` by unit tests so the
 * harnesses cannot drift apart (issue #13127).
 *
 * Grammar:
 * - Key/value directive: optional leading whitespace, `//`, optional
 *   whitespace, `@`, an ASCII `[A-Za-z0-9_]+` key, optional whitespace,
 *   `:`, then the value (rest of line). Keys are case-insensitive;
 *   values are trimmed.
 * - Flag directive: `// @name` with `[A-Za-z0-9_-]+` and nothing but
 *   whitespace after the name (`// @ts-check`, `// @ts-nocheck`).
 * - List values (`@lib: es5,dom`) split on commas, trimmed, empties
 *   dropped. Variant-valued scalar options (`@target: es5,es2015`)
 *   take the first comma-separated value in single-variant runs.
 */

export interface DirectiveLine {
  /** Directive key, lowercased. */
  key: string;
  /** Value with surrounding whitespace trimmed. */
  value: string;
}

const DIRECTIVE_LINE_RE = /^\s*\/\/\s*@([A-Za-z0-9_]+)\s*:([^\r\n]*)$/;
const FLAG_LINE_RE = /^\s*\/\/\s*@([A-Za-z0-9_-]+)\s*$/;

/** Recognize a `// @key: value` directive line. */
export function parseDirectiveLine(line: string): DirectiveLine | undefined {
  const match = line.replace(/\r$/, '').match(DIRECTIVE_LINE_RE);
  if (!match) return undefined;
  return { key: match[1].toLowerCase(), value: match[2].trim() };
}

/**
 * Recognize a flag-form directive line (`// @ts-check`). Returns the
 * name as written; compare case-insensitively.
 */
export function parseFlagDirectiveLine(line: string): string | undefined {
  const match = line.replace(/\r$/, '').match(FLAG_LINE_RE);
  return match ? match[1] : undefined;
}

/** Split a list-valued directive value (`@lib: es5,dom`). */
export function splitListValues(value: string): string[] {
  return value.split(',').map(part => part.trim()).filter(part => part.length > 0);
}

/**
 * First comma-separated value of a variant-valued scalar directive
 * (`@target: es5,es2015` -> `es5`). Matches the conformance harness,
 * whose cache generator compiles only the first variant.
 */
export function firstListValue(value: string): string {
  return (value.split(',', 1)[0] ?? '').trim();
}
