# fix(checker): emit TS2335 for `super` in inner-class method body after sibling arrow uses `super`

- **Date**: 2026-04-29
- **Branch**: `fix/checker-super-keyword-missed-in-method-after-arrow`
- **PR**: #1746
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance and Fingerprints)

## Intent

`typeOfThisInStaticMembers9.ts` is a fingerprint-only failure: tsz misses
exactly one expected TS2335 diagnostic. The missing one is the `super`
reference inside a `method() { return super.f + 6 }` of an inner
non-derived class. tsc emits TS2335 there ("'super' can only be referenced
in a derived class"); tsz emits no diagnostic.

## Minimal Repro (8 lines)

```ts
class C { static f = 1 }
class D extends C {
    static arrowFunctionBoundary = () => super.f + 1;        // [A]
    static functionAndClassDeclBoundary = (() => {
        class C {
            method () { return super.f + 6 }                  // [B]  expected: TS2335
        }
    })();
}
```

- tsz output: 0 diagnostics
- tsc:        TS2335 at [B]

If line [A] is removed, tsz correctly emits TS2335 at [B]. The arrow at
[A] is the trigger.

If [B] is replaced by an instance-property initializer
(`init = super.f + 5`), tsz also correctly emits TS2335 at the
initializer regardless of [A]. Only the *method body* path is blocked.

## Investigation Findings (iter 2 — root cause confirmed)

Instrumented the path with debug `eprintln`s:

- `get_type_of_super_keyword` IS called for the inner method's `super` (idx=22, pos=215).
- `check_super_expression` IS called and reaches the TS2335 gate.
- `find_enclosing_class(idx=22)` correctly returns `NodeIndex(30)` (the
  inner non-derived `class C`); `class_has_base()` returns `false`.
- `error_at_node(idx=22, …, code=2335)` IS invoked, with normalized span
  `start=215 length=5`.
- In `CheckerContext::error` (`crates/tsz-checker/src/context/core.rs:1582`)
  the `emitted_diagnostics.contains(&key)` check passes (no dedup)
  AND the diagnostic is pushed to `ctx.diagnostics`. The eprintln also
  shows `diag_count_before=0` for **every** push — meaning some caller
  is rolling back `ctx.diagnostics` (and the `emitted_diagnostics` dedup
  set) between every super-check pass.

**Root cause:** the inner method body's super check happens inside
`get_type_of_super_keyword`, which can be called from a speculative
type-computation context (the surrounding static field initializer is
being evaluated speculatively to produce the field's declared type).
The speculation snapshot/rollback mechanism in
`crates/tsz-checker/src/context/speculation.rs` truncates `ctx.diagnostics`
back to its snapshot length on rollback, discarding the TS2335. tsz's
existing `deferred_ts2454_errors` queue exists to defer TS2454 emission
past speculation but it is also truncated on rollback (see
`speculation.rs:175-178`), so it does not survive either.

The arrow at [A] never emits anything (the enclosing class IS derived,
so the gate doesn't fire), so its rollback is invisible.
For instance-property initializers (e.g. `init = super.f + 5`), the
type computation path runs in a non-speculative context, so the TS2335
push survives — that's why my super_test4 / super_test7 reproductions
that include an `init = super.f + N` field correctly emit TS2335 on the
init *and* on the method, but the method-only repro emits nothing.

## Required Fix (architectural)

`super` validity errors (TS2335, TS2660, TS2336, TS2337, TS2466) are
**grammar/scope** errors — they should not depend on whether the
surrounding type computation is speculative. They have to survive
speculation rollback.

Two viable approaches:

1. **Add a non-rollback diagnostic channel.** Introduce a parallel
   `ctx.permanent_diagnostics: Vec<Diagnostic>` that the speculation
   snapshot does NOT truncate. Route grammar-class super errors there
   (and possibly other grammar/scope errors). Merge into the final
   diagnostic vector at end of `check_source_file`.
2. **Move super validity checks out of type computation.** Add a
   dedicated AST visitor pass that walks `SuperKeyword` nodes from the
   top-level checker entrypoint — not from `get_type_of_super_keyword`
   — so emissions happen in non-speculative context. The dispatch in
   `crates/tsz-checker/src/dispatch.rs:470` would need a parallel
   `check_super_keyword_validity` call alongside the type computation.

Option 2 is cleaner architecturally (matches tsc's
`checkSuperExpression` which runs at check-time, not type-of-node time)
but bigger surgery. Option 1 is a smaller, well-scoped change and is
the recommended next-iteration target.

## Out of Scope for This PR

The full architectural fix above. Documented for resumption.

## Files Likely Involved

- `crates/tsz-checker/src/classes/super_checker.rs::check_super_expression`
- `crates/tsz-checker/src/types/computation/access_super.rs::get_type_of_super_keyword`
- `crates/tsz-checker/src/state/state.rs::get_type_of_node` super-sensitive
  fast path
- Possibly `crates/tsz-checker/src/classes/...` for inner-class member
  walking under static initializers.

## Verification

- Targeted unit test reproducing the minimal case above
- `./scripts/conformance/conformance.sh run --filter "typeOfThisInStaticMembers9" --verbose`
  — should flip from fingerprint-only to PASS
