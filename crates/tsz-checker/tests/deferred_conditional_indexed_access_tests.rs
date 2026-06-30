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
fn aliased_branch_index_into_deferred_conditional_is_valid() {
    // The branch results are named aliases, so the conditional branch-union key
    // space contains `Lazy(DefId)` members. `keyof` must resolve those aliases
    // before validating the index key.
    let src = "type LeftChoice = { shared: 1; leftOnly: 1 };\n\
               type RightChoice = { shared: 2; rightOnly: 2 };\n\
               type Choice<Input> = Input extends string ? LeftChoice : RightChoice;\n\
               type Shared<Subject> = Choice<Subject>['shared'];";
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

#[test]
fn aliased_branch_key_in_only_one_branch_still_reports_ts2536() {
    let src = "type WithExtra = { shared: 1; extra: 1 };\n\
               type WithoutExtra = { shared: 2 };\n\
               type PickBranch<Source> = Source extends string ? WithExtra : WithoutExtra;\n\
               type Bad<Other> = PickBranch<Other>['extra'];";
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

// ── #14159: indexed access / rest-spread of a conditional property whose
//    branch values are an inferred tuple tail (remeda `TupleParts`) ──
//
// `...infer Tail` in a tuple rest position has the inferred constraint
// `unknown[]` (tsc `getInferredTypeParameterConstraint`). When that captured
// tail is re-extracted through `Parts<T>["rest"]`, the constraint must survive
// so `["length"]`, numeric indexing, and rest-spread see an array. Without the
// constraint, `Tail` looked unconstrained and the apparent type `Tail | []`
// was flagged non-array (false TS2536/TS2574).

fn codes_nuia(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        no_unchecked_indexed_access: true,
        ..CheckerOptions::default()
    };
    diagnostic_codes(&check_source(source, "test.ts", options))
}

const PARTS: &str = "type Parts<T> = T extends readonly [infer _H, ...infer Tail] ? { rest: Tail } : { rest: [] };\n";

#[test]
fn conditional_tail_property_length_index_is_valid() {
    let src = format!("{PARTS}export type Len<T> = Parts<T>[\"rest\"][\"length\"];");
    let c = codes_nuia(&src);
    assert!(!c.contains(&2536), "TS2536 should not fire: {c:?}");
}

#[test]
fn conditional_tail_property_numeric_index_is_valid() {
    let src = format!("{PARTS}export type Elem<T> = Parts<T>[\"rest\"][number];");
    let c = codes_nuia(&src);
    assert!(
        !c.contains(&2536),
        "TS2536 should not fire for numeric index: {c:?}"
    );
}

#[test]
fn conditional_tail_property_rest_spread_is_valid() {
    let src = format!("{PARTS}export type Rebuilt<T> = [...Parts<T>[\"rest\"]];");
    let c = codes_nuia(&src);
    assert!(!c.contains(&2574), "TS2574 should not fire: {c:?}");
}

#[test]
fn conditional_tail_property_rest_spread_between_fixed_elements_is_valid() {
    // `[...A, ...Cond[K], ...B]` — the inferred-tail spread sits between other
    // spreads and must still be array-like.
    let src = format!("{PARTS}export type Wrap<T> = [boolean, ...Parts<T>[\"rest\"], string];");
    let c = codes_nuia(&src);
    assert!(!c.contains(&2574), "TS2574 should not fire: {c:?}");
}

#[test]
fn conditional_tail_property_renamed_binders_is_valid() {
    // Anti-hardcoding: vary every binder/property name; the rule is structural.
    let src = "type Split<Elems> = Elems extends readonly [infer Head, ...infer Body] ? { tail: Body } : { tail: [] };\n\
               export type A<Elems> = [...Split<Elems>[\"tail\"]];\n\
               export type B<Elems> = Split<Elems>[\"tail\"][\"length\"];";
    let c = codes_nuia(src);
    assert!(
        !c.contains(&2574) && !c.contains(&2536),
        "no TS2574/TS2536 expected: {c:?}"
    );
}

#[test]
fn conditional_tail_property_named_rest_member_is_valid() {
    // `...rest: infer Tail` (named tuple member rest) gets the same constraint.
    let src = "type P<T> = T extends readonly [infer _H, ...rest: infer Tail] ? { rest: Tail } : { rest: [] };\n\
               export type R<T> = [...P<T>[\"rest\"]];";
    let c = codes_nuia(src);
    assert!(!c.contains(&2574), "TS2574 should not fire: {c:?}");
}

#[test]
fn conditional_tail_property_value_index_access_is_valid() {
    // The value-position read was already accepted; keep it green as a guard.
    let src =
        format!("{PARTS}export function idx<T>(p: Parts<T>[\"rest\"]): unknown {{ return p[0]; }}");
    let c = codes_nuia(&src);
    assert!(
        !c.contains(&2536) && !c.contains(&2574),
        "no error expected: {c:?}"
    );
}

#[test]
fn conditional_non_array_tail_property_still_rejects_rest_spread() {
    // Negative control: when the conditional's property is NOT array-like in a
    // branch, the rest-spread must still report TS2574 (no over-acceptance).
    let src = "type P<T> = T extends string ? { rest: string } : { rest: number };\n\
               export type R<T> = [...P<T>[\"rest\"]];";
    let c = codes_nuia(src);
    assert!(
        c.contains(&2574),
        "TS2574 expected for non-array branch: {c:?}"
    );
}

#[test]
fn bare_rest_infer_constraint_does_not_leak_to_fixed_position() {
    // The `unknown[]` constraint applies only to the rest element, so a fixed
    // `infer _H` is unaffected and a non-rest `infer` reference is not array-like.
    let src = "type P<T> = T extends readonly [infer Head, ...infer _Tail] ? { head: Head } : { head: unknown };\n\
               export type R<T> = [...P<T>[\"head\"]];";
    let c = codes_nuia(src);
    assert!(
        c.contains(&2574),
        "TS2574 expected for non-array head property: {c:?}"
    );
}
