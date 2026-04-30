//! Locks in TS2339 display for union receivers narrowed by control flow.
//!
//! Structural rule: when a property lookup fails on a union receiver that
//! flow analysis has narrowed to a strict subset of its members, the
//! diagnostic must name the narrowed type — that is the type the lookup
//! actually ran against. The earlier behavior emitted the original
//! pre-narrowing union, which prints the unrelated alternative members
//! and obscures what the type-checker actually saw.
//!
//! Type parameters keep their existing apparent-type display (the
//! constraint), and non-union receivers keep their literal-preserving
//! display (so primitive-literal receivers like `""` still print as
//! `""` rather than the widened `string`).

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
fn instanceof_narrowed_union_receiver_displays_picked_member() {
    let src = r#"
class A { a: string = ""; }
class B { b: string = ""; }
function f(x: A | B) {
    if (x instanceof A) {
        x.b;
    }
}
"#;
    let diags = diagnostic_messages(src);
    let ts2339 = diags
        .iter()
        .find(|(code, _)| *code == 2339)
        .expect("expected TS2339 for missing 'b' on narrowed receiver");
    assert!(
        ts2339.1.contains("type 'A'"),
        "TS2339 should name the narrowed receiver 'A', got: {}",
        ts2339.1
    );
    assert!(
        !ts2339.1.contains("type 'A | B'"),
        "TS2339 should not display the un-narrowed union 'A | B', got: {}",
        ts2339.1
    );
}

/// Literal-typed receivers must keep their literal display in TS2339 — the
/// helper must not collapse `''` to `'string'` just because the lookup
/// resolves through a primitive apparent type.
#[test]
fn literal_receiver_preserves_literal_in_ts2339_display() {
    let src = r#"
class C extends "".bogus {}
"#;
    let diags = diagnostic_messages(src);
    let ts2339 = diags
        .iter()
        .find(|(code, _)| *code == 2339)
        .expect("expected TS2339 for missing 'bogus' on string literal receiver");
    assert!(
        ts2339.1.contains("\"\"") || ts2339.1.contains("''"),
        "TS2339 should preserve the empty-string literal type in the message, got: {}",
        ts2339.1
    );
}
