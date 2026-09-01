//! Regression tests for narrowingUnionToNeverAssigment.ts: a literal-union
//! assignment SOURCE must display its literal members verbatim in the TS2322
//! message, not its widened primitive base.
//!
//! Structural rule: when a non-fresh literal union (declared annotation, named
//! alias, or flow-narrowed residue) is the assignability source, tsc renders
//! the union spelling (`"c" | "d"`). tsz must not widen a non-fresh union
//! source to its primitive base (`string`) for display. Target-position unions
//! already render correctly; only the source side was widening.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

#[test]
fn declared_string_literal_union_source_keeps_union_display() {
    let messages = ts2322_messages(
        r#"
declare const w: "c" | "d";
const y: never = w;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"Type '"c" | "d"' is not assignable to type 'never'"#)),
        "declared string-literal union source must render verbatim, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("Type 'string' is not assignable to type 'never'")),
        "source union must not widen to 'string', got: {messages:?}"
    );
}

#[test]
fn declared_number_literal_union_source_keeps_union_display() {
    let messages = ts2322_messages(
        r#"
declare const w: 1 | 2;
const y: never = w;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("Type '1 | 2' is not assignable to type 'never'")),
        "declared number-literal union source must render verbatim, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("Type 'number' is not assignable to type 'never'")),
        "source union must not widen to 'number', got: {messages:?}"
    );
}

#[test]
fn flow_narrowed_union_source_keeps_union_display() {
    // The original witness: `x` is narrowed to `"c" | "d"` in the else branch.
    let messages = ts2322_messages(
        r#"
type Variants = "a" | "b" | "c" | "d";
function fx1(x: Variants) {
    if (x === "a" || x === "b") {
    } else {
        const y: never = x;
    }
}
"#,
    );
    // tsc renders the flow-narrowed residual union verbatim (member order
    // follows the narrowing, e.g. `"d" | "c"`), never its widened base.
    assert!(
        messages.iter().any(|m| {
            m.ends_with("is not assignable to type 'never'.")
                && m.contains(r#""c""#)
                && m.contains(r#""d""#)
        }),
        "flow-narrowed union source must render its literal members, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("Type 'string' is not assignable to type 'never'")),
        "flow-narrowed union source must not widen to 'string', got: {messages:?}"
    );
}

#[test]
fn literal_union_target_position_still_renders_verbatim() {
    // Guard: the union in TARGET position already renders correctly; the source
    // fix must not disturb it.
    let messages = ts2322_messages(
        r#"
declare const s: string;
const z: "c" | "d" = s;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains(r#"is not assignable to type '"c" | "d"'"#)),
        "target-position union must render verbatim, got: {messages:?}"
    );
}
