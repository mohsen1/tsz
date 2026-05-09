use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn bare_alias_to_generic_class_default_keeps_alias_display() {
    let source = r#"
declare class TableClass<S = any> {
    _field: S;
}

type Table = TableClass;

declare const o: Table;
let value: boolean = o;
"#;

    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            ..CheckerOptions::default()
        },
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for assigning Table to boolean");
    assert!(
        diag.message_text
            .contains("Type 'Table' is not assignable to type 'boolean'."),
        "alias declaration diagnostic should keep the source alias, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("TableClass<any>"),
        "alias declaration diagnostic should not expand to the generic class default, got: {diag:?}"
    );
}
