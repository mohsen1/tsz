//! Tests for `this:`-typed decorators applied via property/element access
//! (#8717 / #7681 `esDecorators-preservesThis.ts`).
//!
//! Structural rule: when a decorator expression is a property or element
//! access (after unwrapping parens, non-null assertions, and type
//! assertions), the call-signature validation must bind the decorator
//! function's `this:` parameter to the type of the receiver expression —
//! exactly as a regular method call (`obj.fn()`) does.
//!
//! Without this binding, decorators declared with an explicit `this:`
//! constraint (e.g. `decorate<T>(this: DecoratorProvider, v: T, ctx): T`)
//! fail the call signature check because the default `void` receiver is
//! not assignable to the declared `this:` type, producing spurious
//! TS1240/TS1241 (and a follow-on TS1270 from the recovery path).

use tsz_checker::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

#[test]
fn legacy_method_decorator_uses_descriptor_type_for_generic_return() {
    let codes = check_source_codes_experimental_decorators(
        r#"
declare const decorator: MethodDecorator;

class A {
    @decorator
    async foo() {}

    @decorator
    async bar(): Promise<number> { return 0; }

    @decorator
    baz(n: Promise<number>): Promise<number> { return n; }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1270),
        "Legacy `MethodDecorator` should infer the descriptor generic from the actual member descriptor and avoid TS1270; got: {codes:?}"
    );
}

#[test]
fn property_access_decorator_binds_this_for_method() {
    // Faithful reproduction of `esDecorators-preservesThis.ts` (Stage-3).
    let codes = check_source_codes(
        r#"
declare class DecoratorProvider {
    decorate<T>(this: DecoratorProvider, v: T, ctx: ClassMethodDecoratorContext): T;
}

declare const instance: DecoratorProvider;

class C {
    @instance.decorate
    method1() { }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1240) && !codes.contains(&1270),
        "Property-access decorator with matching `this:` must not emit TS1240/TS1241/TS1270; got: {codes:?}"
    );
}

#[test]
fn element_access_decorator_binds_this_for_method() {
    let codes = check_source_codes(
        r#"
declare class DecoratorProvider {
    decorate<T>(this: DecoratorProvider, v: T, ctx: ClassMethodDecoratorContext): T;
}

declare const instance: DecoratorProvider;

class C {
    @(instance["decorate"])
    method2() { }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1240) && !codes.contains(&1270),
        "Element-access decorator must bind `this:` to receiver; got: {codes:?}"
    );
}

#[test]
fn parenthesized_property_access_decorator_binds_this_for_method() {
    let codes = check_source_codes(
        r#"
declare class DecoratorProvider {
    decorate<T>(this: DecoratorProvider, v: T, ctx: ClassMethodDecoratorContext): T;
}

declare const instance: DecoratorProvider;

class C {
    @((instance.decorate))
    method3() { }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1240) && !codes.contains(&1270),
        "Parenthesized property-access decorator must bind `this:` past parens; got: {codes:?}"
    );
}

#[test]
fn super_property_access_decorator_binds_this_for_method() {
    // The `super.decorate` form: receiver is `super`, whose type is the
    // base class instance type. Use a renamed identifier (`Provider` ≠
    // `DecoratorProvider`) so the test pins the rule, not a spelling.
    let codes = check_source_codes(
        r#"
declare class Provider {
    deco<T>(this: Provider, v: T, ctx: ClassMethodDecoratorContext): T;
}

class D extends Provider {
    m() {
        class C {
            @(super.deco)
            method1() { }

            @(super["deco"])
            method2() { }

            @((super.deco))
            method3() { }
        }
    }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1240) && !codes.contains(&1270),
        "super.<deco> must bind `this:` to base class instance type across PROPERTY/ELEMENT/PAREN access; got: {codes:?}"
    );
}

#[test]
fn full_es_decorators_preserves_this_repro_emits_no_diagnostics() {
    // The exact conformance fixture from #7681 / #8717.
    let codes = check_source_codes(
        r#"
declare class DecoratorProvider {
    decorate<T>(this: DecoratorProvider, v: T, ctx: ClassMethodDecoratorContext): T;
}

declare const instance: DecoratorProvider;

class C {
    @instance.decorate
    method1() { }

    @(instance["decorate"])
    method2() { }

    @((instance.decorate))
    method3() { }
}

class D extends DecoratorProvider {
    m() {
        class C {
            @(super.decorate)
            method1() { }

            @(super["decorate"])
            method2() { }

            @((super.decorate))
            method3() { }
        }
    }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1240) && !codes.contains(&1270),
        "Conformance fixture esDecorators-preservesThis.ts must produce no decorator-signature diagnostics; got: {codes:?}"
    );
}

#[test]
fn renamed_provider_axis_property_access_decorator_binds_this() {
    // Rename axis: prove the fix is structural, not keyed on
    // `DecoratorProvider` / `instance` / `decorate` spellings (CLAUDE.md §25).
    let codes = check_source_codes(
        r#"
declare class Lib {
    apply<V>(this: Lib, value: V, context: ClassMethodDecoratorContext): V;
}

declare const lib: Lib;

class Renamed {
    @lib.apply
    invoke() { }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1240) && !codes.contains(&1270),
        "Renamed identifiers must not regress the `this:` binding for property-access decorators; got: {codes:?}"
    );
}

