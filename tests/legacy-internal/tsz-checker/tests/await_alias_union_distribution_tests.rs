//! Regression tests for `await` of a deferred generic-alias / conditional type
//! that resolves to a union containing a `Promise` member.
//!
//! `tsc`'s `getAwaitedType` operates on the *resolved* type, so for
//! `type Awaitable<T> = T | Promise<T>` an `await x` with `x: Awaitable<number>`
//! distributes `Awaited` over the union members and unwraps the `Promise<T>`
//! branch, yielding `number`. tsz previously left the operand as a deferred
//! `Application` node (not a `Union` node), so the union-distribution step in
//! `compute_awaited_type` never fired and the awaited type stayed
//! `number | Promise<number>` — a spurious `TS2322` on the assignment.
//!
//! The fix evaluates a still-deferred residual operand to its structural form
//! and only retries awaiting when that exposes a union, intersection, or
//! thenable layer. These tests vary the binder and alias names to keep that
//! name-agnostic.

use crate::test_utils::check_source_codes;

/// Direct `Awaitable<number>` annotation (deferred alias application): `await x`
/// must unwrap to `number`.
#[test]
fn await_unwraps_promise_through_generic_alias_union() {
    let source = r#"
type Awaitable<T> = T | Promise<T>;
async function f(x: Awaitable<number>) {
    const n: number = await x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "await of a generic-alias union (T | Promise<T>) must unwrap the Promise member to its value type; got {codes:?}"
    );
}

/// Same shape with different binder/alias spellings — the fix must not depend on
/// the names `Awaitable`/`T`.
#[test]
fn await_unwraps_promise_through_generic_alias_union_renamed_binders() {
    let source = r#"
type Eventually<Value> = Value | Promise<Value>;
async function consume(input: Eventually<string>) {
    const s: string = await input;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "renamed alias/binder must behave identically; got {codes:?}"
    );
}

/// Union written `Promise<T> | T` (Promise branch first) must unwrap the same
/// way — order independence.
#[test]
fn await_unwraps_promise_through_generic_alias_union_promise_first() {
    let source = r#"
type Aw<T> = Promise<T> | T;
async function f(x: Aw<number>) {
    const n: number = await x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Promise-first alias union must unwrap identically; got {codes:?}"
    );
}

/// The alias' type argument is itself a union: `await Awaitable<number | string>`
/// must yield `number | string`.
#[test]
fn await_unwraps_promise_through_generic_alias_union_with_union_argument() {
    let source = r#"
type Awaitable<T> = T | Promise<T>;
async function f(x: Awaitable<number | string>) {
    const n: number | string = await x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "alias union over a union argument must unwrap each branch; got {codes:?}"
    );
}

/// Awaiting `Promise.all` over an async mapper must preserve the awaited array
/// application as the value type. The deferred-await fallback must not eagerly
/// expand ordinary generic applications like arrays into structural object
/// shapes after the Promise layer has already been unwrapped.
#[test]
fn await_promise_all_async_mapper_preserves_array_value_shape() {
    let source = r#"
interface Item {
    ready: boolean;
}
type Metadata = {
    updated: boolean;
};
declare function scan(item: Item): Promise<Metadata | undefined>;

async function collect(items: Item[]): Promise<void> {
    const pairs: [Item, Metadata | undefined][] = await Promise.all(
        items
            .filter((item) => item.ready)
            .map(async (item) => [item, await scan(item)])
    );
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "`Promise.all` over an async mapper should await to the array value type without structural over-expansion; got {codes:?}"
    );
}

/// A conditional type that resolves to `T | Promise<T>` is also a deferred form
/// that must be evaluated before distribution.
#[test]
fn await_unwraps_promise_through_conditional_resolving_to_union() {
    let source = r#"
type MaybePromise<T> = T extends unknown ? T | Promise<T> : never;
async function f(x: MaybePromise<number>) {
    const n: number = await x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "conditional resolving to a Promise union must unwrap; got {codes:?}"
    );
}

/// Negative control: the awaited value type is genuinely wrong, so a `TS2322`
/// must still fire — the awaited type is `number`, not `string`.
#[test]
fn await_alias_union_still_reports_genuine_mismatch() {
    let source = r#"
type Awaitable<T> = T | Promise<T>;
async function f(x: Awaitable<number>) {
    const wrong: string = await x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "awaiting `Awaitable<number>` yields `number`, which is not assignable to `string`; expected TS2322, got {codes:?}"
    );
}

/// Regression guard: the already-correct direct union form
/// (`number | Promise<number>`) must keep unwrapping.
#[test]
fn await_unwraps_direct_union_promise_member() {
    let source = r#"
async function f(x: number | Promise<number>) {
    const n: number = await x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "direct union with a Promise member must unwrap; got {codes:?}"
    );
}

/// Regression guard: a generic alias whose body is a bare `Promise<T>` (no
/// union) must keep unwrapping through the Promise path.
#[test]
fn await_unwraps_generic_alias_to_bare_promise() {
    let source = r#"
type Eventual<T> = Promise<T>;
async function f(x: Eventual<number>) {
    const n: number = await x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "generic alias to a bare Promise must unwrap; got {codes:?}"
    );
}
