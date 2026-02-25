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

/// Regression test for react16.d.ts infinite loop.
///
/// Deeply-nested generic types with cross-referencing interfaces and type
/// aliases (like React's `InferProps`, `RequiredKeys`, Validator chains) used to
/// cause `evaluate_application_type` to recurse unboundedly because:
/// - Per-context `instantiation_depth` resets on cross-arena delegation
/// - The worklist in `ensure_application_symbols_resolved` expanded transitively
///
/// This test creates a simplified version of the pathological pattern and
/// verifies it completes within a reasonable time (< 5 seconds).
#[test]
fn deeply_nested_generics_do_not_hang() {
    use std::time::Instant;

    // This pattern mimics react16.d.ts: multiple generic interfaces that
    // cross-reference each other through type parameters, creating a deep
    // expansion graph when types are eagerly resolved.
    let source = r#"
interface Validator<T> {
    validate(props: T): Error | null;
}

type ValidationMap<T> = { [K in keyof T]?: Validator<T[K]> };

type InferType<V> = V extends Validator<infer T> ? T : any;

type InferProps<V extends ValidationMap<any>> = {
    [K in keyof V]: InferType<V[K]>;
};

type RequiredKeys<V extends ValidationMap<any>> = {
    [K in keyof V]: V[K] extends Validator<infer T> ? K : never;
}[keyof V];

interface Requireable<T> extends Validator<T | undefined | null> {
    isRequired: Validator<T>;
}

interface ReactPropTypes {
    any: Requireable<any>;
    array: Requireable<any[]>;
    bool: Requireable<boolean>;
    func: Requireable<(...args: any[]) => any>;
    number: Requireable<number>;
    object: Requireable<object>;
    string: Requireable<string>;
    node: Requireable<any>;
    element: Requireable<any>;
}

declare const PropTypes: ReactPropTypes;

type MyProps = InferProps<{
    name: typeof PropTypes.string;
    count: typeof PropTypes.number;
    items: typeof PropTypes.array;
}>;

let p: MyProps;
"#;

    let start = Instant::now();
    let _diags = get_diagnostics(source);
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "Deeply nested generics took {elapsed:?} — should complete in < 5s (was hanging before fix)"
    );
}
