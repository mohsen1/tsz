//! Lock in `finalize_pair_display_for_diagnostic` preserving upstream-widened
//! source displays when the pair is already distinguishable.
//!
//! Regression: assigning `W.a` to `typeof W` reported
//! `Type 'W.a' is not assignable to type 'typeof W'.` even though
//! `format_assignment_source_type_for_diagnostic` correctly widened the
//! enum-member source to the enum type `W`. The disambiguator unconditionally
//! re-formatted from the raw `W.a` TypeId and clobbered the widened display.

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
fn ts2322_enum_member_to_typeof_enum_keeps_widened_source_display() {
    let src = r#"
enum W { a, b, c }
declare var b: typeof W;
b = W.a;
"#;
    let diagnostics = diagnostic_messages(src);
    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for `b = W.a`");
    assert!(
        ts2322.1.contains("Type 'W'"),
        "TS2322 should display the widened enum source `W`, got: {ts2322:?}"
    );
    assert!(
        !ts2322.1.contains("Type 'W.a'"),
        "TS2322 must not re-qualify the widened source back to `W.a`, got: {ts2322:?}"
    );
}

#[test]
fn ts2322_enum_member_to_wstatic_keeps_widened_source_display() {
    let src = r#"
enum W { a, b, c }
namespace W { export class D {} }
interface WStatic { a: W; b: W; c: W }
declare var f: WStatic;
f = W.a;
"#;
    let diagnostics = diagnostic_messages(src);
    let ts2322 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2322)
        .expect("expected TS2322 for `f = W.a`");
    assert!(
        ts2322.1.contains("Type 'W'"),
        "TS2322 should display source as widened `W`, got: {ts2322:?}"
    );
    assert!(
        !ts2322.1.contains("Type 'W.a'"),
        "TS2322 must not re-qualify the widened source, got: {ts2322:?}"
    );
}
