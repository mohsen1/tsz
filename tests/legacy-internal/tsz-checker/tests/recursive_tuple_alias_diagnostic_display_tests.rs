//! Tests for recursive generic type-alias display in assignability diagnostics.
//!
//! Structural rule: when a type that appears in a TS2322/TS2345 message is a
//! *recursive* generic type-alias application — a generic alias whose body
//! transitively references the alias itself, e.g.
//! `type List<T> = [number, List<{ x: T }>]` — `tsc` renders it by its alias
//! name (`List<U>`), never by structural expansion. Structural expansion of a
//! self-referential body is unbounded (`[number, [number, [..., ...]]]`), so it
//! is both wrong and unbounded.
//!
//! Before the fix, only the assignment *target* preserved the alias name (via
//! the as-written annotation). The assignment *source*, call-argument,
//! call-parameter, and return-type roles evaluated the application into its body
//! and printed the runaway expansion. The shared diagnostic-role guard now
//! applies the alias-name rule to every assignability role, so both operands of
//! the message keep their alias spelling regardless of position.
//!
//! Binder names are varied across cases so the rule is proven structurally, not
//! against a specific alias spelling.

use crate::test_utils::check_source_diagnostics;

fn assignability_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| d.code == 2322 || d.code == 2345)
        .map(|d| d.message_text)
        .collect()
}

/// The original #48524/#52722/#49226 repro shape: both operands are recursive
/// generic tuple aliases. Neither may expand structurally.
#[test]
fn recursive_tuple_alias_assignment_keeps_alias_on_both_operands() {
    let messages = assignability_messages(
        r#"
type T1<T> = [number, T1<{ x: T }>];
type T2<T> = [42, T2<{ x: T }>];
function qq<U>(x: T1<U>, y: T2<U>) {
    y = x;
}
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m == "Type 'T1<U>' is not assignable to type 'T2<U>'."),
        "both recursive aliases must keep their names, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("..., ...")),
        "no runaway structural expansion may leak, got: {messages:?}"
    );
}

/// Renamed binders prove the rule is structural, not keyed off `T1`/`T2`.
#[test]
fn recursive_tuple_alias_assignment_renamed_binders() {
    let messages = assignability_messages(
        r#"
type Alpha<Q> = [number, Alpha<{ x: Q }>];
type Beta<Q> = [42, Beta<{ x: Q }>];
function check<Elem>(left: Alpha<Elem>, right: Beta<Elem>) {
    right = left;
}
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m == "Type 'Alpha<Elem>' is not assignable to type 'Beta<Elem>'."),
        "renamed recursive aliases must keep their names, got: {messages:?}"
    );
}

/// Concrete type arguments still render by alias name on both sides.
#[test]
fn recursive_tuple_alias_assignment_concrete_args() {
    let messages = assignability_messages(
        r#"
type T1<T> = [number, T1<{ x: T }>];
type T2<T> = [42, T2<{ x: T }>];
declare let a: T1<number>;
declare let b: T2<number>;
b = a;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("Type 'T1<number>' is not assignable to type 'T2<number>'.")),
        "concrete-arg recursive aliases must keep their names, got: {messages:?}"
    );
}

/// Call-argument position (TS2345): both the argument (source) and the
/// parameter (target) keep their alias names.
#[test]
fn recursive_tuple_alias_call_argument_keeps_alias() {
    let messages = assignability_messages(
        r#"
type T1<T> = [number, T1<{ x: T }>];
type T2<T> = [42, T2<{ x: T }>];
declare function consume(p: T2<number>): void;
declare const arg: T1<number>;
consume(arg);
"#,
    );
    assert!(
        messages.iter().any(|m| m.starts_with(
            "Argument of type 'T1<number>' is not assignable to parameter of type 'T2<number>'."
        )),
        "call argument/parameter recursive aliases must keep their names, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("..., ...")),
        "no runaway structural expansion may leak in call diagnostics, got: {messages:?}"
    );
}

/// Return-type position (TS2322): the return-statement value (source) and the
/// declared return type (target) keep their alias names.
#[test]
fn recursive_tuple_alias_return_type_keeps_alias() {
    let messages = assignability_messages(
        r#"
type T1<T> = [number, T1<{ x: T }>];
type T2<T> = [42, T2<{ x: T }>];
declare const value: T1<number>;
function produce(): T2<number> {
    return value;
}
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("Type 'T1<number>' is not assignable to type 'T2<number>'.")),
        "return-type recursive aliases must keep their names, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("..., ...")),
        "no runaway structural expansion may leak in return diagnostics, got: {messages:?}"
    );
}

/// Recursive *object* aliases already kept their names (objects carry a symbol
/// stamp); this locks in that the guard does not disturb them.
#[test]
fn recursive_object_alias_assignment_keeps_alias() {
    let messages = assignability_messages(
        r#"
type Node1<T> = { value: T; next: Node1<T> };
declare let s: Node1<string>;
declare let n: Node1<number>;
n = s;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m
                .starts_with("Type 'Node1<string>' is not assignable to type 'Node1<number>'.")),
        "recursive object aliases must keep their names, got: {messages:?}"
    );
}

/// Recursive mapped-object aliases still need the role-specific formatter: tsc
/// reduces through mapped property substitutions here and reports the concrete
/// property value type (`string`), not the recursive alias application.
#[test]
fn recursive_mapped_alias_property_value_still_reduces() {
    let messages = assignability_messages(
        r#"
type Envelope<Subject, Extra> = {
    readonly[Key in keyof Subject]: {
        value: Subject[Key];
        also: Extra;
        readonly children: Envelope<Subject[Key], Extra>;
    };
}

interface Payload {
    name: string;
    count: number;
}

declare const output: Envelope<Payload, boolean>;
const shouldFail: { important: boolean } = output.name.children;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m == "Type 'string' is not assignable to type '{ important: boolean; }'."),
        "recursive mapped property value should reduce to the concrete source, got: {messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("Envelope<string")),
        "mapped/object recursion must not be preserved by the tuple guard, got: {messages:?}"
    );
}

/// Fallback boundary: a *non-recursive* tuple alias is NOT affected by the
/// recursive guard. `tsc` expands a non-recursive tuple alias structurally in
/// the assignment-target position, so the guard must leave that behavior intact.
#[test]
fn non_recursive_tuple_alias_target_still_expands() {
    let messages = assignability_messages(
        r#"
type Pair<T> = [T, T];
declare let a: Pair<string>;
declare let b: Pair<number>;
b = a;
"#,
    );
    assert!(
        messages
            .iter()
            .any(|m| m
                .starts_with("Type 'Pair<string>' is not assignable to type '[number, number]'.")),
        "non-recursive tuple alias target must still expand structurally, got: {messages:?}"
    );
}
