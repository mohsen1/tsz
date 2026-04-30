# fix(checker): TS2322 on `IntrinsicElements[T1]` assigned to `IntrinsicElements[T2]`

- **Date**: 2026-04-30
- **Branch**: `fix/checker-ts2322-generic-jsx-indexed-access-empty-object`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints — Big3)

## Intent

Make `errorInfoForRelatedIndexTypesNoConstraintElaboration.ts` pass.

tsc emits one TS2322 at line 6 col 15:
> Type 'IntrinsicElements[T1]' is not assignable to type 'IntrinsicElements[T2]'.

tsz emits one TS2322 at the wrong location (line 5 col 13) — false-positive:
> Type '{}' is not assignable to type 'IntrinsicElements[T1]'.

Two independent failures rolled into one fingerprint mismatch:

1. **Line 5 false positive (specific to large JSX corpus).** `let c1: JSX.IntrinsicElements[T1] = {};` rejects `{}`. A 3-element reproduction passes (no error). The full JSX `IntrinsicElements` union triggers the rejection — likely scale-related (fuel/memo/large-union). Hypothesis: `check_generic_index_access_subtype` distributes over the ~200-key constraint union, evaluates `IntrinsicElements["a" | "abbr" | …]`, and `check_subtype({}, …)` fails for at least one entry (or hits a fuel limit and returns false-as-default).
2. **Line 6 missing positive (general).** `const c2: I[T2] = c1` should reject `c1: I[T1]`. Reproduces with a 3-element JSX repro:
   ```ts
   declare namespace JSX { interface IntrinsicElements { a: {key?: number}; b: {key?: number}; c: {key?: number}; } }
   class I<T1 extends keyof JSX.IntrinsicElements, T2 extends keyof JSX.IntrinsicElements> { M() {
     let c1: JSX.IntrinsicElements[T1] = {};            // tsz: silently OK
     const c2: JSX.IntrinsicElements[T2] = c1;          // tsz: silently OK — should be TS2322
   }}
   ```
   `check_generic_index_access_subtype` in `crates/tsz-solver/src/relations/subtype/helpers.rs` *does* contain the right rule at L281–289 (`s_obj == t_obj && s_param != t_param ⇒ return false`), but the surrounding visitor path likely returns True via a different code path (compat looseness?), so the helper's False never propagates to a TS2322.

## Files Touched (planned)

- `crates/tsz-solver/src/relations/subtype/helpers.rs` (the `check_generic_index_access_subtype` helper and/or the visitor-vs-helper conjunction at the call site in `core.rs:2228`)
- `crates/tsz-solver/src/relations/subtype/core.rs`
- New unit tests in `tsz-solver`

## Verification (planned)

- Conformance: `errorInfoForRelatedIndexTypesNoConstraintElaboration.ts` flips fingerprint-only → PASS
- 32 fingerprint-only TS2322 failures may shift; verify net delta ≥ 0
- `cargo nextest run -p tsz-solver -p tsz-checker`

## Investigation log

- **Reproduced** locally with `/tmp/jsx_repro.ts` (3-element IntrinsicElements). Demonstrates issue #2 cleanly. Issue #1 only with full react16.d.ts corpus.
- **Ruled out**: the line-5 false positive does NOT reproduce on a small union — the helper handles small unions correctly.
- **Identified**: `check_generic_index_access_subtype` lives at `crates/tsz-solver/src/relations/subtype/helpers.rs:269`. Call site at `subtype/core.rs:2228`.
- **Next step**: add tracing in the helper and at the call site to confirm whether the False return from the helper is reaching the diagnostic emitter for issue #2; trace the per-key check on the full JSX union for issue #1 to find which element rejects `{}`.
