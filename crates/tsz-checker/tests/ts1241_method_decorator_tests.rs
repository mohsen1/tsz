//! Tests for TS1241 method/accessor decorator signature mismatch.
//!
//! Structural rule:
//! - Stage-3 method/accessor decorators are invoked as `(value, context)`.
//! - Legacy `experimentalDecorators` method/accessor decorators are invoked as
//!   `(target, propertyKey, descriptor)`.
//! - Zero-parameter decorator values keep TS1329 factory-hint priority.

use tsz_checker::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

#[test]
fn stage3_legacy_three_arg_method_decorator_emits_ts1241() {
    let codes = check_source_codes(
        r#"
function legacyDecorator(target: any, key: string, descriptor: PropertyDescriptor) {
    return descriptor;
}

class TestClass {
    @legacyDecorator
    method1(): string {
        return "test";
    }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "Stage-3 method decorators must reject legacy three-arg decorators; got: {codes:?}"
    );
}

#[test]
fn stage3_method_decorator_rejects_wrong_value_type() {
    let codes = check_source_codes(
        r#"
function wrongValue(value: string, context: ClassMethodDecoratorContext) {}

class Renamed {
    @wrongValue
    compute(): number {
        return 1;
    }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "Stage-3 method decorator must reject an incompatible value parameter; got: {codes:?}"
    );
}

#[test]
fn stage3_method_decorator_rejects_wrong_context_type() {
    let codes = check_source_codes(
        r#"
function wrongContext(value: any, context: string) {}

class Worker {
    @wrongContext
    run(): void {}
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "Stage-3 method decorator must reject an incompatible context parameter; got: {codes:?}"
    );
}

#[test]
fn stage3_method_decorator_accepts_value_and_context_shape() {
    let codes = check_source_codes(
        r#"
function logged<This>(
    value: (this: This) => string,
    context: ClassMethodDecoratorContext<This, (this: This) => string>
) {
    return value;
}

class Service {
    @logged
    label(): string {
        return "ok";
    }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1329),
        "Compatible stage-3 method decorator should not emit TS1241/TS1329; got: {codes:?}"
    );
}

#[test]
fn stage3_method_decorator_rejects_incompatible_replacement_return() {
    let codes = check_source_codes(
        r#"
function badReturn(value: () => string, context: ClassMethodDecoratorContext) {
    return 1;
}

class Replacement {
    @badReturn
    label(): string {
        return "";
    }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1270),
        "Stage-3 method decorator must reject incompatible replacement returns; got: {codes:?}"
    );
}

#[test]
fn stage3_zero_arg_method_decorator_keeps_ts1329_priority() {
    let codes = check_source_codes(
        r#"
function makeDecorator() {}

class Example {
    @makeDecorator
    method(): void {}
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1329),
        "Zero-arg method decorator should emit TS1329; got: {codes:?}"
    );
    assert!(
        !codes.contains(&1241),
        "TS1329 should suppress the generic TS1241 method-decorator failure; got: {codes:?}"
    );
}

#[test]
fn legacy_one_arg_method_decorator_emits_ts1241() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function one(value: string) {}

class Legacy {
    @one
    method(): string {
        return "";
    }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "Legacy method decorators must reject one-arg signatures that cannot accept the runtime ABI; got: {codes:?}"
    );
}

#[test]
fn legacy_three_arg_method_decorator_is_accepted() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function legacy(target: any, key: string, descriptor: TypedPropertyDescriptor<() => string>) {}

class LegacyOk {
    @legacy
    method(): string {
        return "";
    }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1329),
        "Compatible legacy method decorator should not emit TS1241/TS1329; got: {codes:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// `getLegacyDecoratorArgumentCount` parity: a 2-parameter legacy decorator
// factory must NOT emit TS1241 when applied to a method/get/set accessor.
//
// tsc adapts the supplied argument count to the decorator's parameter count
// (2 args if ≤ 2 params, otherwise 3). tsz previously always passed 3 args,
// producing a spurious TS1241 for the common `(target, key) => void` shape.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn legacy_two_arg_method_decorator_is_accepted() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function deco(target: object, key: PropertyKey) {}

class TwoArgs {
    @deco
    method(): void {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1329),
        "A 2-param legacy method decorator factory should be accepted by adapting argCount=2; got: {codes:?}"
    );
}

#[test]
fn legacy_two_arg_method_decorator_is_accepted_with_renamed_params() {
    // Same rule, different name choices — proves the fix is structural and
    // does not depend on the spelling of parameter names.
    let codes = check_source_codes_experimental_decorators(
        r#"
function pin(a: any, b: string) {}

class Renamed {
    @pin
    run(): void {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1329),
        "Renamed-parameter 2-arg legacy decorator should still be accepted; got: {codes:?}"
    );
}

#[test]
fn legacy_two_arg_decorator_on_get_accessor_is_accepted() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function deco(target: object, key: PropertyKey) {}

class Accessor {
    @deco
    get value(): string { return ""; }
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1329),
        "2-param legacy decorator should also be accepted on get accessors; got: {codes:?}"
    );
}

