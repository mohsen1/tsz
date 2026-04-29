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

## Required Fix (architectural — iter 2)

Instrumented `lookup_type_alias_name_for_display` and confirmed the
existing display-alias guard at line 1713 works correctly for our case
— for the `Pick<A,'x'>` Object (TypeId 239), `get_display_alias(239) =
Some(459)` (an Application of T), so the checker-side helper returns
`None` already. **The "C" output is coming from the solver-side
formatter, not from the checker helper.**

The actual `find_def_for_type(239) -> C_def` lookup happens in
`crates/tsz-solver/src/diagnostics/format/mod.rs:674-708`. There's
already a `skip_application_alias_names` flag on the formatter that
gates the same skip we need at line 690-692:

```rust
|| (self.skip_application_alias_names
    && def.type_params.is_empty()
    && self.interner.get_display_alias(type_id).is_some())
```

When `skip_application_alias_names` is **on**, the alias name "C" is
skipped and the structural form / Application form is used. The flag
is currently set in three places:

- `error_reporter/assignability.rs:1157`
- `error_reporter/core/diagnostic_source.rs:648`
- `error_reporter/core/type_display.rs:1331`

**Required fix:** Set `with_skip_application_alias_names()` on the
formatter used by `format_type_for_assignability_message` (or the
specific TS2322 source/target rendering). Caveat: this flag may have
wide cross-test impact — it's intentionally not on by default — so the
fix needs careful regression testing across the
`assignmentCompatibility` / variance suites.

## Out of Scope

The full audit of where to enable `skip_application_alias_names`. This
turned out to be a substantial change touching the formatter
configuration system, not a localized helper edit.

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
