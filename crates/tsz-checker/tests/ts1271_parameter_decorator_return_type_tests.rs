//! Tests for TS1271 on legacy (`experimentalDecorators`) parameter decorators.
//!
//! Structural rule: when a parameter decorator's resolved call returns a type
//! that is not assignable to `void` (and is not `any`), `tsc` reports TS1271
//! (`Decorator function return type is '{0}' but is expected to be 'void' or
//! 'any'.`) at the decorator's expression; tsz reports it through the checker's
//! `check_parameter_decorator_return_type`, the sibling of the identical check
//! already run for property decorators.
//!
//! Before this pin the parameter half of that predicate did not exist: the
//! checker resolved the `(target, key, index)` call only to decide TS1239 and
//! discarded the return type, so a value-returning parameter decorator was
//! silently accepted.
//!
//! Every row is oracle-verified against the pinned `typescript@7.0.2`, and the
//! binder names are varied across rows so nothing keys on a user-chosen
//! spelling (anti-hardcoding per CLAUDE.md).

use tsz_checker::test_utils::{check_source_codes, check_source_codes_experimental_decorators};

fn check_experimental(source: &str) -> Vec<u32> {
    check_source_codes_experimental_decorators(source)
}

/// tsc: `m.ts(12,7): error TS1271 ... return type is 'number' ...`
#[test]
fn method_parameter_decorator_returning_a_value_emits_ts1271() {
    let codes = check_experimental(
        r#"
declare function logged(target: any, key: string | undefined, index: number): number;

class Service {
    handle(@logged payload: number) {}
}
"#,
    );

    assert!(
        codes.contains(&1271),
        "Expected TS1271 for a value-returning method parameter decorator, got: {codes:?}"
    );
}

/// The same rule at the constructor-parameter position, where the runtime
/// passes `undefined` for the key rather than a string.
#[test]
fn constructor_parameter_decorator_returning_a_value_emits_ts1271() {
    let codes = check_experimental(
        r#"
declare function inject(subject: any, member: string | undefined, ordinal: number): number;

class Container {
    constructor(@inject dependency: number) {}
}
"#,
    );

    assert!(
        codes.contains(&1271),
        "Expected TS1271 for a value-returning constructor parameter decorator, got: {codes:?}"
    );
}

/// `unknown` is not assignable to `void`, so it is rejected exactly like a
/// concrete value type — the same row the property-decorator suite pins.
#[test]
fn parameter_decorator_returning_unknown_emits_ts1271() {
    let codes = check_experimental(
        r#"
declare function audited(host: any, slot: string | undefined, at: number): unknown;

class Recorder {
    write(@audited entry: number) {}
}
"#,
    );

    assert!(
        codes.contains(&1271),
        "Expected TS1271 for an unknown-returning parameter decorator, got: {codes:?}"
    );
}

/// A decorator factory (`@make()`) whose produced decorator returns a value is
/// judged on the produced decorator's return type, not the factory's.
#[test]
fn parameter_decorator_factory_returning_a_value_decorator_emits_ts1271() {
    let codes = check_experimental(
        r#"
declare function make(): (owner: any, field: string | undefined, position: number) => number;

class Widget {
    render(@make() options: number) {}
}
"#,
    );

    assert!(
        codes.contains(&1271),
        "Expected TS1271 for a factory-produced value-returning parameter decorator, got: {codes:?}"
    );
}

/// tsc reports TS1239 and TS1271 together at the same anchor when the call
/// fails AND the recovered return type is not `void` — the failure does not
/// suppress the return-type check.
#[test]
fn parameter_decorator_with_failing_call_emits_both_ts1239_and_ts1271() {
    let codes = check_experimental(
        r#"
declare function mismatched(scope: any, label: any, slot: string): number;

class Pipeline {
    step(@mismatched stage: number) {}
}
"#,
    );

    assert!(
        codes.contains(&1239) && codes.contains(&1271),
        "Expected TS1239 and TS1271 together for a failing value-returning parameter decorator, got: {codes:?}"
    );
}

/// Negative control for the row above: the same failing call with a `void`
/// return keeps TS1239 alone. This is what proves the new check reads the
/// return type rather than piggybacking on the failure.
#[test]
fn parameter_decorator_with_failing_call_and_void_return_keeps_ts1239_alone() {
    let codes = check_experimental(
        r#"
declare function mismatchedVoid(scope: any, label: any, slot: string): void;

class Conduit {
    step(@mismatchedVoid stage: number) {}
}
"#,
    );

    assert!(
        codes.contains(&1239),
        "Expected TS1239 for a parameter decorator whose call cannot resolve, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1271),
        "A void-returning parameter decorator must not draw TS1271, got: {codes:?}"
    );
}