#[test]
fn legacy_two_arg_decorator_on_set_accessor_is_accepted() {
    let codes = check_source_codes_experimental_decorators(
        r#"
function deco(target: object, key: PropertyKey) {}

class Accessor {
    @deco
    set value(v: string) {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1329),
        "2-param legacy decorator should also be accepted on set accessors; got: {codes:?}"
    );
}

#[test]
fn legacy_method_decorator_with_computed_property_name_is_accepted() {
    // The original regression repro from conformance: a 2-param decorator on
    // a method with a dynamic computed name. tsc emits nothing here because
    // the decorator's 2 parameters match the adapted 2-arg call.
    let codes = check_source_codes_experimental_decorators(
        r#"
function x(o: object, k: PropertyKey) {}

class I {
    @x ["some" + "method"]() {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241),
        "A 2-param legacy decorator on a method with a dynamic computed name should not emit TS1241; got: {codes:?}"
    );
}

#[test]
fn legacy_rest_param_method_decorator_is_accepted() {
    // A rest-only signature `(...args: any[])` has `params.len() == 1`, so it
    // falls into the ≤ 2 bucket and is invoked with 2 args. Rest absorbs the
    // extras and the call succeeds — matching tsc.
    let codes = check_source_codes_experimental_decorators(
        r#"
function deco(...args: any[]) {}

class R {
    @deco
    method(): void {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241) && !codes.contains(&1329),
        "Rest-param legacy decorator should be accepted; got: {codes:?}"
    );
}

#[test]
fn legacy_one_arg_method_decorator_still_emits_ts1241() {
    // Boundary check: the argcount adaptation must not silently accept a
    // decorator whose signature is too narrow for the supplied call. A
    // 1-param decorator gets argCount=2 (since 1 ≤ 2) and 2 args overflow
    // its 1 parameter, so tsc emits TS1241 — tsz must too.
    let codes = check_source_codes_experimental_decorators(
        r#"
function narrow(only: any) {}

class TooFew {
    @narrow
    method(): void {}
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "A 1-param legacy method decorator should still emit TS1241 (argCount=2 overflows 1 param); got: {codes:?}"
    );
}

#[test]
fn legacy_bind_only_object_decorator_still_emits_ts1241() {
    // Negative guard for the `Function`-typed fallback: tsc's
    // `isUntypedFunctionCall` requires assignability to the full global
    // `Function` interface (apply/call/bind/toString/prototype/length/…),
    // not merely "has a `bind` member". An object like `{ bind: any }` is
    // not assignable to `Function` and must still emit TS1241.
    let codes = check_source_codes_experimental_decorators(
        r#"
const dec: { bind: any } = { bind: null as any };

class C {
    @dec
    method(): void {}
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "A `bind`-only object is not Function-assignable; TS1241 must still fire; got: {codes:?}"
    );
}

#[test]
fn legacy_function_typed_decorator_factory_is_accepted() {
    // tsc's `isUntypedFunctionCall` treats a `Function`-typed callee as
    // callable with any signature. Without the fallback, a decorator factory
    // that returns `Function` would produce a spurious TS1241.
    let codes = check_source_codes_experimental_decorators(
        r#"
function dec(): Function {
    return function (target: any, propKey: string, descr: PropertyDescriptor): void {};
}

class HasMethod {
    @dec()
    foo(bar: string): void {}
}
"#,
    )
    .to_vec();

    assert!(
        !codes.contains(&1241),
        "A decorator factory returning `Function` should be treated as an untyped call and not emit TS1241; got: {codes:?}"
    );
}

#[test]
fn stage3_getter_decorator_rejects_wrong_context_type() {
    let codes = check_source_codes(
        r#"
function wrongGetter(value: any, context: string) {}

class HasGetter {
    @wrongGetter
    get value(): string {
        return "";
    }
}
"#,
    )
    .to_vec();

    assert!(
        codes.contains(&1241),
        "Stage-3 getter decorator must reject an incompatible context parameter; got: {codes:?}"
    );
}
