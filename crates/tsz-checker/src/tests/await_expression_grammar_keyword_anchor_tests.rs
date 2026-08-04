//! Regression tests for #16360: TS1308/TS1375/TS1378/TS1309 must anchor at
//! the `await` keyword token alone, not the whole `AwaitExpression` node.
//!
//! `tsc`'s `checkAwaitExpression` computes `getSpanOfTokenAtPosition(sourceFile,
//! node.pos)` once and reuses it for every diagnostic the function can emit —
//! five characters, `await`. Before this fix tsz anchored these diagnostics on
//! the whole `AwaitExpression` node via `error_at_node`, whose stored `end`
//! runs past the operand onto trailing tokens (the same over-extension family
//! as #16259/#16267's close-brace end positions). The bug is invisible in
//! `--pretty false` output — only the start column is graded there, and the
//! start already matched — so the conformance corpus cannot see it; every
//! expectation here is pinned against a live `tsc@7.0.2 --pretty` run.
//!
//! `for await`'s sibling diagnostic (TS1103, `check_for_await_statement`) was
//! already anchored correctly at `(stmt.pos + 4, 5)` and is unaffected.

use crate::test_utils::check_source_diagnostics;

const AWAIT_KEYWORD_LEN: u32 = 5;

fn diagnostic_length(source: &str, code: u32) -> u32 {
    let diagnostics = check_source_diagnostics(source);
    let matches: Vec<_> = diagnostics.iter().filter(|d| d.code == code).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one TS{code} in {source:?}; got {diagnostics:?}"
    );
    matches[0].length
}

/// `function f() { const x = await 1; return x; }` — tsc anchors TS1308 on
/// `await` alone (5 chars), not `await 1` (7) or `await 1;` (8, the
/// statement's trailing `;`).
#[test]
fn ts1308_anchors_await_keyword_not_operand_or_trailing_semicolon() {
    let source = "function f() { const x = await 1; return x; }";
    assert_eq!(
        diagnostic_length(source, 1308),
        AWAIT_KEYWORD_LEN,
        "TS1308 must span only the `await` keyword"
    );
}

/// An arrow function body: same keyword-only span regardless of how deep the
/// enclosing container's own span would have run.
#[test]
fn ts1308_anchors_await_keyword_in_arrow_body() {
    let source = "const g = () => { const x = await 1; return x; };";
    assert_eq!(diagnostic_length(source, 1308), AWAIT_KEYWORD_LEN);
}

/// A class method body — pins the same rule where `did_you_mean_async_related`
/// (TS1356) also attaches, exercising the `error_at_span_with_related` path
/// rather than the related-info-free `error` path.
#[test]
fn ts1308_anchors_await_keyword_with_did_you_mean_related_info() {
    let source = "class C { m() { const x = await 1; return x; } }";
    assert_eq!(diagnostic_length(source, 1308), AWAIT_KEYWORD_LEN);
}

/// No enclosing function to suggest `async` for: the related-info-free `error`
/// path. A top-level statement inside a non-async, non-function container
/// (`with`) still must not widen the span.
#[test]
fn ts1308_anchors_await_keyword_without_did_you_mean_related_info() {
    let source = r"
function outer() {
  with (await 1) { }
}
";
    assert_eq!(diagnostic_length(source, 1308), AWAIT_KEYWORD_LEN);
}

/// A bare top-level `await` in a non-module script: TS1375, still keyword-only.
#[test]
fn ts1375_anchors_await_keyword() {
    let source = "const x = await 1;";
    assert_eq!(diagnostic_length(source, 1375), AWAIT_KEYWORD_LEN);
}
