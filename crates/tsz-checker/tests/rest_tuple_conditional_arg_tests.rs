//! Tests for false-positive TS2345 when rest parameters use conditional tuple types.
//!
//! When a function's rest parameter has the form `...args: [label: K, ...ConditionalArgs<T,K>]`
//! and `ConditionalArgs<T,K>` evaluates to a concrete tuple (e.g. `[data: D]` or `[]`),
//! the solver must evaluate that Application/Conditional type before treating it as a
//! variadic spread, so argument assignability is checked against the concrete element types.
//!
//! Root cause: `expand_tuple_rest` and the Tuple instantiator rest-element flattening
//! did not evaluate Application/Conditional/Lazy types before fallback, causing the
//! expected type for the second argument to remain as `Application(...)` instead of
//! the concrete evaluated type.

use tsz_checker::test_utils::check_source_codes;

// ---------------------------------------------------------------------------
// Issue #6475: exact reproduction
// ---------------------------------------------------------------------------

/// The exact pattern from issue #6475: `EventEmitter` using a conditional
/// tuple rest parameter. Calling with a matching payload must NOT produce TS2345.
#[test]
fn emit_with_matching_data_payload_no_ts2345() {
    let codes = check_source_codes(
        r#"
type EventArgs<E, K extends keyof E> = E[K] extends void ? [] : [data: E[K]];

interface Events {
    click: { x: number; y: number };
    focus: void;
}

declare class EventEmitter<E> {
    emit<K extends keyof E>(...args: [event: K, ...EventArgs<E, K>]): void;
}

declare const emitter: EventEmitter<Events>;
emitter.emit("click", { x: 1, y: 2 });
"#,
    );
    assert!(
        !codes.contains(&2345),
        "expected no TS2345 for valid emit call, got: {codes:?}"
    );
}

