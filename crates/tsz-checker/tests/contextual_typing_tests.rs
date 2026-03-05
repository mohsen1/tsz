//! Tests for the circular return-type assignability fix.
//!
//! When a function/getter has no explicit return type annotation, the checker
//! infers the return type from the body.  Previously it then re-checked the
//! return statement against that inferred type, which could cause false TS2322
//! errors (e.g. for nested array literals with different object shapes).
//!
//! The fix pushes `TypeId::ANY` as the return type context when the return type
//! is purely inferred, so `check_return_statement` skips the circular check.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper: parse, bind, check with default options.
fn check_default(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Function returning nested array literals with different object shapes should
/// NOT produce false TS2322.  The return type is purely inferred so there is no
/// external constraint to check against.
#[test]
fn test_no_false_ts2322_for_inferred_return_with_nested_arrays() {
    let source = r#"
function f() {
    return [
        ['a', { x: 1 }],
        ['b', { y: 2 }]
    ];
}
"#;
    let diagnostics = check_default(source);
    let ts2322_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322_errors.is_empty(),
        "Inferred return type should not cause circular TS2322 check, got: {ts2322_errors:?}"
    );
}

/// Getter returning nested array literals without annotation should not produce
/// false TS2322 — same circular-check avoidance applies to getters.
#[test]
fn test_no_false_ts2322_for_getter_inferred_return() {
    let source = r#"
class C {
    get x() {
        return [
            ['a', { x: 1 }],
            ['b', { y: 2 }]
        ];
    }
}
"#;
    let diagnostics = check_default(source);
    let ts2322_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322_errors.is_empty(),
        "Getter with inferred return should not cause circular TS2322, got: {ts2322_errors:?}"
    );
}

/// When a generic function has a rest parameter whose type is a mapped type Application
/// (e.g., `...values: UnwrapContainers<T>`), the Application must be evaluated before
/// contextual parameter extraction and function subtype comparison. Otherwise, each callback
/// parameter gets the whole tuple type instead of individual elements, causing false TS2345.
#[test]
fn test_no_false_ts2345_for_mapped_tuple_rest_spread() {
    let source = r#"
type Container<T> = { value: T };
type UnwrapContainers<T extends Container<unknown>[]> = { [K in keyof T]: T[K]['value'] };

declare function createContainer<T extends unknown>(value: T): Container<T>;
declare function f<T extends Container<unknown>[]>(
    containers: [...T],
    callback: (...values: UnwrapContainers<T>) => void
): void;

const c1 = createContainer('hi');
const c2 = createContainer(2);

f([c1, c2], (value1, value2) => {
    value1;
    value2;
});
"#;
    let diagnostics = check_default(source);
    let ts2345_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345_errors.is_empty(),
        "Mapped tuple rest spread should not produce false TS2345, got: {ts2345_errors:?}"
    );
}

/// When a function HAS an explicit return type, the check should still work.
/// This ensures we didn't disable return type checking entirely.
#[test]
fn test_annotated_return_type_still_checked() {
    let source = r#"
function f(): number {
    return "hello";
}
"#;
    let diagnostics = check_default(source);
    let ts2322_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322_errors.is_empty(),
        "Annotated return type should still produce TS2322 for type mismatch"
    );
}
