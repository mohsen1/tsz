use super::*;
use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn has_code(diags: &[crate::diagnostics::Diagnostic], code: u32) -> bool {
    diags.iter().any(|d| d.code == code)
}

fn check_source_with_default_libs(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_type_node_checker_number_keyword() {
    let source = "let x: number;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // Just verify the checker can be created - actual type checking
    // requires more complex setup
    let _checker = TypeNodeChecker::new(&mut ctx);
}

#[test]
fn type_level_tuple_negative_index_emits_t2514() {
    let diags = check_source_with_default_libs("type T = [1, 2]; type t = T[-1];");
    assert!(
        has_code(&diags, 2514),
        "Expected TS2514 for type-level tuple negative index, got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn optional_function_signature_type_includes_undefined() {
    let source = r#"
declare var f: (s: string, n?: number) => void;
declare var g: (s: string, b?: boolean) => void;
f = g;
"#;
    let diags = check_source_with_default_libs(source);

    let relevant: Vec<_> = diags
        .iter()
        .filter(|diag| diag.code == 2322 || diag.code == 2345)
        .collect();
    assert!(
        !relevant.is_empty(),
        "Expected a function assignability diagnostic, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let messages: Vec<_> = relevant
        .iter()
        .map(|diag| diag.message_text.as_str())
        .collect();
    let joined = messages.join("\n");
    assert!(
        joined.contains("boolean | undefined"),
        "Expected optional signature diagnostic to preserve undefined for boolean params.\n{joined}"
    );
    assert!(
        joined.contains("number | undefined"),
        "Expected optional signature diagnostic to preserve undefined for number params.\n{joined}"
    );
}
