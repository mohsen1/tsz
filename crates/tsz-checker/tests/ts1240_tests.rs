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
