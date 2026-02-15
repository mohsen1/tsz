//! Test for conditional expression union type compatibility with generic constraints

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_conditional_expression_union_assignable_to_keyof_constraint() {
    let code = r#"
interface Shape {
    width: number;
    height: number;
}

function getProperty<T, K extends keyof T>(obj: T, key: K): T[K] {
    return obj[key];
}

const shape: Shape = { width: 100, height: 200 };
const cond = true;
const result = getProperty(shape, cond ? "width" : "height");
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
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

    // Should have NO errors - "width" | "height" is assignable to keyof Shape
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();

    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 errors for conditional expression with union type matching keyof constraint, got {ts2322_count}"
    );
}
