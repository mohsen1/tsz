//! Anchor tests for TS2769 when no overload matches and the overloads
//! disagree on their failure messages.
//!
//! tsc's rule: when all overloads fail with argument-type mismatches (TS2345
//! elaborations) but the rendered failures differ across overloads (e.g.,
//! each overload rejects a *different* excess property in the same object
//! literal), tsc anchors the top-level TS2769 at the callee / whole call
//! expression, not at the argument. The shared-argument heuristic only fires
//! when every overload rejects the argument for the same reason.
//!
//! Baseline this locks in (TypeScript compiler):
//!   orderMattersForSignatureGroupIdentity.ts(19,1): TS2769 — anchor at `v`,
//!     not at the `{ s: "", n: 0 }` argument.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, u32, String)> {
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
        Default::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
}

#[test]
fn ts2769_anchored_at_callee_when_overloads_disagree() {
    // Two overloads, each rejecting a different excess property on the same
    // object literal. Failure messages differ across overloads, so the
    // top-level TS2769 must anchor at the callee (`v`) — not at the argument.
    let source = r#"interface A {
    (x: { s: string }): string
    (x: { n: number }): number
}
declare var v: A;
v({ s: "", n: 0 });
"#;
    let diags = get_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|(code, _, _)| *code == 2769).collect();
    assert_eq!(ts2769.len(), 1, "expected one TS2769, got {diags:#?}");
    let callee_start = source
        .find("v({ s:")
        .expect("callee start must exist in fixture") as u32;
    let argument_start = source
        .find("{ s: \"\", n: 0 }")
        .expect("argument start must exist") as u32;
    assert_eq!(
        ts2769[0].1, callee_start,
        "TS2769 should anchor at callee `v` (offset {}), not at the argument (offset {}). got start={}",
        callee_start, argument_start, ts2769[0].1
    );
}

#[test]
fn ts2769_still_anchored_at_argument_when_overloads_agree() {
    // Two overloads that reject the argument with the *same* rendered message
    // (both expect the same `string` parameter — the overloads differ only in
    // return type via generics or declarations, not in the argument shape).
    // The argument is the single culprit → anchor should stay on the argument.
    let source = r#"interface A {
    (x: string): string
    (x: string): number
}
declare var f: A;
f(42);
"#;
    let diags = get_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|(code, _, _)| *code == 2769).collect();
    assert_eq!(ts2769.len(), 1, "expected one TS2769, got {diags:#?}");
    let argument_start = source.find("42").expect("argument start must exist") as u32;
    let callee_start = source.find("f(42)").expect("callee start must exist") as u32;
    // When overloads agree on the failure, tsz anchors at the argument.
    assert!(
        ts2769[0].1 == argument_start || ts2769[0].1 == callee_start,
        "TS2769 should anchor at callee or argument for identical-failure overloads; got start={}",
        ts2769[0].1
    );
    // Specifically, the existing behavior for agreeing overloads is argument-anchor;
    // this locks that in so our change does not broaden the callee-anchor path.
    assert_eq!(
        ts2769[0].1, argument_start,
        "TS2769 should stay at argument when overloads produce identical failure messages"
    );
}