/// The void-event case: `focus` maps to `EventArgs<Events, "focus">` which evaluates to `[]`,
/// so `emit("focus")` takes no second argument. Must NOT produce TS2345.
#[test]
fn emit_void_event_no_payload_no_ts2345() {
    let codes = check_source_codes(
        r#"
type EventArgs<E, K extends keyof E> = E[K] extends void ? [] : [data: E[K]];

interface Events {
    click: { x: number; y: number };
    focus: void;
}

declare class EventEmitter<E> {
    emit<K extends keyof E>(...args: [event: K, ...EventArgs<E, K>]): void;
}

declare const emitter: EventEmitter<Events>;
emitter.emit("focus");
"#,
    );
    assert!(
        !codes.contains(&2345),
        "expected no TS2345 for void event with no payload, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Generalisation: different type-parameter names must behave identically
// ---------------------------------------------------------------------------

/// Same semantic, different names: `T` instead of `E`, `N` instead of `K`.
/// The fix must be structural, not keyed on spelling.
#[test]
fn different_type_param_names_still_no_ts2345() {
    let codes = check_source_codes(
        r#"
type Args<T, N extends keyof T> = T[N] extends void ? [] : [payload: T[N]];

interface Signals {
    resize: { width: number; height: number };
    hide: void;
}

declare function dispatch<T, N extends keyof T>(
    ...args: [name: N, ...Args<T, N>]
): void;

declare const _ : <N extends keyof Signals>(...args: [name: N, ...Args<Signals, N>]) => void;

_(  "resize" as "resize", { width: 100, height: 200 });
"#,
    );
    assert!(
        !codes.contains(&2345),
        "expected no TS2345 with different type-param names, got: {codes:?}"
    );
}

/// Inline conditional type (not via a named alias) in a rest spread position.
///
/// Regression guard (was the residual known-limitation of #6475): evaluating an
/// inline `Conditional` rest element (not wrapped in a named type-alias
/// `Application`) from within argument checking now reduces it to its concrete
/// tuple form, so `run("log", "hello")` checks the payload against the evaluated
/// `value: Handlers[K]` element instead of treating the whole conditional as an
/// opaque variadic. The named-alias `Application` form was fixed by #6475; this
/// inline form is now covered by the same `evaluate_tuple_rest_elements`
/// pre-pass over rest elements.
#[test]
fn inline_conditional_rest_spread_no_ts2345() {
    let codes = check_source_codes(
        r#"
interface Handlers {
    log: string;
    stop: void;
}

declare function run<K extends keyof Handlers>(
    ...args: [
        key: K,
        ...(Handlers[K] extends void ? [] : [value: Handlers[K]])
    ]
): void;

run("log", "hello");
"#,
    );
    assert!(
        !codes.contains(&2345),
        "expected no TS2345 for inline conditional rest spread, got: {codes:?}"
    );
}

/// Void variant of the inline conditional.
#[test]
fn inline_conditional_rest_spread_void_no_ts2345() {
    let codes = check_source_codes(
        r#"
interface Handlers {
    log: string;
    stop: void;
}

declare function run<K extends keyof Handlers>(
    ...args: [
        key: K,
        ...(Handlers[K] extends void ? [] : [value: Handlers[K]])
    ]
): void;

run("stop");
"#,
    );
    assert!(
        !codes.contains(&2345),
        "expected no TS2345 for void inline conditional rest spread, got: {codes:?}"
    );
}

/// Renamed binders for the inline conditional (anti-hardcoding): the fix must
/// be structural, not keyed on `Handlers`/`K`/`run` spellings.
#[test]
fn inline_conditional_rest_spread_renamed_binders_no_ts2345() {
    let codes = check_source_codes(
        r#"
interface Signals {
    resize: { w: number };
    hide: void;
}

declare function dispatch<N extends keyof Signals>(
    ...args: [
        name: N,
        ...(Signals[N] extends void ? [] : [payload: Signals[N]])
    ]
): void;

dispatch("resize", { w: 1 });
dispatch("hide");
"#,
    );
    assert!(
        !codes.contains(&2345),
        "renamed-binder inline conditional must behave identically; got: {codes:?}"
    );
}

/// Method (not free function) carrying the inline conditional rest spread, with
/// a structured payload — the kysely/event-emitter shape the residual blocked.
#[test]
fn inline_conditional_rest_spread_method_structured_payload_no_ts2345() {
    let codes = check_source_codes(
        r#"
interface Ev {
    click: { x: number; y: number };
    focus: void;
}

declare class Emitter {
    on<K extends keyof Ev>(
        ...args: [event: K, ...(Ev[K] extends void ? [] : [data: Ev[K]])]
    ): void;
}

declare const e: Emitter;
e.on("click", { x: 1, y: 2 });
e.on("focus");
"#,
    );
    assert!(
        !codes.contains(&2345),
        "method inline conditional rest spread must accept the structured payload; got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: wrong payloads must still produce TS2345
// ---------------------------------------------------------------------------

/// Passing a wrong payload type MUST still produce TS2345.
#[test]
fn emit_wrong_payload_type_produces_ts2345() {
    let codes = check_source_codes(
        r#"
type EventArgs<E, K extends keyof E> = E[K] extends void ? [] : [data: E[K]];

interface Events {
    click: { x: number; y: number };
    focus: void;
}

declare class EventEmitter<E> {
    emit<K extends keyof E>(...args: [event: K, ...EventArgs<E, K>]): void;
}

declare const emitter: EventEmitter<Events>;
emitter.emit("click", "not-an-object");
"#,
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345 for wrong payload type, got: {codes:?}"
    );
}

/// Passing an extra argument to a void event MUST still produce an error.
#[test]
fn emit_extra_arg_for_void_event_produces_error() {
    let codes = check_source_codes(
        r#"
type EventArgs<E, K extends keyof E> = E[K] extends void ? [] : [data: E[K]];

interface Events {
    click: { x: number; y: number };
    focus: void;
}

declare class EventEmitter<E> {
    emit<K extends keyof E>(...args: [event: K, ...EventArgs<E, K>]): void;
}

declare const emitter: EventEmitter<Events>;
emitter.emit("focus", "unexpected");
"#,
    );
    assert!(
        !codes.is_empty(),
        "expected at least one error for extra arg on void event, got no errors"
    );
}

// ---------------------------------------------------------------------------
// Issue #13956: the wrong-payload diagnostic must name the *evaluated value
// element* of the inline-conditional rest spread, not the *key* element.
//
// Residual of #6475: the accept path correctly flattens the inline
// `Conditional` rest element, but the failing-argument diagnostic display
// re-derived the expected type via the generic-call literal-reconstruction
// heuristic, which substituted the sibling `key` literal (whose widened base
// coincided with the real `string` target). The reconstruction must be gated
// to bare type-parameter slots, so a derived `value: Handlers[K]` slot keeps
// its concrete `string` expected.
// ---------------------------------------------------------------------------

use tsz_checker::test_utils::check_source_code_messages;

fn ts2345_message(source: &str) -> String {
    let msgs = check_source_code_messages(source);
    msgs.into_iter()
        .find_map(|(code, m)| (code == 2345).then_some(m))
        .unwrap_or_else(|| panic!("expected a TS2345 for source:\n{source}"))
}

/// The exact witness: `run("log", 123)` must report `number` vs `string`
/// (the evaluated `value: Handlers["log"]` element), never `123` vs `"log"`.
#[test]
fn inline_conditional_rest_spread_wrong_payload_names_value_element() {
    let msg = ts2345_message(
        r#"
interface Handlers { log: string; stop: void; }
declare function run<K extends keyof Handlers>(
    ...args: [key: K, ...(Handlers[K] extends void ? [] : [value: Handlers[K]])]
): void;
run("log", 123);
"#,
    );
    assert!(
        msg.contains("Argument of type 'number' is not assignable to parameter of type 'string'."),
        "expected the value-element target, got: {msg}"
    );
    assert!(
        !msg.contains("\"log\""),
        "must not report the payload against the key element, got: {msg}"
    );
}

/// Renamed binders (anti-hardcoding): the fix is structural, not keyed on
/// `Handlers`/`K`/`run`.
#[test]
fn inline_conditional_rest_spread_wrong_payload_renamed_binders() {
    let msg = ts2345_message(
        r#"
interface Signals { resize: string; hide: void; }
declare function dispatch<N extends keyof Signals>(
    ...args: [name: N, ...(Signals[N] extends void ? [] : [payload: Signals[N]])]
): void;
dispatch("resize", 42);
"#,
    );
    assert!(
        msg.contains("Argument of type 'number' is not assignable to parameter of type 'string'."),
        "renamed-binder inline conditional must name the value element, got: {msg}"
    );
    assert!(
        !msg.contains("\"resize\""),
        "must not name the key element, got: {msg}"
    );
}

/// Method form with a structured (object) payload: the expected type is the
/// structured `data` element, not the `event` key literal.
#[test]
fn inline_conditional_rest_spread_wrong_payload_structured_value() {
    let msg = ts2345_message(
        r#"
interface Ev { click: { x: number; y: number }; focus: void; }
declare class Emitter {
    on<K extends keyof Ev>(
        ...args: [event: K, ...(Ev[K] extends void ? [] : [data: Ev[K]])]
    ): void;
}
declare const e: Emitter;
e.on("click", "not-an-object");
"#,
    );
    assert!(
        msg.contains("{ x: number; y: number; }"),
        "expected the structured value element as target, got: {msg}"
    );
    assert!(
        !msg.contains("\"click\""),
        "must not name the key element, got: {msg}"
    );
}

/// Parity reference: the named-alias `Application` form already reported the
/// right target; it must keep doing so (the value element, not the key).
#[test]
fn named_alias_rest_spread_wrong_payload_names_value_element() {
    let msg = ts2345_message(
        r#"
type EventArgs<E, K extends keyof E> = E[K] extends void ? [] : [value: E[K]];
interface Handlers { log: string; stop: void; }
declare function run<K extends keyof Handlers>(...args: [key: K, ...EventArgs<Handlers, K>]): void;
run("log", 123);
"#,
    );
    assert!(
        !msg.contains("\"log\""),
        "named-alias form must not name the key element either, got: {msg}"
    );
}

/// Guard the legitimate sibling-literal reconstruction the gate must preserve:
/// for a *bare* type-parameter slot (`foo<T>(x: T, y: T)` with `T` exposed via
/// the return type), the displayed expected stays the inferred literal `1`.
#[test]
fn bare_type_param_slot_still_reconstructs_sibling_literal() {
    let msg = ts2345_message("function foo<T>(x: T, y: T): T { return y; }\nfoo(1, '');\n");
    assert!(
        msg.contains("Argument of type '\"\"' is not assignable to parameter of type '1'."),
        "bare type-parameter slot must keep the inferred-literal display, got: {msg}"
    );
}
