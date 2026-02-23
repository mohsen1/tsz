//! Tests for TS2589: Type instantiation is excessively deep and possibly infinite.

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
fn recursive_type_alias_emits_ts2589() {
    // type Foo<T, B> = { "true": Foo<T, Foo<T, B>> }[T] is infinitely recursive
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#;
    assert!(
        has_error_with_code(source, 2589),
        "Should emit TS2589 for infinitely recursive type alias instantiation"
    );
}

#[test]
fn recursive_type_alias_ts2589_at_usage_not_definition() {
    // TS2589 should be at the usage site (f1's type annotation), not the definition
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#;
    let diags = get_diagnostics(source);
    let ts2589_count = diags.iter().filter(|d| d.0 == 2589).count();
    // Expect exactly 1 TS2589 (at the usage), not 2 (at definition + usage)
    assert_eq!(
        ts2589_count, 1,
        "TS2589 should be emitted once at the usage site, got {ts2589_count}"
    );
}

#[test]
fn non_recursive_type_alias_no_ts2589() {
    // A non-recursive generic type alias should not trigger TS2589
    let source = r#"
type Wrapper<T> = { value: T };
let w: Wrapper<string>;
"#;
    assert!(
        !has_error_with_code(source, 2589),
        "Should NOT emit TS2589 for non-recursive type alias"
    );
}

#[test]
fn shallow_recursive_type_alias_no_ts2589() {
    // A type alias that is self-referential but bounded (via conditional) should not trigger TS2589
    // if the recursion terminates before hitting the depth limit
    let source = r#"
type StringOnly<T> = T extends string ? T : never;
let s: StringOnly<"hello">;
"#;
    assert!(
        !has_error_with_code(source, 2589),
        "Should NOT emit TS2589 for bounded conditional type"
    );
}

#[test]
fn ts2589_message_text() {
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f1: Foo<"true", {}>;
"#;
    let diags = get_diagnostics(source);
    let ts2589 = diags.iter().find(|d| d.0 == 2589);
    assert!(ts2589.is_some(), "TS2589 should be emitted");
    assert_eq!(
        ts2589.unwrap().1,
        "Type instantiation is excessively deep and possibly infinite."
    );
}
