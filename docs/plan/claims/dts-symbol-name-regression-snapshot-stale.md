---
name: dts symbol-name regression — `[import]` instead of `[symbolName]`
description: Recent merges regressed 5-6 declaration-emit tests; conformance snapshot is stale and blocking PRs at the snapshot guard
type: claim
status: ready-for-investigation
date: 2026-05-03
---

# Claim

Multiple in-flight PRs (#2418, #2430 confirmed; likely all PRs based
on current main) hit the conformance snapshot guard with the same
cluster of regressions:

- declarationEmitMappedTypeTemplateTypeofSymbol.ts
- declarationEmitMonorepoBaseUrl.ts
- declarationEmitUnsafeImportSymbolName.ts
- enumDeclarationEmitInitializerHasImport.ts
- symbolLinkDeclarationEmitModuleNamesImportRef.ts
- importCallExpressionDeclarationEmit1.ts (in #2418's run only)

These regressions are present on **main** itself, not introduced by
the PRs. Reproduced locally:

```
$ tsz-conformance --filter declarationEmitMappedTypeTemplateTypeofSymbol \
    --cache-file scripts/conformance/tsc-cache-full.json
TS4118 c.ts:3:14 The type of this node cannot be serialized because its property '[import]' cannot be serialized. (missing=0, extra=1)
TS4118 c.ts:3:14 The type of this node cannot be serialized because its property '[timestampSymbol]' cannot be serialized. (missing=1, extra=0)
TS4118 b.ts:2:14 ... '[import]' ...
TS4118 b.ts:2:14 ... '[timestampSymbol]' ...
```

We are emitting `[import]` (the import-binding name) where tsc emits
`[timestampSymbol]` (the unique-symbol's actual name).

## Source pattern

```ts
// a.d.ts
export declare const timestampSymbol: unique symbol;
export declare const Timestamp: { [TKey in typeof timestampSymbol]: true; };
export declare function now(): typeof Timestamp;

// c.ts (consumer, hits TS4118)
import { now } from "./a";
export const timestamp = now();
```

The mapped-type member key is `[typeof timestampSymbol]`. When dts
emit walks this and finds the `timestampSymbol` binding is not
re-exported from the consumer file, it must emit TS4118. The error
message embeds the symbol's display name. We resolve that name to the
**import-side binding** (`import` literal? or the module-namespace
binding? — needs trace) instead of the original `timestampSymbol`
declaration name.

## Bisect result

Confirmed by manual bisect (2026-05-03):

- `71a4f93ecda` (#2419 merge): PASS
- `5b4e3bfeb05` (#2421 merge): PASS
- `15897b69816` (#2425 merge): **FAIL** — regression introduced here

PR #2425 (`fix(dts): alias reserved function namespace properties`)
introduces the regression. The PR adds
`is_late_bound_reserved_binding_name` checks in
`crates/tsz-emitter/src/declaration_emitter/helpers/function_analysis.rs`
that filter out reserved words (`import`, `in`, `typeof`, etc.) from
namespace member names — returning `None` instead of `Some(text)`.

## Hypothesis

The printed-type text for the test now contains
`[TKey in typeof import("./a").timestampSymbol]` (or similar),
where `import` appears as a keyword. The TS4118 resolver
(`find_non_serializable_property_name_in_printed_type` in
`portability_resolve.rs:31`) looks for ` in typeof ` and grabs the
next identifier-like sequence — which is now `import` (truncated at
the `"` because `(` and `"` are not in the allowed-char set).

PR #2425's changes appear to make the emitter prefer the
rewritten `import("X").Y` form for the late-bound member's printed
type, but the TS4118 resolver assumed the printed form is
`typeof Symbol`, not `typeof import("X").Symbol`.

## Fix sketch

Update `find_non_serializable_property_name_in_printed_type` in
`portability_resolve.rs:31` to skip past `import("…").` between
`in typeof ` and the actual symbol name, so the extracted
`symbol_expr` is `timestampSymbol`, not `import`.

Specifically: after `in typeof `, if the next token starts with
`import("`, advance past the closing `")` and the following `.`,
then extract the identifier from there.

## Next-iteration approach

1. Implement the resolver fix in `portability_resolve.rs:31`, with
   a regression unit test.
2. Re-run full conformance to confirm 5-6 dts tests flip back to
   PASS.
3. Refresh `scripts/conformance/conformance-snapshot.json` to
   unblock other PRs at the snapshot gate.

## Status

Ready for fix — bisect complete and hypothesis specific. Affects
multiple in-flight PRs at the snapshot guard.
