//! A function-like source never satisfies a numeric index signature on the target.
//!
//! `tsc` treats a function value's apparent type as its call signatures plus the
//! members of the global `Function` interface. That apparent type carries no
//! numeric index signature, so a target declaring `[n: number]: T` is
//! unsatisfiable by a function no matter which other members the target also
//! requires — including the `call`/`apply` members a function genuinely does
//! provide.
//!
//! tsz answered this correctly only while the target required nothing else. A
//! target shaped `{ apply(..): any; [n: number]: T }` — exactly what a user
//! augmentation gives the global `Function` interface — took the `call`/`apply`
//! compatibility bridge and answered assignable, which silently swallowed both
//! the `TS2322` on a direct assignment and the `TS2430` on an interface that
//! extends such a shape.

use tsz_checker::test_utils::{check_source_code_messages as get_diagnostics, has_diagnostic_code};

fn has_error_with_code(source: &str, code: u32) -> bool {
    has_diagnostic_code(&get_diagnostics(source), code)
}

fn codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = get_diagnostics(source).iter().map(|d| d.0).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

// =========================================================================
// Direct assignment (TS2322): the relation itself
// =========================================================================

#[test]
fn function_is_not_assignable_to_numeric_index_target_that_also_requires_apply() {
    let source = r#"
interface Bar { b: number; }
declare var target: { apply(x: any): any; [n: number]: Bar };
declare var source: (x: any) => void;
target = source;
"#;
    assert!(
        has_error_with_code(source, 2322),
        "a numeric index signature is unsatisfiable by a function even when the \
         target's only other required member is `apply`"
    );
}

#[test]
fn function_is_not_assignable_to_numeric_index_target_that_also_requires_call() {
    let source = r#"
interface Bar { b: number; }
declare var target: { call(x: any): any; [n: number]: Bar };
declare var source: (x: any) => void;
target = source;
"#;
    assert!(
        has_error_with_code(source, 2322),
        "same verdict through the `call` spelling of the bridge"
    );
}

#[test]
fn function_is_not_assignable_to_bare_numeric_index_target() {
    // The pre-existing case, pinned so the widened rule keeps it.
    let source = r#"
interface Bar { b: number; }
declare var target: { [n: number]: Bar };
declare var source: (x: any) => void;
target = source;
"#;
    assert!(has_error_with_code(source, 2322));
}

#[test]
fn function_stays_assignable_to_an_apply_only_target() {
    // The bridge itself is correct and must survive: with no index signature in
    // play, a function does provide `apply`/`call`.
    let source = r#"
declare var target: { apply(x: any): any };
declare var source: (x: any) => void;
target = source;
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "the call/apply compatibility bridge must keep answering assignable"
    );
}

#[test]
fn function_stays_assignable_to_a_call_only_target() {
    let source = r#"
declare var target: { call(x: any): any };
declare var source: (x: any) => void;
target = source;
"#;
    assert_eq!(codes(source), Vec::<u32>::new());
}

// =========================================================================
// Interface heritage (TS2430): the role the corpus rows fail in
// =========================================================================

#[test]
fn derived_this_parameter_conflicting_with_a_numeric_indexed_base_is_ts2430() {
    // The reduced form of lib.es5.d.ts's `CallableFunction extends Function`
    // once a user augments `interface Function` with a numeric index signature:
    // the base member's `this` type is the base interface itself, so the
    // inherited index signature decides the override's compatibility.
    let source = r#"
interface Bar { b: number; }
interface Base {
    [n: number]: Bar;
    apply(this: Base, thisArg: any): any;
}
interface Derived extends Base {
    apply(this: () => void, thisArg: any): any;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "the derived `this: () => void` cannot satisfy a base whose own type \
         carries a numeric index signature"
    );
}

#[test]
fn renamed_binders_reach_the_same_ts2430_verdict() {
    // Binder-name variation: nothing about this rule may key on the member name
    // `apply` or on an interface literally called `Function`.
    let source = r#"
interface Element { b: number; }
interface Zed {
    [n: number]: Element;
    handler(this: Zed, thisArg: any): any;
}
interface Yard extends Zed {
    handler(this: () => void, thisArg: any): any;
}
"#;
    assert!(has_error_with_code(source, 2430));
}

#[test]
fn generic_derived_override_reaches_the_same_ts2430_verdict() {
    // The lib shape is generic (`apply<T, R>(this: (this: T) => R, ..)`); the
    // concrete form above and this one must agree.
    let source = r#"
interface Bar { b: number; }
interface Base {
    [n: number]: Bar;
    apply(this: Base, thisArg: any, argArray?: any): any;
}
interface Derived extends Base {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
}
"#;
    assert!(has_error_with_code(source, 2430));
}

#[test]
fn an_alias_to_the_indexed_base_reaches_the_same_ts2430_verdict() {
    let source = r#"
interface Bar { b: number; }
interface Base {
    [n: number]: Bar;
    apply(this: Base, thisArg: any): any;
}
type BaseAlias = Base;
interface Derived extends Base {
    apply(this: () => void, thisArg: any): any;
}
declare var pinned: BaseAlias;
"#;
    assert!(has_error_with_code(source, 2430));
}

#[test]
fn a_base_without_a_numeric_index_keeps_the_override_compatible() {
    // The negative that localizes the rule: drop only the index signature and
    // the bivariant `this` comparison accepts the override, as it did before.
    let source = r#"
interface Base {
    apply(this: Base, thisArg: any): any;
}
interface Derived extends Base {
    apply(this: () => void, thisArg: any): any;
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "without the inherited numeric index there is nothing for the override \
         to violate"
    );
}

