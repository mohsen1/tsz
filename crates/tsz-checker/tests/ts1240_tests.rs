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
