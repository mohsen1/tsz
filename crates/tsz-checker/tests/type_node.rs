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