#[test]
fn a_string_index_on_the_base_keeps_the_override_compatible() {
    // tsc allows a function against a permissive string index (its apparent
    // members satisfy it), so the numeric rule must not generalize to `string`.
    let source = r#"
interface Base {
    [k: string]: any;
    apply(this: Base, thisArg: any): any;
}
interface Derived extends Base {
    apply(this: () => void, thisArg: any): any;
}
"#;
    assert!(!has_error_with_code(source, 2430));
}

#[test]
fn a_compatible_override_under_a_numeric_indexed_base_stays_silent() {
    let source = r#"
interface Bar { b: number; }
interface Base {
    [n: number]: Bar;
    apply(this: Base, thisArg: any): any;
}
interface Derived extends Base {
    apply(this: Base, thisArg: any): any;
}
"#;
    assert!(!has_error_with_code(source, 2430));
}

// =========================================================================
// Full Function surface (apply + call + bind): the shape a real
// `interface Function` augmentation produces. #16473 fixed the callable and
// object arms of the bridge, but a target carrying the *complete* Function
// method trio matches `is_function_interface_structural` and took an earlier
// fast-path (`core_dispatch`'s `is_function_target` and the subtype visitor's
// `visit_function`/`visit_callable` arms) that answered `True` before the
// numeric-index verdict. This is exactly `CallableFunction extends Function`
// once `Function` gains `[n: number]: T`.
// =========================================================================

#[test]
fn function_is_not_assignable_to_a_numeric_index_target_with_the_full_function_surface() {
    // Direct assignment (TS2322): the target has apply AND call AND bind plus a
    // numeric index. The complete surface must not let the function bypass the
    // numeric-index rejection.
    let source = r#"
interface Bar { b: number; }
declare var target: {
    apply(x: any): any;
    call(x: any): any;
    bind(x: any): any;
    [n: number]: Bar;
};
declare var source: (x: any) => void;
target = source;
"#;
    assert!(
        has_error_with_code(source, 2322),
        "a function does not satisfy a numeric index even when the target also \
         declares the full apply/call/bind surface"
    );
}

#[test]
fn the_full_function_surface_without_a_numeric_index_still_accepts_a_function() {
    // The bridge itself must survive: a target with apply/call/bind and no index
    // signature still accepts a function.
    let source = r#"
declare var target: {
    apply(x: any): any;
    call(x: any): any;
    bind(x: any): any;
};
declare var source: (x: any) => void;
target = source;
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "with no numeric index in play the function-interface bridge still holds"
    );
}

#[test]
fn dual_any_index_still_waives_the_full_function_surface_target() {
    // The dual-`any`-index waiver `check_number_index_compatibility` encodes must
    // survive here too: a co-present `any`-valued string index waives the numeric
    // requirement, so a function is still accepted.
    let source = r#"
declare var target: {
    apply(x: any): any;
    call(x: any): any;
    bind(x: any): any;
    [k: string]: any;
    [n: number]: any;
};
declare var source: (x: any) => void;
target = source;
"#;
    assert_eq!(
        codes(source),
        Vec::<u32>::new(),
        "a dual-any-index target waives the missing numeric index for a function \
         source, matching tsc's indexSignaturesRelatedTo short-circuit"
    );
}

#[test]
fn a_derived_override_under_a_full_function_surface_base_is_ts2430() {
    // Interface heritage (TS2430): `MyCallable extends MyFunction` where
    // `MyFunction` carries the full apply/call/bind surface plus a numeric index
    // — the user-space form of `CallableFunction extends Function`. The derived
    // `this: (function)` override cannot satisfy the numeric-indexed base.
    let source = r#"
interface Bar { b: number; }
interface MyFunction {
    apply(this: MyFunction, thisArg: any, argArray?: any): any;
    call(this: MyFunction, thisArg: any, ...argArray: any[]): any;
    bind(this: MyFunction, thisArg: any, ...argArray: any[]): any;
    prototype: any;
    readonly length: number;
    [n: number]: Bar;
}
interface MyCallableFunction extends MyFunction {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
    apply<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, args: A): R;
    call<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, ...args: A): R;
    bind<T>(this: T, thisArg: any): any;
}
"#;
    assert!(
        has_error_with_code(source, 2430),
        "the derived function-typed `this` override cannot satisfy a base whose \
         own type carries a numeric index signature"
    );
}

#[test]
fn a_full_function_surface_base_without_a_numeric_index_keeps_the_override_compatible() {
    // Negative that localizes the rule to the numeric index: drop only the index
    // and the override is accepted, so nothing keys on the apply/call/bind surface
    // itself.
    let source = r#"
interface MyFunction {
    apply(this: MyFunction, thisArg: any, argArray?: any): any;
    call(this: MyFunction, thisArg: any, ...argArray: any[]): any;
    bind(this: MyFunction, thisArg: any, ...argArray: any[]): any;
    prototype: any;
    readonly length: number;
}
interface MyCallableFunction extends MyFunction {
    apply<T, R>(this: (this: T) => R, thisArg: T): R;
    call<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, ...args: A): R;
    bind<T>(this: T, thisArg: any): any;
}
"#;
    assert!(
        !has_error_with_code(source, 2430),
        "without the inherited numeric index the function-surface override is valid"
    );
}
