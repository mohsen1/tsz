---
name: genericTypeAssertions1 Application-overlap gate net-negative attempt
description: Adding strict type-argument overlap check to types_are_comparable_for_assertion fixes target test but regresses 4 declaration-emit tests
type: claim
status: deferred
date: 2026-05-03
---

# Claim

`genericTypeAssertions1.ts` is missing TS2352 ("conversion may be a
mistake") on `<A<A<number>>>foo` where `foo: A<number>`. Both types
share the same constructor `A<T>` but differ in T arg.

## Attempted fix

In `crates/tsz-solver/src/type_queries/flow.rs:740` (between the empty-
object short-circuit and the property-overlap fallback), added:

```rust
if let (Some(TypeData::Application(s_id)), Some(TypeData::Application(t_id))) =
    (db.lookup(source), db.lookup(target))
{
    let s_app = db.type_application(s_id);
    let t_app = db.type_application(t_id);
    if s_app.base == t_app.base && s_app.args.len() == t_app.args.len() {
        for (s_arg, t_arg) in s_app.args.iter().zip(t_app.args.iter()) {
            if !types_are_comparable_for_assertion_inner(db, *s_arg, *t_arg, depth + 1) {
                return false;
            }
        }
        return true;
    }
}
```

## Result

Conformance delta vs baseline (12346 → 12352, but 215 FAIL → 214):

- IMPROVEMENTS (3): `reservedWords2.ts`, `unresolvableSelfReferencingAwaitedUnion.ts`,
  `tsxTypeArgumentResolution.tsx` — likely stale-baseline credit, not
  actually attributable to the fix.
- REGRESSIONS (4): `declarationEmitMappedTypeTemplateTypeofSymbol.ts`,
  `declarationEmitMonorepoBaseUrl.ts`,
  `declarationEmitUnsafeImportSymbolName.ts`,
  `symbolLinkDeclarationEmitModuleNamesImportRef.ts`.
- 1 new crash.
- Net: **-1 PASS** (and the target test itself does NOT flip — the
  Application form is already deep-evaluated by the time
  `types_are_comparable_for_assertion` is called from
  `dispatch.rs:1421`, so the rule never fires for the target).

## Why it doesn't even hit the target test

`dispatch.rs:1421` calls the comparable-for-assertion check with
`deep_expr` and `deep_asserted` — already-evaluated forms. The
Application wrapper is unwrapped to its substituted body
(`{foo: (x: number) => void}` vs `{foo: (x: A<number>) => void}`).
By the time we're inside `types_are_comparable_for_assertion_inner`,
both types are `Object` shapes, not `Application`. The Application
branch I added never matches.

## Why declaration-emit tests regressed

Unknown. The flow.rs function is shared across many code paths. The
regression suggests the rule fires somewhere in dts emit's type
comparison, where two same-base Application types with structurally-
different-but-still-overlap args should be considered overlapping.

## What a real fix needs

1. Move the Application gate **earlier** — before the deep-evaluation
   step in `dispatch.rs:1421` — or pass the un-evaluated types
   alongside the evaluated ones into the comparable check.
2. Recognize the Application form via type-application metadata even
   after deep-evaluation (e.g. by recording the original Application
   wrapper on the Object shape).
3. Match tsc's `compareTypeArguments` more faithfully: when type
   parameters are read-position (covariant, like in
   `IteratorReturnResult.value`), allow widening; only require strict
   incompatibility when the parameter is in invariant or write-only
   positions.

## Status

Deferred. Naive fix is net-negative on conformance and doesn't even
flip the target test. Needs deeper integration with the assertion
flow's deep-evaluation step.
