//! Tests for TS2558: Expected N type arguments, but got M (new expressions)

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_error_codes(source: &str) -> Vec<u32> {
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

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

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
