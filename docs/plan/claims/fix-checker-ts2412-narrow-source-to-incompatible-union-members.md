---
name: TS2412 source narrowed to incompatible union members
status: claimed
timestamp: 2026-05-03 11:39:10
branch: fix/checker-ts2412-narrow-source-to-incompatible-union-members
---

# Claim

Workstream 1 (Diagnostic Conformance) — TS2412 source display under
`exactOptionalPropertyTypes: true` should report only the union members
that fail assignability against the target.

## Problem

For `obj.prop = value` where `value: string | undefined` and
`obj.prop: string`, tsc emits:

  TS2412: Type **'undefined'** is not assignable to type 'string' with
  'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the
  type of the target.

Tsz emitted:

  TS2412: Type **'string | undefined'** is not assignable to type 'string'
  ...

The diagnostic-source helper printed the entire source union; only the
non-target-assignable member (`undefined`) is the actual mismatch.

## Fix

Narrow the source for the TS2412 message to its incompatible union
members before formatting. `exact_optional_source_for_message` walks the
source's union members, drops every member assignable to the evaluated
target, and returns either the single offending member, a smaller union,
or the original source if nothing could be filtered.

Wired into the TS2412 emission site in `assignability.rs` next to the
existing target-side helper.

## Tests

- All 3228 `tsz-checker` lib tests pass.
- Conformance net **+1** vs current main: `parserClassDeclaration1.ts`
  flips to PASS. Several `strictOptionalProperties1.ts` fingerprints
  collapse closer to tsc's TS2412 source display (e.g.
  `string | undefined` → `undefined`).
- Zero regressions.
