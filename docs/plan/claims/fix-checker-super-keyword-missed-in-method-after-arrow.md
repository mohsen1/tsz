# fix(checker): emit TS2335 for `super` in inner-class method body after sibling arrow uses `super`

- **Date**: 2026-04-29
- **Branch**: `fix/checker-super-keyword-missed-in-method-after-arrow`
- **PR**: TBD
- **Status**: claim
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

## Investigation Findings

- TS2335 is emitted from
  `crates/tsz-checker/src/classes/super_checker.rs::check_super_expression`,
  which is called from
  `crates/tsz-checker/src/types/computation/access_super.rs::get_type_of_super_keyword`.
- For [A], `get_type_of_super_keyword` is reached and the super-keyword
  type is computed. This populates `enclosing_class` and any per-super
  caching state during D's static-member checking.
- For [B], the inner method body is checked. Either the inner method's
  `super` keyword type is never computed (so `check_super_expression`
  is never called), OR the cached state from [A] suppresses the check.
- Hypothesis: the type-of-node cache short-circuit path in
  `crates/tsz-checker/src/state/state.rs:~1330` flags `is_super_keyword`
  as super-sensitive but the surrounding logic doesn't re-run
  `check_super_expression` on a cache hit. If [A] and [B] resolve to the
  same cached `TypeId::ERROR` (because no enclosing class for [A]'s arrow
  is "derived" enough to give [A] a real super type), the second access
  returns the cache without re-checking validity.
- Alternative hypothesis: the inner class C's method body is being
  visited under a different recursion guard that, in this nested static
  context, marks the inner method body as "already checked" before its
  body expressions are walked. The instance-property initializer path
  takes a different walk that isn't affected.

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
