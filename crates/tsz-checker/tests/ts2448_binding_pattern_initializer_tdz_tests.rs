//! Regression tests for TS2448 (block-scoped variable used before its
//! declaration) when the self-reference lives in a *binding pattern's* own
//! initializer, or in a `for-in` header — cases the plain-identifier
//! `usage.pos <= decl.end` position test used to miss.
//!
//! A block-scoped binding is in its temporal dead zone throughout its own
//! initializer region. For a binding pattern the declared name sits in a small
//! binding element whose end precedes the outer initializer, so
//! `let [x] = x + 1` and `for (let [v] of v) {}` slipped past the position
//! test; `for (let v in v) {}` was likewise uncovered by the for-of-only path.
//!
//! Every expectation is oracle-pinned against `tsc` 7.0.2
//! (`--noEmit --target es6 --strict false`), matching the `// @strict: false`
//! directive `recursiveLetConst.ts` carries. Binder names are varied so nothing
//! here can be satisfied by a user-chosen identifier.

use crate::test_utils::check_source_non_strict_codes;

const TS2448: u32 = 2448;

fn codes(source: &str) -> Vec<u32> {
    check_source_non_strict_codes(source)
}

fn assert_has_2448(source: &str, label: &str) {
    let got = codes(source);
    assert!(
        got.contains(&TS2448),
        "{label}: expected TS2448 (self-reference in own declaration), got {got:?}"
    );
}

fn assert_no_2448(source: &str, label: &str) {
    let got = codes(source);
    assert!(
        !got.contains(&TS2448),
        "{label}: expected no TS2448, got {got:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive — the self-reference is inside the declaration's own initializer.
// ---------------------------------------------------------------------------

#[test]
fn array_binding_pattern_referencing_itself_in_the_initializer_is_ts2448() {
    // `recursiveLetConst.ts`: `let [x1] = x1 + 1;`
    assert_has_2448("let [alpha] = alpha + 1;\n", "let array pattern");
}

#[test]
fn const_array_binding_pattern_referencing_itself_is_ts2448() {
    // `const [y1] = y1 + 1;`
    assert_has_2448("const [beta] = beta + 1;\n", "const array pattern");
}

#[test]
fn object_binding_pattern_referencing_itself_in_the_initializer_is_ts2448() {
    assert_has_2448("let { gamma } = gamma;\n", "let object pattern");
}

#[test]
fn c_style_for_binding_pattern_referencing_itself_is_ts2448() {
    // `for (let [v] = v; ;) { }`
    assert_has_2448(
        "for (let [delta] = delta; ; ) { }\n",
        "for-init array pattern",
    );
}

#[test]
fn for_in_header_referencing_its_own_loop_variable_is_ts2448() {
    // `for (let v in v) { }` — the for-of-only path did not cover this.
    assert_has_2448("for (let epsilon in epsilon) { }\n", "for-in loop variable");
}

#[test]
fn for_of_binding_pattern_referencing_its_own_loop_variable_is_ts2448() {
    // `for (let [v] of v) { }`
    assert_has_2448("for (let [zeta] of zeta) { }\n", "for-of array pattern");
}

#[test]
fn simple_identifier_self_reference_still_reports_ts2448() {
    // The plain case that always worked — must stay green.
    assert_has_2448("let eta = eta + 1;\n", "simple identifier");
}

// ---------------------------------------------------------------------------
// Negative — no self-reference, or a deferred reference. No TS2448.
// ---------------------------------------------------------------------------

#[test]
fn array_binding_pattern_without_self_reference_is_clean() {
    assert_no_2448("let [theta] = [1];\n", "array pattern, no self-ref");
}

#[test]
fn object_binding_pattern_from_a_prior_binding_is_clean() {
    assert_no_2448(
        "let iota = { k: 1 };\nlet { k: kappa } = iota;\n",
        "object pattern from prior binding",
    );
}

#[test]
fn binding_pattern_reading_an_earlier_variable_is_clean() {
    // The initializer reads a *different*, already-declared variable.
    assert_no_2448(
        "let lambda = 1;\nlet [mu] = [lambda];\n",
        "reads earlier variable",
    );
}

#[test]
fn deferred_self_reference_in_a_binding_pattern_initializer_is_clean() {
    // The arrow defers execution past the declaration, so it is not a TDZ use —
    // `let z0 = () => z0;` and its binding-pattern form both stay clean.
    assert_no_2448("let [nu] = [() => nu];\n", "deferred arrow in pattern");
}

#[test]
fn ordinary_for_in_and_for_of_loops_stay_clean() {
    assert_no_2448(
        "let xi = { a: 1 };\nfor (let k in xi) { }\nfor (let [v] of [[1]]) { v; }\n",
        "ordinary for-in / for-of",
    );
}

#[test]
fn a_binding_pattern_default_that_reads_itself_is_ts2448() {
    // `let [x2 = x2] = []` — the self-reference is in the element's *default*.
    // This already worked via the binding-element normalization; pin it so the
    // new outer-initializer path does not accidentally regress it.
    assert_has_2448(
        "let [omicron = omicron] = [];\n",
        "pattern default self-ref",
    );
}