/// An uncalled zero-argument decorator factory draws only the TS1329
/// "did you mean to call it first" hint; tsc does not judge its return type.
#[test]
fn uncalled_zero_arg_parameter_decorator_factory_does_not_emit_ts1271() {
    let codes = check_experimental(
        r#"
declare function bare(): (holder: any, tag: any, spot: number) => number;

class Uncalled {
    invoke(@bare argument: number) {}
}
"#,
    );

    assert!(
        !codes.contains(&1271),
        "An uncalled zero-arg decorator factory must not draw TS1271, got: {codes:?}"
    );
}

/// `void`, `any`, `undefined` and `never` returns are all accepted: the first
/// two by tsc's own short-circuits, the last two because both are assignable
/// to `void`.
#[test]
fn parameter_decorators_returning_void_any_undefined_or_never_are_accepted() {
    for (label, annotation) in [
        ("void", "void"),
        ("any", "any"),
        ("undefined", "undefined"),
        ("never", "never"),
    ] {
        let source = format!(
            r#"
declare function marker(anchor: any, moniker: string | undefined, seat: number): {annotation};

class Accepting {{
    execute(@marker input: number) {{}}
}}
"#
        );
        let codes = check_experimental(&source);
        assert!(
            !codes.contains(&1271),
            "A parameter decorator returning `{label}` must not draw TS1271, got: {codes:?}"
        );
    }
}

/// The corpus-realistic shape: the decorator's return type is INFERRED from its
/// body rather than annotated. A body that falls off the end infers `void` and
/// stays clean; one that returns a value infers that value's type and is
/// rejected; a bare `return;` guard still infers `void`.
///
/// This is the row that matters for regression risk — an unannotated
/// `function dec(t, k, i) {}` is how nearly every real parameter decorator is
/// written, and it must not start drawing TS1271.
#[test]
fn parameter_decorator_return_type_is_judged_on_the_inferred_body_type() {
    let implicit_void = check_experimental(
        r#"
function plain(target: any, key: any, index: number) { }

class Plain {
    method(@plain arg: number) {}
}
"#,
    );
    assert!(
        !implicit_void.contains(&1271),
        "A parameter decorator whose body infers `void` must stay clean, got: {implicit_void:?}"
    );

    let guarded_void = check_experimental(
        r#"
function guarded(target: any, key: any, index: number) { if (index) { return; } }

class Guarded {
    method(@guarded arg: number) {}
}
"#,
    );
    assert!(
        !guarded_void.contains(&1271),
        "A bare `return;` guard still infers `void`, got: {guarded_void:?}"
    );

    let inferred_value = check_experimental(
        r#"
function producing(target: any, key: any, index: number) { return 1; }

class Producing {
    method(@producing arg: number) {}
}
"#,
    );
    assert!(
        inferred_value.contains(&1271),
        "A parameter decorator whose body infers a value type must draw TS1271, got: {inferred_value:?}"
    );
}

/// A decorator whose type is `any` is unchecked — no signature resolution, so
/// no return type to judge.
#[test]
fn parameter_decorator_typed_any_is_not_judged() {
    let codes = check_experimental(
        r#"
declare const opaque: any;

class Opaque {
    run(@opaque value: number) {}
}
"#,
    );

    assert!(
        !codes.contains(&1271) && !codes.contains(&1239),
        "An `any`-typed parameter decorator must not draw TS1239/TS1271, got: {codes:?}"
    );
}

/// The check is gated on `experimentalDecorators`. Without it the parameter
/// decorator is rejected outright (TS1206) and its return type is never judged.
#[test]
fn parameter_decorator_return_type_is_not_judged_without_experimental_decorators() {
    let codes = check_source_codes(
        r#"
declare function tagged(carrier: any, name: string | undefined, idx: number): number;

class Stage3 {
    apply(@tagged amount: number) {}
}
"#,
    );

    assert!(
        !codes.contains(&1271),
        "TS1271 is an experimentalDecorators-only check, got: {codes:?}"
    );
}

/// The property-decorator sibling this fix mirrors must keep behaving exactly
/// as before — the shared zero-argument-factory predicate was extracted, not
/// re-specified.
#[test]
fn property_decorator_return_type_check_is_unchanged() {
    let value_returning = check_experimental(
        r#"
declare function stamped(container: any, member: string): number;

class Stamped {
    @stamped amount: number = 0;
}
"#,
    );
    assert!(
        value_returning.contains(&1271),
        "Expected TS1271 for a value-returning property decorator, got: {value_returning:?}"
    );

    let void_returning = check_experimental(
        r#"
declare function sealed(container: any, member: string): void;

class Sealed {
    @sealed amount: number = 0;
}
"#,
    );
    assert!(
        !void_returning.contains(&1271),
        "A void-returning property decorator must stay clean, got: {void_returning:?}"
    );
}
