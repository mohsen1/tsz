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
