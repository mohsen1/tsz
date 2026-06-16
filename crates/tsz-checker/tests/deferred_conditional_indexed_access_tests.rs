//! Regression coverage for #13654: indexed access / assertion comparability on a
//! deferred conditional base.
//!
//! Structural rule: when the base of an indexed access (or the source of an `as`
//! / comparability check) is a *deferred conditional* (or a generic application
//! whose body is a conditional with an unresolved check type), `tsc` validates
//! the index key / assertion against `getBaseConstraintOfType` — the union of
//! both branch result constraints. The conditional stays deferred so a later
//! concrete instantiation still resolves to the selected branch; only the
//! key/overlap *validation* uses the branch union.
//!
//! Before the fix, the checker let a concrete-literal (or literal-union) index
//! key defeat the deferred-object `TS2536` suppression, and the solver had no
//! branch-union base constraint for a deferred conditional, so:
//! - `C<T>['x']` emitted a false `TS2536`,
//! - `Box<T>[keyof Box<T>] as string` emitted a false `TS2352`.
//!
//! Verified against `tsc` 6.0.3 (all positive cases exit 0; the missing-key and
//! key-in-only-one-branch cases still report `TS2536`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source, diagnostic_codes};

fn codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    diagnostic_codes(&check_source(source, "test.ts", options))
}

fn count(source: &str, code: u32) -> usize {
    codes(source).into_iter().filter(|&c| c == code).count()
}

// ── Indexed access into a deferred conditional ──

#[test]
fn concrete_literal_index_into_deferred_conditional_is_valid() {
    // 'x' is a key of both branch results, so it is a key of the branch-union
    // base constraint `{ x: 1; y: 2 } | { x: 3; y: 4 }`.
    let src = "type C<T> = T extends string ? { x: 1; y: 2 } : { x: 3; y: 4 };\n\
               type X<T> = C<T>['x'];";
    assert_eq!(count(src, 2536), 0, "no TS2536 expected: {:?}", codes(src));
}

#[test]
fn literal_union_index_into_deferred_conditional_is_valid() {
    let src = "type C<T> = T extends string ? { x: 1; y: 2 } : { x: 3; y: 4 };\n\
               type X<T> = C<T>['x' | 'y'];";
    assert_eq!(count(src, 2536), 0, "no TS2536 expected: {:?}", codes(src));
}

#[test]
fn renamed_binders_index_into_deferred_conditional_is_valid() {
    // Binder names vary; the structural rule must not depend on identifiers.
    let src = "type Cond<Foo> = Foo extends number ? { p: 'a' } : { p: 'b' };\n\
               type Pick1<Bar> = Cond<Bar>['p'];";
    assert_eq!(count(src, 2536), 0, "no TS2536 expected: {:?}", codes(src));
}

#[test]
fn non_distributive_conditional_index_is_valid() {
    // `[T] extends [string]` is non-distributive; the branch-union rule still
    // applies.
    let src = "type C<T> = [T] extends [string] ? { x: 1 } : { x: 3 };\n\
               type X<T> = C<T>['x'];";
    assert_eq!(count(src, 2536), 0, "no TS2536 expected: {:?}", codes(src));
}

#[test]
fn never_false_branch_infer_value_index_is_valid() {
    // trpc `inferAsyncIterable` shape: false branch is `never`, true branch has
    // `infer` value types. The key set is still concrete.
    let src = "interface AIter<Y, R = any, N = any> { __y: Y; __r: R; __n: N }\n\
               type Infer<T> = T extends AIter<infer Y, infer R, infer N> ? { yield: Y; return: R; next: N } : never;\n\
               type Yld<T> = Infer<T>['yield'];\n\
               type Ret<T> = Infer<T>['return'];\n\
               type Nxt<T> = Infer<T>['next'];";
    assert_eq!(count(src, 2536), 0, "no TS2536 expected: {:?}", codes(src));
}

#[test]
fn recursive_conditional_index_is_valid() {
    // tanstack-router `ParsePathParams` shape: recursive template-literal split
    // chained through nested conditional false branches. The branch-union must
    // flatten nested conditionals (bounded by the fuel guard) so `'param'`
    // validates.
    let src = "type ParsePathParams<T extends string> =\n\
               T extends `${string}/$${infer Param}/${infer Rest}`\n\
               ? { param: Param; rest: ParsePathParams<Rest> }\n\
               : T extends `${string}/$${infer Param}`\n\
               ? { param: Param; rest: never }\n\
               : { param: never; rest: never };\n\
               type FirstParam<T extends string> = ParsePathParams<T>['param'];";
    assert_eq!(count(src, 2536), 0, "no TS2536 expected: {:?}", codes(src));
}

#[test]
fn keyof_index_into_deferred_conditional_still_passes() {
    // Control: `keyof C<T>` and `C<T>[keyof C<T>]` already exit 0; the fix must
    // not regress this path.
    let src = "type C<T> = T extends string ? { x: 1; y: 2 } : { x: 3; y: 4 };\n\
               type K<T> = keyof C<T>;\n\
               type ByKeyof<T> = C<T>[keyof C<T>];";
    assert_eq!(count(src, 2536), 0, "no TS2536 expected: {:?}", codes(src));
}

// ── Negative cases: the key must still be rejected when missing ──

#[test]
fn missing_key_into_deferred_conditional_still_reports_ts2536() {
    // 'z' is a key of neither branch result.
    let src = "type C<T> = T extends string ? { x: 1; y: 2 } : { x: 3; y: 4 };\n\
               type Bad<T> = C<T>['z'];";
    assert_eq!(count(src, 2536), 1, "TS2536 expected: {:?}", codes(src));
}

#[test]
fn key_in_only_one_branch_still_reports_ts2536() {
    // 'y' is in the true branch only; the union's key space (intersection of
    // member keys) excludes it, matching tsc.
    let src = "type C<T> = T extends string ? { x: 1; y: 2 } : { x: 3 };\n\
               type Partial1<T> = C<T>['y'];";
    assert_eq!(count(src, 2536), 1, "TS2536 expected: {:?}", codes(src));
}

// ── Assertion / comparability over a deferred conditional ──

#[test]
fn assertion_of_deferred_conditional_indexed_result_is_valid() {
    // tanstack-router `Matches.ts:96` shape: the indexed-access result of a
    // deferred conditional is string-domain, so `as string` overlaps.
    let src = "type Box<T> = T extends string ? { a: T } : { a: string };\n\
               type Member<T> = Box<T>[keyof Box<T>];\n\
               export const h = <T,>(v: Member<T>) => (v as string);";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}

#[test]
fn assertion_into_deferred_conditional_is_valid() {
    // Casting-into sibling: the source object overlaps the deferred conditional's
    // branch-union base constraint.
    let src = "type Box<T> = T extends string ? { a: T } : { a: string };\n\
               export const g = <T,>(b: Box<T>) => (b as { a: string });";
    assert_eq!(count(src, 2352), 0, "no TS2352 expected: {:?}", codes(src));
}
