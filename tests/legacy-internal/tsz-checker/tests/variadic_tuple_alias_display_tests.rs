//! Tests for issue #10817: variadic (spread) tuple alias applications must
//! display as their resolved tuple form in diagnostics, matching tsc.
//!
//! Structural rule: tsc instantiates spread tuple aliases (`[T, ...A]`,
//! `[...A, ...B]`, recursive tuple-building conditionals such as `Zip`/`Reverse`)
//! through tuple spreading, which produces a fresh tuple carrying no
//! `aliasSymbol`. The printer therefore renders the structural tuple
//! (`[1, 2, 3]`) instead of the named application (`Prepend<1, [2, 3]>`).
//! Non-spread tuple aliases (`Pair<A, B> = [A, B]`) keep their alias name.
//!
//! tsz had been preserving the named application form for variadic tuple
//! aliases, both via the evaluator's display-alias provenance (for types that
//! flow through inference) and via the formatter (for raw applications). This
//! locks in the resolved-tuple display.

use crate::test_utils::check_source_diagnostics;

fn ts2322_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text)
        .collect()
}

/// A recursive variadic-tuple utility (`Zip`) whose result flows through a
/// function call: the inferred type is the evaluated tuple, so the diagnostic
/// must show the flattened tuple, not `Zip<...>` / `Prepend<...>`.
#[test]
fn zip_result_through_inference_displays_flattened_tuple() {
    let messages = ts2322_messages(
        r#"
type Prepend<T, A extends any[]> = [T, ...A];
type Zip<A extends any[], B extends any[]> =
  A extends [infer AH, ...infer AT]
    ? B extends [infer BH, ...infer BT]
      ? Prepend<[AH, BH], Zip<AT, BT>>
      : []
    : [];
declare function zip<A extends any[], B extends any[]>(a: A, b: B): Zip<A, B>;
const z = zip([1, 2, 3] as [1, 2, 3], ["a", "b", "c"] as [string, boolean, string]);
const bad: [[1, "x"]] = z;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("[[1, string], [2, boolean], [3, string]]")),
        "Zip result should display as the flattened tuple, got: {messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("Zip<") || m.contains("Prepend<")),
        "named variadic application form must not leak into the message, got: {messages:?}"
    );
}

/// A bare `Concat<[1, 2], [3, 4, 5]>` annotation (no inference): the formatter
/// must locally flatten the concrete spread-tuple alias.
#[test]
fn concat_application_displays_flattened_tuple() {
    let messages = ts2322_messages(
        r#"
type Concat<A extends any[], B extends any[]> = [...A, ...B];
declare const c: Concat<[1, 2], [3, 4, 5]>;
const bad: [1, 2, 3, 4] = c;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("[1, 2, 3, 4, 5]")),
        "Concat should display as the flattened tuple, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Concat<")),
        "named Concat application must not leak into the message, got: {messages:?}"
    );
}

/// `Prepend<1, [2, 3]>` — fixed-prefix spread tuple alias.
#[test]
fn prepend_application_displays_flattened_tuple() {
    let messages = ts2322_messages(
        r#"
type Prepend<T, A extends any[]> = [T, ...A];
declare const p: Prepend<1, [2, 3]>;
const bad: [9] = p;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("[1, 2, 3]")),
        "Prepend should display as the flattened tuple, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Prepend<")),
        "named Prepend application must not leak into the message, got: {messages:?}"
    );
}

/// A *fixed* (non-spread) tuple alias keeps its name — the suppression must be
/// scoped to variadic bodies only.
#[test]
fn fixed_tuple_alias_keeps_name() {
    let messages = ts2322_messages(
        r#"
type Pair<A, B> = [A, B];
declare const p: Pair<1, 2>;
const bad: 0 = p;
"#,
    );
    assert!(
        messages.iter().any(|m| m.contains("Pair<1, 2>")),
        "fixed tuple alias should keep its name, got: {messages:?}"
    );
}
