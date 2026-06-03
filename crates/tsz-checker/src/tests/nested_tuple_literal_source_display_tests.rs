//! Tests for nested array-literal tuple source display in TS2322 assignability
//! diagnostics.
//!
//! Structural rule: when an array literal is contextually typed against a tuple
//! target and the overall assignment fails (e.g. an arity mismatch), the source
//! type shown in the message must preserve the contextually-typed shape of every
//! nested array-literal element. `tsc` renders the inner element as the tuple it
//! was contextually typed as (e.g. `[1]`), not the non-contextual widened array
//! form (`number[]`).
//!
//! Before the fix, only the *outer* tuple structure survived: each nested
//! array-literal element was re-typed without its contextual tuple slot and
//! widened to `number[]`, so `[[1], [2]]` displayed as `[number[], number[]]`.
//! The fix recurses through `array_literal_tuple_source_type_display` so the
//! inner literal is rendered against its own tuple target slot.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn nested_tuple_literal_fewer_elements_preserves_inner_tuples() {
    // `[[1], [2]]` against `[[1], [2], [3]]`: arity mismatch. The source must
    // render the inner literals as `[1]`, `[2]` (their contextual tuple slots),
    // not the widened `number[]` form.
    let messages = ts2322_messages(
        r#"
const bad: [[1], [2], [3]] = [[1], [2]];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '[[1], [2]]' is not assignable")),
        "inner tuple literals must be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("number[], number[]")),
        "inner literals must not be array-widened, got: {messages:?}"
    );
}

#[test]
fn nested_tuple_literal_extra_element_widens_only_uncovered_slot() {
    // `[[1], [2], [3], [4]]` against `[[1], [2], [3]]`: the first three inner
    // literals keep their tuple slots; the fourth has no contextual slot and
    // widens to `number[]` — matching `tsc` exactly.
    let messages = ts2322_messages(
        r#"
const bad: [[1], [2], [3]] = [[1], [2], [3], [4]];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '[[1], [2], [3], number[]]' is not assignable")),
        "covered slots preserved, uncovered slot widened, got: {messages:?}"
    );
}

#[test]
fn deeply_nested_tuple_literal_preserves_all_levels() {
    // Two levels of nesting: the inner `[[1], [2], [3]]` keeps its full shape
    // even though the outer literal is shorter than its tuple target.
    let messages = ts2322_messages(
        r#"
const bad: [[[1], [2], [3]], string] = [[[1], [2], [3]]];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '[[[1], [2], [3]]]' is not assignable")),
        "all nesting levels must be preserved, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("number[][]")),
        "nested literals must not collapse to arrays, got: {messages:?}"
    );
}

#[test]
fn nested_string_literal_tuple_preserved() {
    // The rule is structural, not number-specific: string-literal tuples are
    // preserved the same way.
    let messages = ts2322_messages(
        r#"
const bad: [["a"], ["b"]] = [["a"]];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"Type '[["a"]]' is not assignable"#)),
        "string-literal inner tuples must be preserved, got: {messages:?}"
    );
}

#[test]
fn array_typed_target_slot_still_widens() {
    // When the target slot is an array (not a tuple), the inner array literal is
    // genuinely an array and must stay widened — the recursion declines and the
    // existing widened-array fallback applies, matching `tsc`.
    let messages = ts2322_messages(
        r#"
const bad: [number[], number[]] = [[1]];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '[number[]]' is not assignable")),
        "array-typed slots keep the widened form, got: {messages:?}"
    );
}

#[test]
fn mixed_flat_and_nested_elements_preserved() {
    // A tuple mixing a flat literal and a nested tuple element keeps both forms.
    let messages = ts2322_messages(
        r#"
const bad: [1, [2, 3], 4] = [1, [2, 3]];
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '[1, [2, 3]]' is not assignable")),
        "flat and nested elements both preserved, got: {messages:?}"
    );
}
