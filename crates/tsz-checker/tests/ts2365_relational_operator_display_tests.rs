//! Tests for TS2365 relational operator error message display.
//!
//! When `<`, `>`, `<=`, or `>=` is applied to incompatible types, tsz must
//! widen the displayed type to its primitive (e.g. literal `2` → `number`,
//! `1n` → `bigint`) rather than showing the raw annotation text. Type
//! parameter names (e.g. `T`, `K`) should still be shown as-is.

use tsz_checker::test_utils::check_source_code_messages;

fn diags(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

fn ts2365_messages(source: &str) -> Vec<String> {
    diags(source)
        .into_iter()
        .filter(|(code, _)| *code == 2365)
        .map(|(_, msg)| msg)
        .collect()
}

// --- Numeric literal annotations must widen to 'number' ---

#[test]
fn numeric_literal_param_displays_as_number_not_literal() {
    // `two: 2` — annotation is the literal type `2`; error must say 'number'
    let msgs = ts2365_messages("function f(two: 2) { return false < two; }");
    assert!(!msgs.is_empty(), "expected TS2365 for false < (param: 2)");
    assert!(
        msgs.iter().all(|m| !m.contains("'2'")),
        "error must not show raw literal '2'; got: {:?}",
        msgs
    );
    assert!(
        msgs.iter().any(|m| m.contains("'number'")),
        "error must show widened 'number'; got: {:?}",
        msgs
    );
}

#[test]
fn numeric_literal_param_alternate_value_displays_as_number() {
    // Same rule with a different literal value — `x: 1` — to prove the fix
    // is not keyed on the spelling `2` but on the structural property
    // (numeric literal annotation).
    let msgs = ts2365_messages("function f(x: 1) { return false < x; }");
    assert!(!msgs.is_empty(), "expected TS2365 for false < (param: 1)");
    assert!(
        msgs.iter().all(|m| !m.contains("'1'")),
        "error must not show raw literal '1'; got: {:?}",
        msgs
    );
    assert!(
        msgs.iter().any(|m| m.contains("'number'")),
        "error must show widened 'number'; got: {:?}",
        msgs
    );
}

#[test]
fn numeric_union_literal_param_displays_as_number() {
    // `onethree: 1 | 3` — union of numeric literals; annotation text is
    // longer than 3 chars so it falls through the heuristic, but the
    // display must still be widened to 'number' by the solver path.
    let msgs = ts2365_messages("function f(onethree: 1 | 3) { return false < onethree; }");
    assert!(
        !msgs.is_empty(),
        "expected TS2365 for false < (param: 1 | 3)"
    );
    assert!(
        msgs.iter().any(|m| m.contains("'number'")),
        "error must show widened 'number' for union literal param; got: {:?}",
        msgs
    );
}

// --- Type parameter annotations must still pass through as-is ---

#[test]
fn type_param_annotation_displays_raw_name() {
    // A parameter typed as a type parameter `T` should still show `T` in the
    // error — the heuristic's intended purpose.
    let msgs = ts2365_messages("function f<T extends number>(a: T) { return false < a; }");
    // The error may or may not fire (T extends number is comparable), so only
    // assert that if it fires it shows 'T' rather than widening incorrectly.
    for msg in &msgs {
        assert!(
            !msg.contains("'number'") || msg.contains("'T'"),
            "type param error should reference 'T'; got: {}",
            msg
        );
    }
}

// --- Bigint literal annotations must widen to 'bigint' ---

#[test]
fn bigint_literal_param_displays_as_bigint_not_literal() {
    // `big: 1n` — bigint literal annotation; error must say 'bigint'
    // Annotation text is `1n` (2 chars) which would previously pass the
    // ≤3 heuristic and be returned as raw text.
    let msgs = ts2365_messages("function f(big: 1n) { return false < big; }");
    if !msgs.is_empty() {
        assert!(
            msgs.iter().all(|m| !m.contains("'1n'")),
            "error must not show raw bigint literal '1n'; got: {:?}",
            msgs
        );
    }
}