#[test]
fn property_access_decorator_with_wrong_receiver_still_emits_ts1241() {
    // Negative guard: when the receiver type is NOT assignable to the
    // decorator's `this:` constraint, the call signature check must still
    // fail with TS1241 — matching tsc parity for the wrong-receiver case.
    // Uses `this: string` on a deliberately object-shaped receiver so the
    // mismatch cannot be "absorbed" by structural compatibility.
    let codes = check_source_codes(
        r#"
declare const fnWithThis: <V>(this: string, value: V, context: ClassMethodDecoratorContext) => V;
declare const other: { name: number, apply: typeof fnWithThis };

class C {
    @other.apply
    m() { }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "A decorator applied via property access on a receiver that does not match the declared `this:` must still emit TS1241; got: {codes:?}"
    );
}

#[test]
fn bare_identifier_decorator_with_this_constraint_still_emits_ts1241() {
    // Negative guard: a decorator with `this:` invoked as a bare identifier
    // has no receiver, so `actual_this_type` stays `None` and the call
    // signature check falls through to the existing `void` check — which
    // tsc also rejects.
    let codes = check_source_codes(
        r#"
declare const apply: <V>(this: { name: string }, value: V, context: ClassMethodDecoratorContext) => V;

class C {
    @apply
    m() { }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "Bare-identifier decorator with `this:` constraint must still emit TS1241; got: {codes:?}"
    );
}

#[test]
fn property_access_decorator_binds_this_for_field() {
    // Stage-3 field decorators use a different first-argument shape
    // (undefined), but the `this:` binding rule is the same.
    let codes = check_source_codes(
        r#"
declare class Lib {
    apply(this: Lib, value: undefined, context: ClassFieldDecoratorContext): void;
}

declare const lib: Lib;

class C {
    @lib.apply
    field = 1;
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1240) && !codes.contains(&1241) && !codes.contains(&1270),
        "Property-access decorator on a field must bind `this:` to receiver; got: {codes:?}"
    );
}

#[test]
fn property_access_decorator_binds_this_for_getter() {
    let codes = check_source_codes(
        r#"
declare class Lib {
    apply<V>(this: Lib, value: () => V, context: ClassGetterDecoratorContext): () => V;
}

declare const lib: Lib;

class C {
    @lib.apply
    get prop(): number { return 1; }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1240) && !codes.contains(&1241) && !codes.contains(&1270),
        "Property-access decorator on a getter must bind `this:` to receiver; got: {codes:?}"
    );
}

#[test]
fn property_access_decorator_binds_this_for_setter() {
    let codes = check_source_codes(
        r#"
declare class Lib {
    apply<V>(this: Lib, value: (v: V) => void, context: ClassSetterDecoratorContext): (v: V) => void;
}

declare const lib: Lib;

class C {
    @lib.apply
    set prop(v: number) {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1240) && !codes.contains(&1241) && !codes.contains(&1270),
        "Property-access decorator on a setter must bind `this:` to receiver; got: {codes:?}"
    );
}

#[test]
fn property_access_decorator_binds_this_for_legacy_parameter() {
    // Legacy decorator path (experimentalDecorators): the same receiver
    // binding rule should apply to parameter decorators with `this:` too.
    let codes = check_source_codes_experimental_decorators(
        r#"
declare class Lib {
    apply(this: Lib, target: any, key: string, idx: number): void;
}

declare const lib: Lib;

class C {
    m(@lib.apply arg: number) {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1239) && !codes.contains(&1240) && !codes.contains(&1241),
        "Legacy parameter decorator must bind `this:` to property-access receiver; got: {codes:?}"
    );
}

#[test]
fn property_access_decorator_binds_this_for_legacy_property() {
    let codes = check_source_codes_experimental_decorators(
        r#"
declare class Lib {
    apply(this: Lib, target: any, key: string): void;
}

declare const lib: Lib;

class C {
    @lib.apply
    prop: number = 1;
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1239) && !codes.contains(&1240) && !codes.contains(&1241),
        "Legacy property decorator must bind `this:` to property-access receiver; got: {codes:?}"
    );
}

#[test]
fn type_assertion_around_property_access_decorator_binds_this() {
    // The receiver is unwrapped past `as Lib` so the binding still works.
    let codes = check_source_codes(
        r#"
declare class Lib {
    apply<V>(this: Lib, value: V, context: ClassMethodDecoratorContext): V;
}

declare const lib: unknown;

class C {
    @((lib as Lib).apply)
    m() {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1240) && !codes.contains(&1241) && !codes.contains(&1270),
        "Decorator with type-assertion wrapper around receiver must still bind `this:`; got: {codes:?}"
    );
}

#[test]
fn non_null_assertion_around_property_access_decorator_binds_this() {
    let codes = check_source_codes(
        r#"
declare class Lib {
    apply<V>(this: Lib, value: V, context: ClassMethodDecoratorContext): V;
}

declare const lib: Lib | undefined;

class C {
    @(lib!.apply)
    m() {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1240) && !codes.contains(&1241) && !codes.contains(&1270),
        "Decorator with non-null-assertion wrapper must still bind `this:`; got: {codes:?}"
    );
}
