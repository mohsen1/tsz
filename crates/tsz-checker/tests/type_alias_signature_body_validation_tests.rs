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
