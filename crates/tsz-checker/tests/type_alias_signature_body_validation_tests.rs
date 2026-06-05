//! Guards for type-alias bodies whose type literal contains only signatures.

use tsz_checker::test_utils::check_source_code_messages;

fn codes(source: &str) -> Vec<u32> {
    check_source_code_messages(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

#[test]
fn signature_only_type_literal_alias_reports_rest_parameter_type_error() {
    let diagnostics = codes(
        r#"
type Callable = {
    (...args: string): void;
};
"#,
    );

    assert!(
        diagnostics.contains(&2370),
        "expected TS2370 for rest parameter in call signature, got {diagnostics:?}"
    );
}

#[test]
fn signature_only_type_literal_alias_reports_missing_type_names() {
    let diagnostics = codes(
        r#"
type Callable = {
    <Item>(value: MissingBox<Item>): Item;
};
"#,
    );

    assert!(
        diagnostics.contains(&2304),
        "expected TS2304 for missing type in call signature, got {diagnostics:?}"
    );
}

#[test]
fn signature_only_type_literal_alias_reports_type_argument_constraint_errors() {
    let diagnostics = codes(
        r#"
type Box<Item extends string> = Item;
type Callable = {
    (value: Box<number>): void;
};
"#,
    );

    assert!(
        diagnostics.contains(&2344),
        "expected TS2344 for constrained type argument in call signature, got {diagnostics:?}"
    );
}

#[test]
fn type_literal_property_alias_reports_nested_type_argument_constraint_error() {
    let diagnostics = codes(
        r#"
type Holder<Value extends string> = Value;
type Bag = {
    field: Holder<number>;
};
"#,
    );

    assert!(
        diagnostics.contains(&2344),
        "expected TS2344 for constrained property type argument, got {diagnostics:?}"
    );
}

#[test]
fn type_literal_property_alias_reports_nested_non_generic_type_arguments() {
    let diagnostics = codes(
        r#"
type Plain = string;
type Bag = {
    field: Plain<number>;
};
"#,
    );

    assert!(
        diagnostics.contains(&2315),
        "expected TS2315 for non-generic property type reference, got {diagnostics:?}"
    );
}

#[test]
fn type_literal_property_alias_accepts_renamed_valid_type_arguments() {
    let diagnostics = codes(
        r#"
type Wrapper<Element extends string> = Element;
type Container = {
    value: Wrapper<'ok'>;
};
"#,
    );

    assert!(
        !diagnostics.contains(&2315) && !diagnostics.contains(&2344),
        "expected no TS2315/TS2344 for valid property type reference, got {diagnostics:?}"
    );
}

#[test]
fn nested_type_literal_alias_reports_invalid_computed_entity_name_type() {
    let diagnostics = codes(
        r#"
namespace Foo {
    export enum Enum {
        A = "a",
        B = "b",
    }
}

type Container = {
    x?: { [Foo.Enum]: 0 };
};
"#,
    );

    assert!(
        diagnostics.contains(&2464),
        "expected TS2464 for enum namespace object used as computed property name, got {diagnostics:?}"
    );
}
