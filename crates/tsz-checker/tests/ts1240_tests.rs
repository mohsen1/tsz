//! Tests for TS1240: unable to resolve a property decorator signature.

#[test]
fn ts1240_es_field_decorator_rejects_non_undefined_value_parameter() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
interface ClassFieldDecoratorContext<T = unknown, V = unknown> {}

function bound<T, V extends (this: T, ...args: any[]) => any>(
    _target: V,
    _context: ClassFieldDecoratorContext<T, V>
) {
    return function (this: T, initialValue: V): V {
        return initialValue;
    };
}

class Button {
    @bound
    handleClick = () => {};
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Expected TS1240 for ES field decorator value mismatch, got: {codes:?}"
    );
}

#[test]
fn ts1240_es_field_decorator_accepts_undefined_value_parameter() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
function field(_value: undefined, _context: any) {}

class Button {
    @field
    handleClick = () => {};
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Should not emit TS1240 for a compatible ES field decorator, got: {codes:?}"
    );
}

#[test]
fn ts1240_es_field_decorator_accepts_union_of_callable_decorators() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
function dec1(_value: undefined, _context: any) {}
function dec2(_value: undefined, _context: any) {}
declare const cond: boolean;
const dec = cond ? dec1 : dec2;

class Button {
    @dec
    handleClick = () => {};
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Should not emit TS1240 for a union of compatible callable decorators, got: {codes:?}"
    );
}

#[test]
fn ts1240_es_field_decorator_still_rejects_non_callable_decorator() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
const dec = 1;

class Button {
    @dec
    handleClick = () => {};
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Expected TS1240 for a non-callable field decorator, got: {codes:?}"
    );
}

// --- accessor (ES decorator) tests ---

/// Repro from issue #6397: a correctly-typed accessor decorator must not emit TS1240.
/// The rule: when `accessor` keyword is present, the decorator receives a
/// `ClassAccessorDecoratorTarget`-like object as first arg, never `undefined`.
#[test]
fn ts1240_es_accessor_decorator_with_correct_target_type_no_false_positive() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}
interface ClassAccessorDecoratorContext {}
interface ClassAccessorDecoratorResult<Value> {
    get?(): Value;
    set?(value: Value): void;
}

function logged<This, Value>(
    target: ClassAccessorDecoratorTarget<This, Value>,
    context: ClassAccessorDecoratorContext,
): ClassAccessorDecoratorResult<Value> | undefined {
    return undefined;
}

class WithAccessor {
    @logged
    accessor count = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Should not emit TS1240 for a correctly-typed accessor decorator, got: {codes:?}"
    );
}

/// Variant: rename type parameters — fix must not be keyed on parameter names.
#[test]
fn ts1240_es_accessor_decorator_renamed_type_params_no_false_positive() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<C, V> {
    get(this: C): V;
    set(this: C, value: V): void;
}
interface ClassAccessorDecoratorContext {}
interface ClassAccessorDecoratorResult<V> {}

function trace<C, V>(
    _target: ClassAccessorDecoratorTarget<C, V>,
    _ctx: ClassAccessorDecoratorContext,
): ClassAccessorDecoratorResult<V> | void {}

class WithAccessor {
    @trace
    accessor value = "hello";
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Should not emit TS1240 regardless of type-parameter naming, got: {codes:?}"
    );
}

/// `any` as the first parameter is always compatible because the solver uses `ANY` as
/// the conservative accessor-target argument — so `(any, any)` decorators must pass.
#[test]
fn ts1240_es_accessor_decorator_any_target_no_false_positive() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
function mark(_target: any, _ctx: any): void {}

class C {
    @mark
    accessor x = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Should not emit TS1240 for (any, any) accessor decorator, got: {codes:?}"
    );
}

/// Static and instance `accessor` members share the same calling convention
/// (`ClassAccessorDecoratorTarget` as first arg), so both should be accepted.
#[test]
fn ts1240_es_static_accessor_decorator_no_false_positive() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<C, V> {
    get(this: C): V;
    set(this: C, value: V): void;
}
interface ClassAccessorDecoratorContext {}

function dec<C, V>(
    _target: ClassAccessorDecoratorTarget<C, V>,
    _ctx: ClassAccessorDecoratorContext,
): void {}

class C {
    @dec
    static accessor count = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Should not emit TS1240 for a static accessor decorator, got: {codes:?}"
    );
}

/// A non-callable value applied as an accessor decorator still triggers TS1240.
#[test]
fn ts1240_es_accessor_decorator_non_callable_still_rejected() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
const dec = 42;

class C {
    @dec
    accessor x = 0;
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Expected TS1240 for a non-callable accessor decorator, got: {codes:?}"
    );
}

/// Field decorator that rejects non-undefined must still emit TS1240 on a plain field.
/// Ensure the accessor fix does not regress plain field checking.
#[test]
fn ts1240_es_field_decorator_not_affected_by_accessor_fix() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<C, V> {
    get(this: C): V;
    set(this: C, value: V): void;
}
interface ClassAccessorDecoratorContext {}

function dec<C, V>(
    _target: ClassAccessorDecoratorTarget<C, V>,
    _ctx: ClassAccessorDecoratorContext,
): void {}

class C {
    @dec
    x = 0;
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Should still emit TS1240 when an accessor-typed decorator is applied to a plain field, got: {codes:?}"
    );
}
