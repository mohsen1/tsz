//! Tests for TS2558: Expected N type arguments, but got M (new expressions)

use crate::test_utils::check_source_codes as get_error_codes;

#[test]
fn test_new_too_many_type_args() {
    let codes = get_error_codes(
        r#"
class Foo<T> { x!: T; }
let a = new Foo<string, number>();
"#,
    );
    assert!(
        codes.contains(&2558),
        "Should emit TS2558 for too many type args in new expression, got: {codes:?}"
    );
}

#[test]
fn test_new_too_few_type_args() {
    let codes = get_error_codes(
        r#"
class Foo<T, U> { x!: T; y!: U; }
let a = new Foo<string>();
"#,
    );
    assert!(
        codes.contains(&2558),
        "Should emit TS2558 for too few type args in new expression, got: {codes:?}"
    );
}

#[test]
fn test_new_correct_type_args_no_error() {
    let codes = get_error_codes(
        r#"
class Foo<T> { x!: T; }
let a = new Foo<string>();
"#,
    );
    assert!(
        !codes.contains(&2558),
        "Should not emit TS2558 for correct type arg count, got: {codes:?}"
    );
}

#[test]
fn test_type_reference_too_many_type_args() {
    let codes = get_error_codes(
        r#"
interface Foo<T> { x: T; }
let a: Foo<string, number>;
"#,
    );
    // TSC emits TS2314 (not TS2558) for type reference arity mismatches
    assert!(
        codes.contains(&2314),
        "Should emit TS2314 for too many type args in type reference, got: {codes:?}"
    );
}

#[test]
fn test_type_reference_too_few_type_args() {
    let codes = get_error_codes(
        r#"
interface Foo<T, U> { x: T; y: U; }
let a: Foo<string>;
"#,
    );
    // TSC emits TS2314 (not TS2558) for type reference arity mismatches
    assert!(
        codes.contains(&2314),
        "Should emit TS2314 for too few type args in type reference, got: {codes:?}"
    );
}

#[test]
fn test_new_generic_class_in_static_method_no_false_ts2558() {
    let codes = get_error_codes(
        r#"
class Foo<T> {
    value!: T;
    static create(): Foo<number> {
        return new Foo<number>();
    }
}
"#,
    );
    assert!(
        !codes.contains(&2558),
        "Should not emit TS2558 for new Foo<number>() inside static method, got: {codes:?}"
    );
}

#[test]
fn test_new_generic_class_in_generic_static_method_no_false_ts2558() {
    let codes = get_error_codes(
        r#"
class Foo<T> {
    value!: T;
    static create<T>(): Foo<T> {
        return new Foo<T>();
    }
}
"#,
    );
    assert!(
        !codes.contains(&2558),
        "Should not emit TS2558 for new Foo<T>() inside generic static method, got: {codes:?}"
    );
}
