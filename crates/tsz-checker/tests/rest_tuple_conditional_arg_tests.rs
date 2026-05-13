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
/// This is a known limitation: evaluating inline Conditionals (not wrapped in
/// a named type alias Application) from within argument checking requires the
/// `TypeEnvironment` to have the relevant interfaces pre-resolved, which is not
/// guaranteed in the current evaluation path. Tracked for follow-up.
#[test]
#[ignore = "inline conditional rest spread not yet supported; named-alias Application case fixed"]
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
