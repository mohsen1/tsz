//! Tests for issue #13040: rendering one diagnostic must do bounded work.
//!
//! Structural rule: when a diagnostic's displayed type is a self-expanding
//! generic (each evaluation step interns fresh `TypeId`s, so cycle sets and
//! per-`TypeId` memos never converge), tsc renders a truncated display in
//! bounded time; tsz does the same through the per-rendered-type display
//! budget (`error_reporter::display_budget`), which caps normalization node
//! visits and evaluation fuel instead of re-running full type evaluation per
//! node of an unbounded expansion.
//!
//! The mechanics of the budget itself (exhaustion, scoping, memo) are unit
//! tested in `error_reporter::display_budget`. These tests pin the observable
//! contract: diagnostics on self-expanding generics are still emitted with
//! the correct code, and ordinary diagnostics render byte-identical messages
//! (the budget is far above what any realistic message consumes).

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn self_expanding_generic_interface_source_still_reports_ts2322() {
    // Every property nests a structurally fresh instantiation (`Wrap<Wrap<T>>`,
    // `Wrap<[T]>`, `Wrap<{ v: T }>`), so display normalization can never
    // converge by `TypeId` identity; it must terminate via the work budget.
    let codes = codes(
        r#"
interface Wrap<T> {
    one: Wrap<Wrap<T>>;
    two: Wrap<[T]>;
    three: Wrap<{ v: T }>;
    value: T;
}
declare const w: Wrap<string>;
const sink: number = w;
"#,
    );
    assert!(
        codes.contains(&2322),
        "self-expanding interface source must still report TS2322, got: {codes:?}"
    );
}

#[test]
fn self_expanding_generic_alias_argument_still_reports_ts2345() {
    // Same family through the call-argument display path (the sampled hot
    // stack in #13040 was TS2345 emission), with renamed binders and a
    // generic alias instead of an interface.
    let codes = codes(
        r#"
type Grow<U> = {
    left: Grow<{ a: U }>;
    right: Grow<{ b: U }>;
    middle: Grow<[U, U]>;
    leaf: U;
};
declare const g: Grow<"seed">;
declare function take(n: number): void;
take(g);
"#,
    );
    assert!(
        codes.contains(&2345),
        "self-expanding alias argument must still report TS2345, got: {codes:?}"
    );
}

#[test]
fn self_expanding_promise_chain_target_still_reports_ts2322() {
    // Awaited-style async expansion: the negative/fallback direction where
    // the *target* of the assignment is the self-expanding side.
    let codes = codes(
        r#"
interface Chain<T> {
    next: Chain<Promise<T>>;
    alt: Chain<{ inner: T }>;
    value: T;
}
declare const c: Chain<number>;
declare let dst: Chain<string>;
dst = c;
"#,
    );
    assert!(
        codes.contains(&2322),
        "self-expanding target must still report TS2322, got: {codes:?}"
    );
}

#[test]
fn ordinary_diagnostic_messages_are_unchanged_by_the_budget() {
    // Positive control: a small, fully convergent type must render the exact
    // message tsc shows — the budget must be invisible for realistic types.
    let diagnostics = check_source_diagnostics(
        r#"
const plain: { kind: "circle"; radius: number } = { kind: "square", radius: 1 };
"#,
    );
    let messages: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("\"square\"") && m.contains("\"circle\"")),
        "ordinary literal mismatch display must be unaffected, got: {messages:?}"
    );
}
