# fix(checker): elaborate arrow-body return mismatch on direct assignment to generic function-type targets

- **Date**: 2026-04-26 16:48:27
- **Branch**: `fix/checker-elaborate-arrow-init-generic-return`
- **PR**: TBD
- **Status**: claim
- **Workstream**: Conformance — TS2322 anchor parity for direct assignment to generic function-type targets

## Intent

`try_elaborate_function_arg_return_error` skipped *all* body-level
elaboration whenever the expected callback return type contained a free
type parameter. That skip was added to prevent false TS2322s during
generic-call inference (e.g. `compose<A, B, C>(...)` where `B` is still
unresolved when the callback body is checked). It is correct for that
path but fires too eagerly when the call site is *direct assignment* to
a generic function-type target — there the type parameters are bound by
the target's own quantifier (`type EnvFunction = <T>() => T`), not by an
outer call, so a concrete body type really is unsatisfiable against `T`
and tsc anchors TS2322 at the body expression.

This PR splits the skip behind an `allow_unresolved_holes` flag and
takes the elaboration path for the direct-assignment entrypoint
(`try_elaborate_assignment_source_error → arrow/function expression
case`). Call-argument elaboration still skips on unresolved holes
(unchanged behavior).

## Files Touched

- `crates/tsz-checker/src/error_reporter/call_errors/elaboration.rs`
  (~70 LOC: split `try_elaborate_function_arg_return_error` behind a
  new `_with_options` helper, route the direct-assignment branch
  through `allow_unresolved_holes = true`).
- `crates/tsz-checker/src/error_reporter/call_errors_tests.rs`
  (~95 LOC: two new unit tests — positive (direct assignment elaborates
  at body) and negative (generic-call inference does not elaborate)).

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2891 tests pass.
- `cargo nextest run -p tsz-checker --lib -E 'test(ts2322_anchors_at_arrow_body) or test(ts2322_skips_arrow_body_elaboration)'` — both new tests pass.
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` —
  full conformance: **+3 tests** (12183 → 12186), zero regressions.
  - Improvements:
    - `compiler/jsExportMemberMergedWithModuleAugmentation2.ts`
    - `conformance/statements/tryStatements/catchClauseWithTypeAnnotation.ts`
    - `conformance/types/literal/templateLiteralTypes5.ts`
- Targeted: `unresolvableSelfReferencingAwaitedUnion.ts` advances from
  whole-assignment-anchored TS2322 to body-anchored TS2322; remains
  fingerprint-only because the message text still expands the alias
  (`SimpleType` → `string | Promise<SimpleType>`) — that is a
  separate alias-display path in `format_assignment_source_type_for_diagnostic`
  and is out of scope here.
