//! Regression tests for `await` of a *structural* thenable whose `then` member
//! is stored as a bare `Function` shape rather than a `Callable` shape.
//!
//! tsc's `getAwaitedType` inspects a thenable's `then`/`onfulfilled` callback
//! structurally, regardless of how the thenable (and its `then` member) was
//! declared. tsz unwraps a `then` stored as a `Callable` (the representation
//! used for named `interface`/`type` *method* members) but previously returned
//! no call signatures for a `then` stored as a `Function` — the representation
//! used for object-literal methods and for plain function-typed properties
//! (`then: (cb) => void`). The awaited value then stayed un-unwrapped, producing
//! a spurious `TS2322` on the inferred async-function return type or on a value
//! assignment.
//!
//! The fix makes the thenable's `then`-signature acquisition accept both the
//! `Function` and `Callable` forms (a non-constructor `Function` contributes
//! exactly one call signature), so structural thenables unwrap identically no
//! matter how `then` was written. These tests vary the binder/alias names so the
//! behavior stays name-agnostic.

use crate::test_utils::check_source_codes;

/// Inline object-literal thenable (`then` is an object-literal method → a
/// `Function` shape): `await { then(cb) {} }` must unwrap to the callback's
/// value type (`number`).
#[test]
fn await_unwraps_inline_object_literal_thenable() {
    let source = r#"
async function f() {
    const y = await { then(cb: (x: number) => void) {} };
    const n: number = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "awaiting an inline object-literal thenable must unwrap to its `onfulfilled` value type; got {codes:?}"
    );
}

/// The inferred async return type of a function that returns an inline
/// object-literal thenable must be the *unwrapped* `Promise<number>`, not
/// `Promise<{ then(...) }>`.
#[test]
fn async_return_of_object_literal_thenable_unwraps() {
    let source = r#"
async function a() {
    const y = await { then(cb: (x: number) => void) {} };
    return y;
}
const p: Promise<number> = a();
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "inferred async return of an awaited object-literal thenable must unwrap to Promise<number>; got {codes:?}"
    );
}

/// `type` alias to an object type with a method-style `then` (`then(cb): void`).
#[test]
fn await_unwraps_type_alias_method_style_thenable() {
    let source = r#"
type T = { then(cb: (x: number) => void): void };
declare const t: T;
async function f() {
    const y = await t;
    const n: number = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "awaiting a method-style alias thenable must unwrap to its value type; got {codes:?}"
    );
}

/// `type` alias with a property-style `then` (`then: (cb) => void`). The `then`
/// member is a function-typed property — also a `Function` shape.
#[test]
fn await_unwraps_type_alias_property_style_thenable() {
    let source = r#"
type Eventual = { then: (cb: (value: string) => void) => void };
declare const e: Eventual;
async function consume() {
    const y = await e;
    const s: string = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "awaiting a property-style alias thenable must unwrap to its value type; got {codes:?}"
    );
}

/// A union with a structural-thenable member and a plain member:
/// `await ({ then(...) } | string)` must yield `number | string`.
#[test]
fn await_unwraps_union_with_structural_thenable_member() {
    let source = r#"
declare const u: { then(cb: (x: number) => void): void } | string;
async function f() {
    const y = await u;
    const z: number | string = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "awaiting a union with a structural-thenable member must unwrap that member; got {codes:?}"
    );
}

/// `Promise<Th>` where `Th` is an alias-thenable: unwrapping the lib `Promise`
/// reveals `Th`, which must itself be re-unwrapped to its value type.
#[test]
fn await_unwraps_promise_of_alias_thenable() {
    let source = r#"
type Th = { then(cb: (x: number) => void): void };
declare const p: Promise<Th>;
async function f() {
    const y = await p;
    const n: number = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "awaiting Promise<alias-thenable> must unwrap both layers to the value type; got {codes:?}"
    );
}

/// Control: the structurally-identical named `interface` form (a `Callable`
/// `then`) must keep unwrapping — guard against any divergence between the two
/// representations.
#[test]
fn await_unwraps_named_interface_thenable_control() {
    let source = r#"
interface I { then(cb: (x: number) => void): void }
declare const i: I;
async function f() {
    const y = await i;
    const n: number = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "named-interface thenable control must keep unwrapping; got {codes:?}"
    );
}

/// Negative control: a structural thenable awaits to `number`, which is not a
/// `string`, so the genuine mismatch must still surface a `TS2322`.
#[test]
fn await_structural_thenable_still_reports_genuine_mismatch() {
    let source = r#"
declare const t: { then(cb: (x: number) => void): void };
async function f() {
    const y = await t;
    const wrong: string = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "awaiting the thenable yields `number`, not assignable to `string`; expected TS2322, got {codes:?}"
    );
}

/// Negative control: a *plain* object (no `then` member) must NOT be unwrapped —
/// `await { x: number }` stays `{ x: number }`, so assigning to `number` errors.
/// Guards against the fix over-eagerly treating any object as a thenable.
#[test]
fn await_plain_object_is_not_unwrapped() {
    let source = r#"
declare const plain: { x: number };
async function f() {
    const y = await plain;
    const wrong: number = y;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "a plain object has no `then` and must not unwrap; expected TS2322, got {codes:?}"
    );
}
