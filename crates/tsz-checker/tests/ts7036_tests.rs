//! Tests for TS7036: Dynamic import's specifier must be of type 'string'.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn import_boolean_specifier_emits_ts7036() {
    // import() with a boolean argument should emit TS7036
    let src = r#"
declare function getSpec(): boolean;
import(getSpec());
"#;
    assert!(has_error_with_code(src, 7036));
}

#[test]
fn import_number_specifier_emits_ts7036() {
    let src = "import(42);";
    assert!(has_error_with_code(src, 7036));
}

#[test]
fn import_array_specifier_emits_ts7036() {
    let src = r#"import(["path1", "path2"]);"#;
    assert!(has_error_with_code(src, 7036));
}

#[test]
fn import_arrow_function_specifier_emits_ts7036() {
    let src = r#"import(() => "module");"#;
    assert!(has_error_with_code(src, 7036));
}

#[test]
fn import_string_literal_no_ts7036() {
    // String literal specifier should NOT emit TS7036
    let src = r#"import("./module");"#;
    assert!(!has_error_with_code(src, 7036));
}

#[test]
fn import_string_variable_no_ts7036() {
    // Variable of type string should NOT emit TS7036
    let src = r#"
declare var path: string;
import(path);
"#;
    assert!(!has_error_with_code(src, 7036));
}

#[test]
fn import_any_specifier_no_ts7036() {
    // `any` type is assignable to string, no TS7036
    let src = r#"
declare var x: any;
import(x);
"#;
    assert!(!has_error_with_code(src, 7036));
}

#[test]
fn ts7036_message_contains_type_name() {
    let src = r#"
declare function getSpec(): boolean;
import(getSpec());
"#;
    let diags = get_diagnostics(src);
    let msg = &diags.iter().find(|d| d.0 == 7036).unwrap().1;
    assert!(
        msg.contains("boolean"),
        "message should contain the type name: {msg}"
    );
}
