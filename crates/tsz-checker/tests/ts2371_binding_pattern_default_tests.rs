//! TS2371: a parameter initializer is only allowed in a function or
//! constructor implementation. The default may be written *inside* a
//! destructuring binding pattern, in which case tsc still reports TS2371 at
//! the binding element. These tests cover ambient functions, interface and
//! type-literal method/call signatures, and function-type aliases, with
//! object/array/nested patterns and renamed variables to prove the rule is
//! structural rather than keyed on a specific spelling.

use tsz_checker::context::CheckerOptions;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn count_2371(source: &str) -> usize {
    diagnostic_codes(source)
        .into_iter()
        .filter(|&c| c == 2371)
        .count()
}

#[test]
fn object_binding_default_in_ambient_function() {
    let source = r#"declare function f({ mult = 1 }: { mult?: number }): void;"#;
    assert!(
        count_2371(source) >= 1,
        "expected TS2371 for object binding default in ambient function"
    );
}

#[test]
fn array_binding_default_in_ambient_function() {
    let source = r#"declare function f([a = 1]: number[]): void;"#;
    assert!(
        count_2371(source) >= 1,
        "expected TS2371 for array binding default in ambient function"
    );
}

#[test]
fn nested_object_binding_default_in_ambient_function() {
    let source = r#"declare function f({ a: { b = 2 } }: { a: { b?: number } }): void;"#;
    assert!(
        count_2371(source) >= 1,
        "expected TS2371 for nested binding default in ambient function"
    );
}

#[test]
fn interface_method_signature_binding_default() {
    let source = r#"interface I { m({ x = 1 }: { x?: number }): void }"#;
    assert!(
        count_2371(source) >= 1,
        "expected TS2371 for binding default in interface method signature"
    );
}

#[test]
fn type_literal_call_signature_binding_default() {
    let source = r#"type C = { ({ x = 1 }: { x?: number }): void };"#;
    assert!(
        count_2371(source) >= 1,
        "expected TS2371 for binding default in call signature"
    );
}

#[test]
fn function_type_alias_binding_default() {
    let source = r#"type FA = ({ first = 0 }: { first?: number }) => void;"#;
    assert!(
        count_2371(source) >= 1,
        "expected TS2371 for binding default in function-type alias"
    );
}

#[test]
fn rule_is_not_keyed_on_variable_name() {
    // Renaming the bound variable must not change the outcome.
    let source = r#"declare function f({ renamed = 9 }: { renamed?: number }): void;"#;
    assert!(
        count_2371(source) >= 1,
        "TS2371 must fire regardless of the binding variable's name"
    );
}

#[test]
fn multiple_binding_defaults_each_report() {
    let source = r#"declare function f({ a = 1, b = 2 }: { a?: number; b?: number }): void;"#;
    assert!(
        count_2371(source) >= 2,
        "each binding-element default should report its own TS2371"
    );
}

#[test]
fn top_level_default_still_reports() {
    // Control: the pre-existing top-level case must keep reporting.
    let source = r#"declare function f(x = 1): void;"#;
    assert!(
        count_2371(source) >= 1,
        "top-level parameter default must still report TS2371"
    );
}

#[test]
fn implementation_binding_default_is_allowed() {
    // Control: a real implementation legally allows the initializer.
    let source = r#"function f({ x = 1 }: { x?: number } = {}) { return x; }"#;
    assert_eq!(
        count_2371(source),
        0,
        "binding defaults are legal in a function implementation"
    );
}

#[test]
fn implementation_without_binding_default_no_false_positive() {
    // Control: no default at all -> no TS2371.
    let source = r#"declare function f({ x }: { x: number }): void;"#;
    assert_eq!(
        count_2371(source),
        0,
        "a destructured parameter without a default must not report TS2371"
    );
}
