---
name: render_return_type_mismatch elaboration cascade
status: claimed
timestamp: 2026-05-04 09:30:00
branch: fix/checker-render-return-type-mismatch-no-redundant-cascade
---

# Claim

Workstream 1 (Diagnostic Conformance) — TS2322 / TS2345 elaboration shape
parity for function-return-type mismatches.

## Problem

Two compounding bugs in the TS2322/TS2345 elaboration cascade for
function-return-type mismatches:

### Bug 1 — bogus outer-source re-render at depth >= 1

Given:

```ts
declare let f1: (x: Object) => string;
declare let f3: (x: Object) => Object;
f1 = f3;
```

tsz emitted:

```
error TS2322: Type '(x: Object) => Object' is not assignable to type '(x: Object) => string'.
  Return type 'Object' is not assignable to 'string'.
  Type '(x: Object) => Object' is not assignable to type 'string'.   ← BOGUS
```

The third line claims the outer function type is not assignable to the
inner return type `string` — a category error.

**Root cause:** `format_nested_assignment_source_type_for_diagnostic`
(`error_reporter/core/diagnostic_source/assignment_formatting.rs`)
re-derives the source from the anchor's expression node, ignoring the
passed `source` parameter. During nested elaboration of a structural
failure (e.g. function-return mismatch elaborates with the inner return
types as `source`/`target`, but the anchor still points at the outer
assignment expression), this returns the OUTER value's type instead of
the inner return type passed in.

**Fix:** before walking the anchor's expression, check whether the
expression's type matches the passed `source`. If not, format `source`
directly via `format_assignability_type_for_message`.

### Bug 2 — double elaboration of the same gap

Even with Bug 1 fixed, tsz still emitted:

```
error TS2322: Type '(x: Object) => Object' is not assignable to type '(x: Object) => string'.
  Return type 'Object' is not assignable to 'string'.   ← extra
  Type 'Object' is not assignable to type 'string'.
```

tsc emits only the second line:

```
error TS2322: Type '(x: Object) => Object' is not assignable to type '(x: Object) => string'.
  Type 'Object' is not assignable to type 'string'.
```

**Root cause:** `render_return_type_mismatch` always emits the
"Return type 'X' is not assignable to 'Y'." fallback label AND then
recursively renders the nested reason — duplicating the inner mismatch.

**Fix:** in the depth=0 branch, only emit the "Return type ..." label
when there is no nested reason. When a nested reason is present, let the
recursive render carry the inner mismatch directly. tsc never uses
"Return type ..." in its output (verified: zero matches in tsc baseline
files).

## Files Touched

- `crates/tsz-checker/src/error_reporter/core/diagnostic_source/assignment_formatting.rs`
  (+13 / 0) — add `source_matches_anchor_expr` guard at the start of
  `format_nested_assignment_source_type_for_diagnostic`.
- `crates/tsz-checker/src/error_reporter/render_failure.rs`
  (+8 / -8) — restructure the depth=0 branch of
  `render_return_type_mismatch` to emit "Return type ..." only when no
  nested reason is present.
- `crates/tsz-checker/tests/ts2322_tests.rs` (+88 / 0) — two new
  structural tests (different binding names + return types) per the
  anti-hardcoding directive.

## Tests

- New: `ts2322_function_return_mismatch_does_not_double_elaborate_with_outer_source`
- New: `ts2322_function_return_mismatch_param_name_independent` (locks
  the fix as structural per CLAUDE.md §25).
- Existing: `test_ts2345_function_return_mismatch_includes_related_return_detail`
  and `test_ts2345_function_return_mismatch_related_detail_qualifies_same_named_returns`
  still pass (these don't have a nested_reason that triggers the
  recursive render — confirmed empirically).

## Verification

- `cargo nextest run -p tsz-checker` — 6191/6191 pass, 36 skipped.
- Full conformance: `12400 → 12413 (+13)`. Two listed regressions
  (`avoidCycleWithVoidExpressionReturnedFromArrow.ts` and
  `jsEnumTagOnObjectFrozen.ts`) are pre-existing drift on plain main
  (verified by stash + rerun).

## Notes

The 15 improvements span function-return mismatch tests across
`compiler/`, `conformance/jsx/`, and `conformance/salsa/` directories,
indicating the elaboration cascade was a shared bug across multiple
diagnostic surfaces.
