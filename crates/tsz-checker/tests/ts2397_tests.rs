//! Tests for TS2397: Declaration name conflicts with built-in global identifier.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str, filename: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(filename.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        filename.to_string(),
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
    get_diagnostics(source, "test.ts")
        .iter()
        .any(|d| d.0 == code)
}

#[test]
fn var_undefined_emits_ts2397() {
    assert!(has_error_with_code("var undefined = null;", 2397));
}

#[test]
fn var_undefined_message_text() {
    let diags = get_diagnostics("var undefined = null;", "test.ts");
    let msg = &diags.iter().find(|d| d.0 == 2397).unwrap().1;
    assert!(msg.contains("'undefined'"));
    assert!(msg.contains("built-in global identifier"));
}

#[test]
fn let_undefined_emits_ts2397() {
    // `let undefined` also conflicts
    assert!(has_error_with_code("let undefined: any;", 2397));
}

#[test]
fn namespace_global_this_emits_ts2397() {
    let source = r#"namespace globalThis { export function foo() {} }"#;
    assert!(has_error_with_code(source, 2397));
}

#[test]
fn var_global_this_emits_ts2397() {
    let source = "var globalThis;";
    assert!(has_error_with_code(source, 2397));
}

#[test]
fn global_this_message_text() {
    let diags = get_diagnostics("var globalThis;", "test.ts");
    let msg = &diags.iter().find(|d| d.0 == 2397).unwrap().1;
    assert!(msg.contains("'globalThis'"));
}

#[test]
fn interface_undefined_no_ts2397() {
    // Type declarations named `undefined` should NOT trigger TS2397
    assert!(!has_error_with_code("interface undefined {}", 2397));
}

#[test]
fn type_alias_undefined_no_ts2397() {
    assert!(!has_error_with_code("type undefined = string;", 2397));
}

#[test]
fn namespace_undefined_emits_ts2397() {
    // `namespace undefined { ... }` conflicts with built-in `undefined`
    let source = r#"namespace undefined { export var x = 42; }"#;
    assert!(has_error_with_code(source, 2397));
}

#[test]
fn module_scoped_global_this_no_ts2397() {
    // In an external module (has import/export), `globalThis` declarations
    // should NOT trigger TS2397 because they're module-scoped.
    let source = r#"
export {};
namespace globalThis { export function foo() {} }
"#;
    assert!(!has_error_with_code(source, 2397));
}
