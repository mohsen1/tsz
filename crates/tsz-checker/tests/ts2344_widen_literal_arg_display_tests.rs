//! Tests for TS2344 message-display widening of literal type arguments.
//!
//! When a literal type argument fails a primitive base-type constraint,
//! tsc widens the literal to its primitive base for the message:
//!
//! - `Uppercase<42>` (constraint `string`) → `Type 'number' does not satisfy …`
//!
//! Literal-vs-literal mismatches keep the literal display:
//!
//! - `Foo<"false">` against constraint `"true"` → `Type '"false"' does not satisfy …`
//!
//! Conformance test: `intrinsicTypes.ts`.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn ts2344_widens_number_literal_arg_against_primitive_constraint() {
    let diags = compile_diagnostics(
        r#"
type Foo<T extends string> = T;
type R = Foo<42>;
"#,
    );
    let ts2344 = diags
        .iter()
        .find(|(code, _)| *code == 2344)
        .expect("expected TS2344");
    assert!(
        ts2344.1.contains("Type 'number'"),
        "expected widened 'number' display, got: {ts2344:?}"
    );
    assert!(
        !ts2344.1.contains("'42'"),
        "literal '42' should not appear in widened message: {ts2344:?}"
    );
}

#[test]
fn ts2344_widens_bigint_literal_arg_against_primitive_constraint() {
    let diags = compile_diagnostics(
        r#"
type Foo<T extends string> = T;
type R = Foo<42n>;
"#,
    );
    let ts2344 = diags
        .iter()
        .find(|(code, _)| *code == 2344)
        .expect("expected TS2344");
    assert!(
        ts2344.1.contains("Type 'bigint'"),
        "expected widened 'bigint' display, got: {ts2344:?}"
    );
}

#[test]
fn ts2344_keeps_string_literal_against_string_literal_constraint() {
    // tsc baseline (limitDeepInstantiations.errors.txt) shows the literal
    // '"false"' is preserved when the constraint is a string literal type.
    let diags = compile_diagnostics(
        r#"
type Foo<X extends "true"> = X;
type R = Foo<"false">;
"#,
    );
    let ts2344 = diags
        .iter()
        .find(|(code, _)| *code == 2344)
        .expect("expected TS2344");
    assert!(
        ts2344.1.contains("'\"false\"'"),
        "literal '\"false\"' should be preserved against literal constraint: {ts2344:?}"
    );
    assert!(
        !ts2344.1.contains("'string' does not satisfy"),
        "must not widen to 'string' against literal constraint: {ts2344:?}"
    );
}
