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
- **Identified**: `check_generic_index_access_subtype` lives at `crates/tsz-solver/src/relations/subtype/helpers.rs:269`. Call site at `subtype/core.rs:2228`. Visitor at `subtype/visitor.rs:828` already has the s_param.name != t_param.name guard at L840-855.
- **Iteration 2 finding (DEBUG eprintln)**: For repro4 (`function f<Alpha…, Beta…>(x: I[Alpha]): I[Beta] { return x; }`), only ONE `is_assignable_impl` call fires, and it is `source=TypeParameter(name=Alpha) target=Union(...)` — almost certainly the constraint check `Alpha extends keyof I`, NOT the `return x` assignability check. The `return x` check is never reaching the assignability gate. `visit_index_access` is never called, hence the existing T1 vs T2 guard never gets a chance to fire.
- **Hypothesis (next iteration)**: the return-statement handler in the checker is silently skipping the `is_assignable` call when the return type is a deferred IndexAccess containing a generic type parameter. Need to grep `check_return_statement` / `return_type_check` in `tsz-checker` and understand the bypass.
- **Issue #1 partially fixed**: `is_concrete_source_to_deferred_keyof_index_access` at L1989 was routing the per-candidate check through `self.ctx.types.is_assignable_to` (the strict Judge / SubtypeChecker), instead of the checker's compat-aware `self.is_assignable_to` (the Lawyer / CompatChecker). The Judge rejects `{}` against object types with all-optional properties; the Lawyer accepts them. Switched the call to the checker entry point. Six unit tests in `crates/tsz-checker/tests/deferred_keyof_index_access_assignability_tests.rs` lock the structural rule with two name choices for the bound parameter (anti-hardcoding §25). All 11349 checker+solver tests pass; no regressions across the 13 indexedAccess conformance tests.
- **Issue #1 residual on full JSX corpus**: the conformance test still fingerprint-only. Debug trace shows `collect_deferred_index_access_candidate_types` returns 170 candidates of the form `DetailedHTMLProps<HTMLAttributes<error>, error>` — type arguments for `HTMLAnchorElement`/etc. resolve to ERROR even when the harness loads DOM lib. That ERROR substitution causes every per-candidate compat check to reject `{}`. Root cause is in candidate-type-argument resolution from generic indexed-access constraint distribution, separate from the Judge/Lawyer mixup fixed here. Deferred to a follow-up.
