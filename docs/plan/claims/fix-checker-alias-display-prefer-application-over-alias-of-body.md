# fix(checker): prefer the Application form (`T<A>`) over the alias-of-body (`C`) when both share a TypeId

- **Date**: 2026-04-29
- **Branch**: `fix/checker-alias-display-prefer-application-over-alias-of-body`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

`genericIndexedAccessVarianceComparisonResultCorrect.ts` is
fingerprint-only on TS2322 with a single missing/extra pair where only
the source/target display differs.

```ts
type T<X extends { x: any }> = Pick<X, 'x'>;
type C = T<A>;
type D = T<B>;
declare let a: T<A>;
declare let b: T<B>;
b = a;   // line 25
```

- tsc:  `Type 'T<A>' is not assignable to type 'T<B>'.`
- tsz:  `Type 'C' is not assignable to type 'D'.`

Both pairs are valid aliases for the same `TypeId` (the underlying
`Pick<A,'x'>` / `Pick<B,'x'>` shape), so tsz's printer is technically
not wrong — it's preferring the simpler alias `C`/`D` over the
application form `T<A>`/`T<B>`.

tsc's preference: pick the alias *used at the declaration site* of the
expression being checked. Since `a` is declared `let a: T<A>`, the
display for `typeof a` should resolve to `T<A>`, not its alias-equivalent
`C`. Without per-position context, tsc's `getTypeReferenceFromText`
keeps the application form on the type when it was encountered before
the structural alias.

## Required Fix (architectural)

Two viable approaches:

1. **Store-time priority.** When `register_resolved_type` runs for a
   `TypeAlias` whose body is itself an `Application(<other_alias>, …)`,
   *don't* register the application's resolved-body TypeId as also
   pointing to the wrapper alias. That prevents `C` from being a
   display candidate for the structural shape that `T<A>` already
   represents. The `display_alias` map should keep the most
   "informative" key, not just the most recent.
2. **Lookup-time priority.** When the formatter has multiple display
   candidates for a `TypeId`, prefer the candidate whose syntactic form
   (Application > bare Lazy alias) provides more information. Implement
   in `lookup_type_alias_name_for_display`.

Approach 1 is cleaner architecturally; approach 2 is more localized.

## Files Likely Involved

- `crates/tsz-checker/src/context/def_mapping.rs::register_resolved_type`
  (alias body registration)
- `crates/tsz-checker/src/error_reporter/core_formatting.rs::lookup_type_alias_name_for_display`
  (alias-name preference)
- `crates/tsz-solver/src/diagnostics/format/...` (formatter alias lookup)

## Verification

- New unit test locking the application-form preference for variables
  declared with the application annotation.
- `./scripts/conformance/conformance.sh run --filter "genericIndexedAccessVarianceComparisonResultCorrect" --verbose`
  — should flip to PASS.
- Targeted regression: should NOT regress the previously-fixed
  `parenthesisDoesNotBlockAliasSymbolCreation.ts` (PR #1738) where the
  preferred display IS the application form `A<{x:number}>`. The fix
  must keep that behavior intact.
