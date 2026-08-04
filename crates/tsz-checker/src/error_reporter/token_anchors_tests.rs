//! Span-level tests for the `getSpanOfTokenAtPosition` anchor family.
//!
//! Every expectation here is the `(start, length)` pair `tsc@7.0.2` reports
//! for the same source, read off `--pretty` output (the squiggle's column and
//! width). Asserting the code alone cannot see this defect: the `start`
//! already matched before the fix, which is exactly why the conformance
//! corpus — graded on `--pretty false`, i.e. `file(line,col)` only — is
//! structurally blind to it (#16360).
//!
//! Binder names are varied across the matrix so no expectation can be
//! satisfied by a name-shaped rule.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_diagnostics, check_source_with_file_is_esm};

/// `(start, length)` of every diagnostic with `code`, in report order.
fn spans_for(source: &str, code: u32) -> Vec<(u32, u32)> {
    check_source_diagnostics(source)
        .iter()
        .filter(|diag| diag.code == code)
        .map(|diag| (diag.start, diag.length))
        .collect()
}

/// The byte offset of `needle` in `source`, as a span of `needle`'s length.
/// Expectations are written against the source text itself so a test cannot
/// silently encode an off-by-one the implementation also has.
fn span_of(source: &str, needle: &str) -> (u32, u32) {
    let start = source.find(needle).expect("needle not present in source");
    (
        u32::try_from(start).expect("offset fits u32"),
        u32::try_from(needle.len()).expect("length fits u32"),
    )
}

const AWAIT: &str = "await";
const TS1308: u32 = 1308;
const TS1103: u32 = 1103;

#[test]
fn ts1308_anchors_the_await_keyword_not_the_expression() {
    // tsc: `f.ts:1:26 - error TS1308` with a 5-character squiggle over
    // `await`. Anchoring the AwaitExpression node covered `await 1;`,
    // including the statement's trailing semicolon.
    let source = "function outer() { const value = await 1; return value; }";
    assert_eq!(spans_for(source, TS1308), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1308_anchor_is_the_keyword_for_a_multi_token_operand() {
    // The operand's size must not reach the squiggle at all.
    let source = "function compute() { return await someCall(1, 2, 3).then(handler); }";
    assert_eq!(spans_for(source, TS1308), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1308_anchor_survives_trivia_between_the_operator_and_its_operand() {
    // Comment trivia sits inside the AwaitExpression's span but after the
    // keyword, so a node anchor swallows it and a token anchor does not.
    let source = "function withNote() { const held = await /* pending */ 1; return held; }";
    assert_eq!(spans_for(source, TS1308), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1308_anchor_is_per_await_in_a_function_expression() {
    // Adjacent binder shape: function expression rather than declaration,
    // and two independent awaits, each anchored on its own keyword.
    let source =
        "const runner = function () { const a = await 1; const b = await 2; return a + b; };";
    let spans = spans_for(source, TS1308);
    let first = span_of(source, AWAIT);
    let second_start = source[first.0 as usize + 1..]
        .find(AWAIT)
        .expect("second await present") as u32
        + first.0
        + 1;
    assert_eq!(spans, vec![first, (second_start, 5)]);
}

#[test]
fn ts1308_anchor_in_an_arrow_body() {
    let source = "const load = () => await fetchThing();";
    assert_eq!(spans_for(source, TS1308), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1308_anchor_in_a_method() {
    let source = "class Holder { grab() { const item = await 1; return item; } }";
    assert_eq!(spans_for(source, TS1308), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1308_anchor_in_a_getter() {
    let source = "class Store { get value() { return await 1; } }";
    assert_eq!(spans_for(source, TS1308), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1308_anchor_in_a_generator() {
    let source = "function* stream() { const chunk = await 1; yield chunk; }";
    assert_eq!(spans_for(source, TS1308), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1103_anchors_the_await_keyword_of_a_for_await() {
    // tsc anchors `ForOfStatement.awaitModifier`, not the statement.
    let source = "function drain() { for await (const line of lines) { use(line); } }";
    assert_eq!(spans_for(source, TS1103), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1103_anchor_holds_when_the_keywords_are_not_one_space_apart() {
    // The regression this replaces a fixed `stmt.pos + 4` offset for: any
    // spacing other than a single space mis-anchored the squiggle, silently,
    // since only the length and start-within-the-line move.
    let source = "function drain() { for /* soon */ await (const row of rows) { use(row); } }";
    assert_eq!(spans_for(source, TS1103), vec![span_of(source, AWAIT)]);
}

#[test]
fn ts1103_anchor_holds_across_a_line_break_after_for() {
    let source = "function drain() {\n  for\n    await (const item of items) { use(item); }\n}";
    assert_eq!(spans_for(source, TS1103), vec![span_of(source, AWAIT)]);
}

#[test]
fn top_level_await_pair_anchors_the_keyword_in_a_script_file() {
    // Negative-control axis for the top-level arm: a non-module file gets
    // TS1375, and it is anchored on the same keyword token as TS1308's arm.
    // `check_source_diagnostics` checks a script (non-ESM) file.
    let source = "const started = await 1;";
    let expected = span_of(source, AWAIT);
    let spans = spans_for(source, 1375);
    assert_eq!(spans, vec![expected], "TS1375 anchors the `await` keyword");
}

#[test]
fn an_await_inside_an_async_function_is_still_clean() {
    // The anchor change must not make a legal `await` report anything.
    let source = "async function ok() { const value = await 1; return value; }";
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|diag| matches!(diag.code, TS1308 | TS1103 | 1375 | 1378)),
        "legal await reported {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn ts1378_anchors_the_keyword_in_a_module_file() {
    // The fourth changed site. In an ESM file the TS1375 "must be a module"
    // arm falls away, leaving the module/target arm — which `tsc` anchors on
    // the same keyword token. Covering it here keeps all four
    // `checkAwaitExpression` diagnostics pinned, not just the two a script
    // file can reach.
    let source = "export const started = await 1;";
    let diagnostics =
        check_source_with_file_is_esm(source, "m.ts", CheckerOptions::default(), Some(true));
    let spans: Vec<(u32, u32)> = diagnostics
        .iter()
        .filter(|diag| diag.code == 1378)
        .map(|diag| (diag.start, diag.length))
        .collect();
    assert_eq!(spans, vec![span_of(source, AWAIT)]);
    assert!(
        !diagnostics
            .iter()
            .any(|diag| matches!(diag.code, TS1308 | TS1103 | 1375)),
        "a module file answers neither TS1308/TS1103 nor TS1375 here; got {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
