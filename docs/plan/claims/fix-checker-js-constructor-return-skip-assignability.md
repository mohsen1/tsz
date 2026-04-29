# fix(checker): suppress constructor return-type assignability check in JS files

- **Date**: 2026-04-29
- **Branch**: `fix/checker-js-constructor-return-skip-assignability`
- **PR**: #1749
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix wrong-code emission on `conformance/jsdoc/extendsTag5.ts`: tsc expects
only TS2344 from the JSDoc generic-constraint mismatch, but tsz also emits
extra TS2322 + TS2409 at `return a` inside a JS class constructor.

In JavaScript, returning a value from a constructor replaces `this` at
runtime — so `return a` (where `a` is some unrelated type) is idiomatic
in `--checkJs` mode. tsc gates the constructor return-type assignability
check on `!isJavaScriptFile`, suppressing both TS2322 and TS2409 in JS.

## Root Cause

`crates/tsz-checker/src/types/type_checking/core_statement_checks.rs::check_return_statement`
sets `skip_assignability` only for bare `return;` in constructors:

```rust
let skip_assignability = is_in_constructor && return_data.expression.is_none();
```

In JS files, the constructor return-type check should also skip when an
expression is present.

## Fix

Extend `skip_assignability` to also fire when `is_in_constructor && self.is_js_file()`,
mirroring tsc's `isJavaScriptFile` gate. Both the TS2322 path and the
companion TS2409 emission then short-circuit cleanly.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/core_statement_checks.rs` (~7 LOC)

## Verification

- `cargo nextest run -p tsz-checker -E 'test(/js_constructor|js_class|constructor|return_statement/)'` (253/253 pass)
- `./scripts/conformance/conformance.sh run --filter "extendsTag5" --verbose` (1/1 PASS, was 0/1)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run`: net **12235 → 12236 (+1)**;
  3 improvements (`extendsTag5.ts`, `declarationEmitExpressionInExtends6.ts`,
  `iterableTReturnTNext.ts`); 2 regressions reported by snapshot diff
  (`inferenceOfNullableObjectTypesWithCommonBase.ts`,
  `typeArgumentInference.ts`) — both reproduce on bare `origin/main` without
  this fix, so they are stale-snapshot artifacts, not caused by this change.
