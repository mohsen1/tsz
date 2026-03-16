//! Test for conditional expression literal preservation under generic keyof contexts.

use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_source_diagnostics;

#[test]
fn conditional_expression_union_assignable_to_keyof_constraint_has_no_ts2345() {
    let source = r#"
        interface Shape {
            name: string;
            width: number;
            height: number;
        }

        function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        function test(shape: Shape, cond: boolean) {
            let widthOrHeight = getProperty(shape, cond ? "width" : "height");
        }
    "#;

    let errors = check_source_diagnostics(source);
    let ts2345_errors: Vec<_> = errors
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    assert!(
        ts2345_errors.is_empty(),
        "Expected no TS2345 for conditional expression in generic keyof call, got: {ts2345_errors:?}"
    );
}

#[test]
fn nested_conditional_expression_union_assignable_to_keyof_constraint_has_no_ts2345() {
    let source = r#"
        type Point = { x: number; y: number; z: number };

        function pick<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        function test(p: Point, a: boolean, b: boolean) {
            let value = pick(p, a ? "x" : (b ? "y" : "z"));
        }
    "#;

    let errors = check_source_diagnostics(source);
    let ts2345_errors: Vec<_> = errors
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    assert!(
        ts2345_errors.is_empty(),
        "Expected no TS2345 for nested conditional expression in generic keyof call, got: {ts2345_errors:?}"
    );
}

#[test]fn string_literal_argument_assignable_to_keyof_this_has_no_ts2345() {
    let source = r#"
        class C {
            x: number = 0;

            get<K extends keyof this>(key: K) {
                return this[key];
            }

            set<K extends keyof this>(key: K, value: this[K]) {
                this[key] = value;
            }

            test() {
                this.get("x");
                this.set("x", 42);
            }
        }
    "#;

    let errors = check_source_diagnostics(source);
    let ts2345_errors: Vec<_> = errors
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    assert!(
        ts2345_errors.is_empty(),
        "Expected no TS2345 for string literal argument in keyof this call, got: {ts2345_errors:?}"
    );
}

#[test]
fn generic_indexed_access_assignable_to_empty_object_with_nullish_union() {
    let source = r#"
        interface I {
            foo: string;
        }

        declare function take<T>(p: T): void;

        function fn<T extends I, K extends keyof T>(o: T, k: K) {
            let a: {} | null | undefined;
            a = o[k];
            take<{} | null | undefined>(o[k]);
        }
    "#;

    let errors = check_source_diagnostics(source);
    let relevant: Vec<_> = errors
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || d.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no TS2322/TS2345 for generic indexed access into top-like union target, got: {relevant:?}"
    );
}
