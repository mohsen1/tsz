//! Regression: TS2344 messages must preserve literal type arguments rather
//! than widening them to the base type. tsc's `checkTypeArgumentConstraints`
//! passes the type argument to `typeToString` unchanged, so a constraint like
//! `T extends "true"` violated by `"false"` yields:
//!
//!   Type '"false"' does not satisfy the constraint '"true"'.
//!
//! Previously we widened `"false"` to `string`, producing a nonsensical
//! message when the constraint is itself a string-literal type.

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
fn ts2344_preserves_string_literal_type_argument() {
    let source = r#"
type Foo<T extends "true", B> = { "true": Foo<T, Foo<T, B>> }[T];
let f2: Foo<"false", {}>;
"#;
    let diags = compile_and_get_diagnostics(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Expected TS2344 for '\"false\"' violating '\"true\"' constraint, got: {diags:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|(_, m)| m.contains("\"false\"") && m.contains("\"true\"")),
        "Expected TS2344 message to preserve literal types (\"false\" / \"true\"), got: {ts2344:?}"
    );
    for (_, msg) in &ts2344 {
        assert!(
            !msg.contains("'string'") || msg.contains("\""),
            "TS2344 message should not widen literal to 'string': {msg}"
        );
    }
}

#[test]
fn ts2344_preserves_numeric_literal_type_argument() {
    let source = r#"
type Only42<T extends 42> = T;
type X = Only42<7>;
"#;
    let diags = compile_and_get_diagnostics(source);
    let ts2344: Vec<&(u32, String)> = diags.iter().filter(|(c, _)| *c == 2344).collect();
    assert!(
        !ts2344.is_empty(),
        "Expected TS2344 for 7 violating 42 constraint, got: {diags:?}"
    );
    assert!(
        ts2344
            .iter()
            .any(|(_, m)| m.contains("7") && m.contains("42")),
        "Expected TS2344 message to preserve numeric literals (7 / 42), got: {ts2344:?}"
    );
}
