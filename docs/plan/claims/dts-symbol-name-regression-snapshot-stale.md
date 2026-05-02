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

## Likely culprit window

Recent merges that touched dts emit / symbol name resolution:

- 5b4e3bfeb05 fix(dts): preserve symlinked package import names (#2421)
- 15897b69816 fix(dts): alias reserved function namespace properties (#2425)
- 2dc7165af50 fix(dts): emit js commonjs type reference aliases (#2413)

The `[import]` literal in our output strongly suggests one of these
changed how an import-side symbol's display name is resolved when
walking through `typeof X` where X is itself imported.

## Next-iteration approach

1. `git bisect` between #2417 (snapshot refresh, baseline known good)
   and current main on this single test:
   ```
   tsz-conformance --filter declarationEmitMappedTypeTemplateTypeofSymbol \
     --cache-file scripts/conformance/tsc-cache-full.json
   ```
2. Once the offending commit is identified, fix the symbol-name
   resolution OR update the snapshot guard if the change is
   intentional and tsc parity isn't expected here.
3. Refresh `scripts/conformance/conformance-snapshot.json` so other
   PRs unblock at the snapshot gate.

## Status

Ready for investigation. Affects multiple in-flight PRs at the
snapshot guard; high-leverage to bisect and either fix or refresh.
