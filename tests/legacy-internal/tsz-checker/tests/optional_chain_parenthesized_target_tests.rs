//! Parentheses terminate an optional chain, so a parenthesized chain is not an
//! optional-chain write target.
//!
//! Structural rule: tsc's parser stops propagating `NodeFlags.OptionalChain` at
//! a `ParenthesizedExpression`. `(a.b?.c).d = 1` is therefore an ordinary
//! assignment to a property of a possibly-nullish receiver — TS2532 alone —
//! while tsz's chain walker skipped parentheses at every level of the left
//! spine and added a spurious TS2779.
//!
//! The rule is "a `ParenthesizedExpression` terminates the chain WALK", not
//! "parentheses suppress the diagnostic", and it has three distinct halves that
//! a fix must keep apart:
//!
//! - Parens *inside* the spine end the chain: `(a.b?.c).d = 1` -> no TS2779.
//! - Parens *around the whole target* do not, because tsc's
//!   `checkReferenceExpression` skips the target's own outer parentheses before
//!   testing the chain flag: `(a.b?.c.d) = 1` -> TS2779.
//! - A chain that continues *after* the parenthesized part is still a chain:
//!   `(a.b)?.c.d = 1` -> TS2779.
//!
//! Assertions are not parentheses and do not end a chain: `a?.b!.c = 1` keeps
//! its TS2779.
//!
//! Oracle: `tsc` 7.0.2 (`scripts/conformance/typescript-versions.json`),
//! `--noEmit --strict --target es2022 --lib es2022 --module esnext`. Every
//! expectation below is pinned against a real run.

use crate::test_utils::check_source_strict_codes as strict;

const TS18048: u32 = 18048; // '<x>' is possibly 'undefined'.
const TS2532: u32 = 2532; // Object is possibly 'undefined'.
const TS2777: u32 = 2777; // Increment/decrement operand may not be an optional property access.
const TS2779: u32 = 2779; // Assignment LHS may not be an optional property access.
const TS2781: u32 = 2781; // `for...of` LHS may not be an optional property access.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

fn preamble(outer: &str, mid: &str, leaf: &str) -> String {
    format!(
        "declare const {outer}: {{ {mid}?: {{ {leaf}: number }} }};\ndeclare const items: number[];\n"
    )
}

#[test]
fn a_parenthesized_chain_is_not_an_optional_assignment_target() {
    // Binders are re-spelled per row so nothing keys on identifier text.
    for (outer, mid, leaf) in [
        ("a", "b", "c"),
        ("zq", "al", "be"),
        ("holder", "inner", "leaf"),
    ] {
        let source = format!(
            "{}({outer}.{mid}?.{leaf}).length = 1;",
            format_args!(
                "declare const {outer}: {{ {mid}?: {{ {leaf}: {{ length: number }} }} }};\n"
            )
        );
        let codes = strict(&source);
        assert_eq!(
            count(&codes, TS2532),
            1,
            "`({outer}.{mid}?.{leaf}).length = 1` must report the possibly-undefined receiver, \
             got: {codes:?}"
        );
        assert_eq!(
            count(&codes, TS2779),
            0,
            "parentheses END the chain, so this is an ordinary assignment target and TS2779 \
             must not fire, got: {codes:?}"
        );
    }
}

#[test]
fn a_parenthesized_chain_is_not_an_optional_increment_operand() {
    let source = format!(
        "{}(holder?.inner).leaf++;",
        preamble("holder", "inner", "leaf")
    );
    let codes = strict(&source);
    assert_eq!(count(&codes, TS2532), 1, "got: {codes:?}");
    assert_eq!(
        count(&codes, TS2777),
        0,
        "parentheses end the chain, so the operand is an ordinary target, got: {codes:?}"
    );
}

#[test]
fn a_parenthesized_chain_is_not_an_optional_for_of_head() {
    let source = format!(
        "{}for ((holder?.inner).leaf of items);",
        preamble("holder", "inner", "leaf")
    );
    let codes = strict(&source);
    assert_eq!(count(&codes, TS2532), 1, "got: {codes:?}");
    assert_eq!(
        count(&codes, TS2781),
        0,
        "parentheses end the chain, so the `for...of` head is an ordinary target, got: {codes:?}"
    );
}

// -------------------------------------------------------------------------
// Controls: the two shapes that MUST keep reporting.
// -------------------------------------------------------------------------

#[test]
fn parentheses_around_the_whole_target_keep_it_an_optional_chain_target() {
    // tsc's `checkReferenceExpression` skips the target's OWN outer parentheses
    // before testing the chain flag.
    let source = format!(
        "{}(holder?.inner.leaf) = 1;",
        preamble("holder", "inner", "leaf")
    );
    let codes = strict(&source);
    assert_eq!(
        count(&codes, TS2779),
        1,
        "`(a?.b.c) = 1` is still an optional-chain target, got: {codes:?}"
    );
}

#[test]
fn a_chain_that_continues_after_a_parenthesized_receiver_is_still_a_chain() {
    // The `?.` is OUTSIDE the parentheses here, so the chain starts after them
    // and the target is still an optional-chain target.
    let source = "declare const a: { b?: { c: { d: number } } };\n(a.b)?.c.d = 1;";
    let codes = strict(source);
    assert_eq!(
        count(&codes, TS2779),
        1,
        "`(a.b)?.c.d = 1` must still report TS2779 — the rule terminates the chain WALK at a \
         parenthesis, it does not suppress the diagnostic, got: {codes:?}"
    );
}

#[test]
fn an_assertion_does_not_end_a_chain() {
    let source = format!(
        "{}holder?.inner!.leaf = 1;",
        preamble("holder", "inner", "leaf")
    );
    let codes = strict(&source);
    assert_eq!(
        count(&codes, TS2779),
        1,
        "`!` is not a parenthesis — the chain continues through it, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS18048),
        0,
        "`!` removes the nullish receiver, got: {codes:?}"
    );
}

#[test]
fn a_parenthesized_receiver_without_any_chain_stays_clean() {
    let source = "\
declare const plain: { inner: { leaf: number } };
(plain.inner).leaf = 1;";
    let codes = strict(source);
    assert!(
        codes.is_empty(),
        "a parenthesized non-optional receiver must stay clean, got: {codes:?}"
    );
}
