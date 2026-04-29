//! Locks tsc-parity for the TS2352 (`as`-conversion overlap) message when
//! the asserted type is an `Application(<alias>, [args])` whose alias body
//! is parenthesized.
//!
//! Regression target: `parenthesisDoesNotBlockAliasSymbolCreation.ts`.
//!
//! tsc renders `null as A<{x:number}>` (where `A<T> = (T & InvalidKeys<"a">)`)
//! as `Conversion of type 'null' to type 'A<{ x: number; }>' ...`. tsz used to
//! expand the alias to its body intersection in this position because the
//! formatter looked through the evaluated body's `display_alias` (which on
//! `evaluate_application` only stores the *body* surface, not always an
//! `Application` node), bypassing the `type_application(target_type)` path
//! that would have preserved the alias name.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_common::checker_options::CheckerOptions;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn checker_diagnostics(source: &str) -> Vec<(u32, String)> {
    let file_name = "test.ts";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions::default();

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn ts2352_messages(diags: &[(u32, String)]) -> Vec<String> {
    diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::CONVERSION_OF_TYPE_TO_TYPE_MAY_BE_A_MISTAKE_BECAUSE_NEITHER_TYPE_SUFFICIENTLY_OV)
        .map(|(_, m)| m.clone())
        .collect()
}

#[test]
fn ts2352_preserves_alias_name_when_alias_body_is_parenthesized_intersection() {
    let source = r#"
type InvalidKeys<K extends string|number|symbol> = { [P in K]?: never };
type A<T> = (
    T & InvalidKeys<"a">
);

const a = null as A<{ x: number }>;
"#;
    let diags = checker_diagnostics(source);
    let msgs = ts2352_messages(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("'A<{ x: number; }>'")),
        "TS2352 should display the alias surface 'A<{{ x: number; }}>'. Got: {msgs:?}"
    );
    assert!(
        !msgs
            .iter()
            .any(|m| m.contains("'{ x: number; } & InvalidKeys<\"a\">'")),
        "TS2352 must not expand the alias to its evaluated body for this case. Got: {msgs:?}"
    );
}

#[test]
fn ts2352_preserves_alias_name_when_alias_body_is_parenthesized_mapped() {
    let source = r#"
type InvalidKeys2<K extends string|number|symbol> = (
    { [P in K]?: never }
);
type A2<T> = T & InvalidKeys2<"a">;

const a2 = null as A2<{ x: number }>;
"#;
    let diags = checker_diagnostics(source);
    let msgs = ts2352_messages(&diags);
    assert!(
        msgs.iter().any(|m| m.contains("'A2<{ x: number; }>'")),
        "TS2352 should display the outer alias 'A2<{{ x: number; }}>'. Got: {msgs:?}"
    );
}

#[test]
fn ts2352_no_alias_case_still_shows_expanded_intersection() {
    // Control: when there is no enclosing alias, tsc displays the structural
    // intersection (no alias to preserve). This must remain unchanged.
    let source = r#"
type InvalidKeys<K extends string|number|symbol> = { [P in K]?: never };

const a = null as { x: number } & InvalidKeys<"a">;
"#;
    let diags = checker_diagnostics(source);
    let msgs = ts2352_messages(&diags);
    assert!(
        msgs.iter()
            .any(|m| m.contains("'{ x: number; } & InvalidKeys<\"a\">'")),
        "Without an outer alias, the intersection must be displayed expanded. Got: {msgs:?}"
    );
}
