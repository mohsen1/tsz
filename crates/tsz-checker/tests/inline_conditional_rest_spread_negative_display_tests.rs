//! Negative-case display parity for inline-`Conditional` rest-spread call
//! parameters (residual of #6475, tracked by #13956).
//!
//! When an argument to a call whose rest parameter is an *inline* conditional
//! tuple element — `...[key: K, ...(M[K] extends void ? [] : [value: M[K]])]` —
//! has the wrong type, tsc reports the payload argument against the evaluated
//! `value` element (`string`), not against the `key` element (`"log"`):
//!
//! ```text
//! TS2345: Argument of type 'number' is not assignable to parameter of type 'string'.
//! ```
//!
//! The accept path already flattens the inline conditional rest element, so the
//! relation result was correct; only the *displayed* expected type diverged.
//! The defect was the literal-display heuristic in `handle_call_result`: it
//! repainted the widened-primitive `expected` (`string`) with a sibling
//! argument's literal whenever that literal widened to the same primitive —
//! here the `key` literal `"log"` — even though the mismatched parameter slot
//! (`value: M[K]`) is a structural type, not the bare type parameter the
//! sibling pinned.
//!
//! The tests vary interface / type-parameter / function / property names to
//! prove the rule is structural and name-agnostic (see `.claude/CLAUDE.md`).

use tsz_checker::test_utils::check_source_code_messages as diagnostics;

fn ts2345_messages(source: &str) -> Vec<String> {
    diagnostics(source)
        .into_iter()
        .filter_map(|(code, msg)| (code == 2345).then_some(msg))
        .collect()
}

// The #13956 witness: the payload `123` is reported against the evaluated
// `value` element (`string`), not the `key` element (`"log"`).
#[test]
fn inline_conditional_rest_payload_reports_value_element_not_key() {
    let source = r#"
interface Handlers { log: string; stop: void; }
declare function run<K extends keyof Handlers>(
    ...args: [key: K, ...(Handlers[K] extends void ? [] : [value: Handlers[K]])]
): void;
run("log", 123);
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'number'")
            && msgs[0].contains("parameter of type 'string'"),
        "inline conditional rest payload must report the value element, got: {msgs:#?}"
    );
}

// Renaming every binder (interface, type parameter, function, members) must not
// change the result — the rule is structural, not bound to `Handlers`/`K`/etc.
#[test]
fn inline_conditional_rest_payload_display_is_structural_under_renaming() {
    let source = r#"
interface EventMap { click: number; ready: void; }
declare function dispatch<E extends keyof EventMap>(
    ...parts: [name: E, ...(EventMap[E] extends void ? [] : [payload: EventMap[E]])]
): void;
dispatch("click", "oops");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'string'")
            && msgs[0].contains("parameter of type 'number'"),
        "renamed binders must still report the value element, got: {msgs:#?}"
    );
}

// Method form: the same signature on a class method routes through the same
// display path and must report the value element.
#[test]
fn inline_conditional_rest_payload_method_form_reports_value_element() {
    let source = r#"
interface Signals { warn: string; idle: void; }
class Bus {
    emit<S extends keyof Signals>(
        ...args: [signal: S, ...(Signals[S] extends void ? [] : [value: Signals[S]])]
    ): void {}
}
new Bus().emit("warn", 42);
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'number'")
            && msgs[0].contains("parameter of type 'string'"),
        "method-form inline conditional rest must report the value element, got: {msgs:#?}"
    );
}

// Structured (object-typed) payload: the value element is an object type, so the
// reported expected type must be that object, never the `key` literal.
#[test]
fn inline_conditional_rest_structured_payload_reports_value_element() {
    let source = r#"
interface Routes { open: { id: string }; close: void; }
declare function navigate<R extends keyof Routes>(
    ...args: [route: R, ...(Routes[R] extends void ? [] : [params: Routes[R]])]
): void;
navigate("open", 7);
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type 'number'")
            && msgs[0].contains("parameter of type '{ id: string; }'"),
        "structured payload must report the value (object) element, got: {msgs:#?}"
    );
}

// Positive control: the sibling-literal display heuristic must STILL fire when
// the mismatched parameter slot is genuinely the bare type parameter the
// sibling pinned (`f<T>(a: T, b: T)`), so this fix is narrow.
#[test]
fn bare_type_parameter_sibling_literal_display_is_preserved() {
    let source = r#"
declare function couple<T>(first: T, second: T): T;
couple(1, "");
"#;
    let msgs = ts2345_messages(source);
    assert_eq!(msgs.len(), 1, "expected one TS2345, got: {msgs:#?}");
    assert!(
        msgs[0].contains("Argument of type '\"\"'") && msgs[0].contains("parameter of type '1'"),
        "bare type-parameter sibling literal display must be preserved, got: {msgs:#?}"
    );
}
