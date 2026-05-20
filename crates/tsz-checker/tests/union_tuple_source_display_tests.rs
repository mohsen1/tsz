//! Tests for TS2322 source-type display when assigning a tuple literal to a
//! union-of-tuples type.
//!
//! **Structural rule**: When a tuple literal is assigned to a union of same-arity
//! fixed-length tuples and the assignment fails, the error must display the
//! source with its literal element types (e.g. `["A", "A"]`), not the widened
//! primitives (e.g. `[string, string]`).
//!
//! **Root cause fixed**: `array_literal_tuple_source_type_display` returned `None`
//! for union-of-tuple targets because it only handled plain tuple targets.
//! The `None` caused the caller to fall through to `widen_type_for_display`,
//! which widens string/number/boolean literal element types to their primitives.
//!
//! **Owner layer**: Diagnostic display only — no change to assignability semantics.
//! The fix builds per-position synthetic `TupleElement`s (whose `type_id` is the
//! union of that position's type across all union member tuples), allowing the
//! existing literal-preservation logic to detect literal types at each position.

fn messages(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn ts2322_source(source: &str) -> String {
    messages(source)
        .into_iter()
        .find(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Core reported repro
// ---------------------------------------------------------------------------

/// The canonical repro from the issue: `["A","A"]` must appear in the error
/// message, not `[string, string]`.
#[test]
fn union_tuple_target_preserves_literal_source_display() {
    let msg = ts2322_source(
        r#"
type AB = ["A","B"] | ["B","A"];
const x: AB = ["A","A"];
"#,
    );
    assert!(
        msg.contains(r#""A""#),
        "expected literal \"A\" in message, got: {msg}"
    );
    assert!(
        !msg.contains("string, string"),
        "expected no widened `string, string`, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Name-independence: different bound-variable names must give the same result
// ---------------------------------------------------------------------------

/// Same shape with different string literals confirms the rule is not keyed
/// on the identifier name `AB`.
#[test]
fn union_tuple_target_different_literal_names_still_preserves() {
    let msg = ts2322_source(
        r#"
type PQ = ["P","Q"] | ["Q","P"];
const x: PQ = ["P","P"];
"#,
    );
    assert!(
        msg.contains(r#""P""#),
        "expected literal \"P\" in message, got: {msg}"
    );
    assert!(
        !msg.contains("string, string"),
        "expected no widened `string, string`, got: {msg}"
    );
}

/// A three-character literal set proves the fix is not tied to two-member
/// character sets.
#[test]
fn union_tuple_target_abc_literals_preserved() {
    let msg = ts2322_source(
        r#"
type Rot = ["A","B","C"] | ["B","C","A"] | ["C","A","B"];
const x: Rot = ["A","A","A"];
"#,
    );
    assert!(
        msg.contains(r#""A""#),
        "expected literal \"A\" in message, got: {msg}"
    );
    assert!(
        !msg.contains("string"),
        "expected no widened `string`, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Other literal kinds
// ---------------------------------------------------------------------------

/// Number literals in union-of-tuples targets are also preserved.
#[test]
fn union_tuple_target_number_literals_preserved() {
    let msg = ts2322_source(
        r#"
type Swap = [1, 2] | [2, 1];
const x: Swap = [1, 1];
"#,
    );
    assert!(
        msg.contains("1, 1"),
        "expected literal `1, 1` in message, got: {msg}"
    );
    assert!(
        !msg.contains("number, number"),
        "expected no widened `number, number`, got: {msg}"
    );
}

/// Boolean literals in union-of-tuples targets are preserved.
#[test]
fn union_tuple_target_boolean_literals_preserved() {
    let msg = ts2322_source(
        r#"
type TF = [true, false] | [false, true];
const x: TF = [true, true];
"#,
    );
    assert!(
        msg.contains("true, true"),
        "expected `true, true` in message, got: {msg}"
    );
    assert!(
        !msg.contains("boolean, boolean"),
        "expected no widened `boolean, boolean`, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Correct assignments must remain error-free
// ---------------------------------------------------------------------------

/// The valid members of the union must still be accepted.
#[test]
fn valid_union_tuple_assignments_are_accepted() {
    let codes = tsz_checker::test_utils::check_source_codes(
        r#"
type AB = ["A","B"] | ["B","A"];
const x: AB = ["A","B"];
const y: AB = ["B","A"];
"#,
    );
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for valid assignments, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative / fallback cases
// ---------------------------------------------------------------------------

/// A union with mixed arities falls back gracefully (no panic, no wrong
/// literal display — just the general widened form or similar).
#[test]
fn mixed_arity_union_tuple_does_not_panic() {
    // This should produce TS2322 without panicking. The exact message is
    // unspecified (it may be widened); the key requirement is stability.
    let msg = ts2322_source(
        r#"
type Uneven = ["A","B"] | ["X"];
const x: Uneven = ["Z","Z"];
"#,
    );
    // At minimum an error was reported; its form is unspecified for mixed arities.
    assert!(!msg.is_empty(), "expected some TS2322 message, got none");
}

/// A non-tuple member in the union disables the literal-preservation path.
#[test]
fn non_tuple_union_member_does_not_panic() {
    let msg = ts2322_source(
        r#"
type T = ["A","B"] | string;
const x: T = ["Z","Z"];
"#,
    );
    assert!(!msg.is_empty(), "expected some TS2322 message, got none");
}
