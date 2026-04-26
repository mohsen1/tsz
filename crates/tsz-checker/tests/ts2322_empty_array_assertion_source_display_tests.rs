//! Locks in TS2322 messages keeping the asserted type as the source display
//! when the inner expression is an empty array literal.
//!
//! Regression: assigning `[] as Foo` to an incompatible target reported
//! `Type 'never[]' is not assignable to type 'Bar'.` instead of
//! `Type 'Foo' is not assignable to type 'Bar'.`. The empty-array source
//! display helper used `skip_parenthesized_and_assertions`, which drilled
//! through `as`/angle-bracket type assertions and substituted the bare
//! `[]` literal type (`never[]` under strict null checks) for the asserted
//! type — matching neither tsc's diagnostic shape nor the assertion's
//! semantic effect.
//!
//! `object_literal_source_type_display` already had the correct behavior
//! (skip parens only, not assertions) for `({} as Foo)`. This regression
//! test pins the parallel behavior for the array-literal branch.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostic_messages(source: &str) -> Vec<(u32, String)> {
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
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn ts2322_empty_array_assertion_source_uses_asserted_type_not_never_array() {
    // `[] as X` should display the asserted source as `X`, not `never[]`,
    // when the variable assignment to a non-array target fails.
    let src = r#"
type X = { a: number };
const target: { b: number } = [] as X;
"#;
    let diagnostics = diagnostic_messages(src);
    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322 || *code == 2740 || *code == 2741)
        .expect("expected an assignability error for `[] as X`");
    assert!(
        ts2322.1.contains("Type 'X'") || ts2322.1.contains("type 'X'"),
        "assignability diagnostic should display the asserted type 'X', got: {ts2322:?}"
    );
    assert!(
        !ts2322.1.contains("never[]"),
        "assignability diagnostic must not display the inner empty-array literal type \
         when the source is wrapped in a type assertion, got: {ts2322:?}"
    );
}

#[test]
fn ts2322_empty_array_assertion_source_uses_asserted_type_against_readonly_array_target() {
    // `[] as X` against a `readonly any[]` target was the original failure
    // mode. The asserted source should still display as `X`, not `never[]`.
    let src = r#"
type X = { a: number };
const target: readonly any[] = [] as X;
"#;
    let diagnostics = diagnostic_messages(src);
    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322 || *code == 2740 || *code == 2741)
        .expect("expected an assignability error for `[] as X` to readonly array");
    assert!(
        ts2322.1.contains("Type 'X'") || ts2322.1.contains("type 'X'"),
        "diagnostic should display the asserted type 'X', got: {ts2322:?}"
    );
    assert!(
        !ts2322.1.contains("never[]"),
        "diagnostic must not display 'never[]' when the source is `[] as X`, got: {ts2322:?}"
    );
}

#[test]
fn ts2322_empty_array_paren_only_source_still_displays_never_array() {
    // Sanity check: when the source is just a parenthesized empty array
    // (no type assertion), the display should still surface the empty
    // array's literal source type. The fix only suppresses the override
    // for type assertions, not for plain parenthesization.
    let src = r#"
const target: { b: number } = ([]);
"#;
    let diagnostics = diagnostic_messages(src);
    let assignability = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322 || *code == 2740 || *code == 2741)
        .expect("expected an assignability error for `([]) -> { b: number }`");
    assert!(
        assignability.1.contains("never[]"),
        "diagnostic should still surface 'never[]' for a paren-only empty-array source, \
         got: {assignability:?}"
    );
}
