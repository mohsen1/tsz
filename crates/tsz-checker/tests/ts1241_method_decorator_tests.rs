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
