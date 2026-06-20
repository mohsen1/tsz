//! Regression tests for issue #14176.
//!
//! An empty tuple `[]` **matches** an infer pattern whose leading element(s)
//! are optional, e.g. `[(infer H)?, ...infer T]`: the optional prefix can match
//! zero source elements, so the conditional takes its **true** branch. tsz's
//! `match_tuple_elements` (rest-present branch) previously counted the optional
//! prefix as required and rejected the empty source, taking the false branch
//! and inverting the base case of tuple-deconstruction utilities (remeda
//! `TupleParts`/`Head`).
//!
//! Each test pins the *branch selection* by assigning the conditional's true
//! and false literal to a variable: the assignment that contradicts the
//! resolved type must emit `TS2322` and the matching one must be clean. Binder
//! names (type alias, infer variable names) are varied across cases so the fix
//! is structural, not name-driven.

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2322_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

#[test]
fn empty_tuple_matches_optional_prefix_takes_true_branch() {
    // Reported repro: `Test` resolves to "MATCH" (true branch), so assigning the
    // false-branch literal "NO_MATCH" must error TS2322.
    let source = r#"
type Test = readonly [] extends readonly [(infer _H)?, ...infer _T] ? "MATCH" : "NO_MATCH";
const t: Test = "NO_MATCH";
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "[] should match [(infer H)?, ...infer T] (true branch), so assigning the false-branch literal must error"
    );
}

#[test]
fn empty_tuple_matches_optional_prefix_accepts_true_literal() {
    // Same pattern, assigning the true-branch literal must be clean.
    let source = r#"
type Test = readonly [] extends readonly [(infer _H)?, ...infer _T] ? "MATCH" : "NO_MATCH";
const t: Test = "MATCH";
"#;
    assert_eq!(
        ts2322_count(source),
        0,
        "assigning the resolved true-branch literal must type-check cleanly"
    );
}

#[test]
fn negative_control_required_prefix_takes_false_branch() {
    // Negative control: a REQUIRED leading element cannot match the empty
    // source, so the conditional takes the FALSE branch ("NO_MATCH"). The
    // matching assignment is clean; the contradicting one must error.
    let clean = r#"
type Req = readonly [] extends readonly [infer _H, ...infer _T] ? "MATCH" : "NO_MATCH";
const r: Req = "NO_MATCH";
"#;
    assert_eq!(
        ts2322_count(clean),
        0,
        "required prefix must take the false branch, so assigning \"NO_MATCH\" is clean"
    );

    let err = r#"
type Req = readonly [] extends readonly [infer _H, ...infer _T] ? "MATCH" : "NO_MATCH";
const r: Req = "MATCH";
"#;
    assert_eq!(
        ts2322_count(err),
        1,
        "required prefix takes the false branch, so assigning the true-branch literal must error"
    );
}

#[test]
fn two_optional_prefix_elements_match_empty_tuple() {
    // Adjacent case (varied binder names): two optional prefix elements both
    // absorb the missing source, so `[]` still matches (true branch).
    let source = r#"
type Pair = readonly [] extends readonly [(infer Alpha)?, (infer Beta)?, ...infer Rest] ? "YES" : "NO";
const p: Pair = "NO";
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "two optional prefix elements must both match zero source elements (true branch)"
    );
}

#[test]
fn required_then_optional_prefix_rejects_empty_tuple() {
    // Adjacent negative control: a required element followed by an optional one.
    // The required leading element still needs a source element, so `[]` does
    // NOT match and the conditional takes the FALSE branch.
    let clean = r#"
type Mix = readonly [] extends readonly [infer Head, (infer Opt)?, ...infer Tail] ? "YES" : "NO";
const m: Mix = "NO";
"#;
    assert_eq!(
        ts2322_count(clean),
        0,
        "a required leading element keeps the empty source on the false branch"
    );

    let err = r#"
type Mix = readonly [] extends readonly [infer Head, (infer Opt)?, ...infer Tail] ? "YES" : "NO";
const m: Mix = "YES";
"#;
    assert_eq!(
        ts2322_count(err),
        1,
        "false branch is selected, so assigning the true-branch literal must error"
    );
}

#[test]
fn one_element_source_matches_optional_prefix() {
    // Adjacent case: a single-element source fills the optional prefix slot, so
    // the pattern still matches (true branch). Confirms the fix did not regress
    // the previously-accepted in-range arity.
    let source = r#"
type One = readonly [string] extends readonly [(infer _H)?, ...infer _T] ? "MATCH" : "NO_MATCH";
const o: One = "NO_MATCH";
"#;
    assert_eq!(
        ts2322_count(source),
        1,
        "a one-element source filling the optional prefix must still match (true branch)"
    );
}
