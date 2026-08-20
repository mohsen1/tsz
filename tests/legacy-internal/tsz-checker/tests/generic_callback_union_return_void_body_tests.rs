//! An empty (`void`) callback body inferring a naked type parameter that sits
//! inside a **union** callback-return target.
//!
//! For `m<U>(cb: (v: T) => U | X): U`, a callback with an empty body returns
//! `void`, so `tsc` infers `U = void` from the naked `U` arm of the union. tsz
//! computed the empty body's return type from the contextual return type: an
//! empty body was narrowed to `undefined` whenever `undefined` was assignable to
//! the contextual return — including a `void | X` union that also accepts `void`.
//! Because the contextual return is itself the inference target `U | X`, that
//! narrowing made the inferred body type oscillate between `void` and
//! `undefined` as `U` was fixed and re-substituted across inference rounds, so
//! `U` resolved to `undefined`/`any` instead of `void`. Under
//! `strictNullChecks: false` the target return then dropped its `undefined` arm
//! and the callback's `void` return no longer matched, producing a spurious
//! `TS2345` — the false positive tracked in #16632 (the `Promise.all(...).then`
//! shape of `inferenceLimit.ts`).
//!
//! The fix keeps `void` whenever the contextual return type accepts `void` (or
//! is still an unresolved inference target), and only substitutes `undefined`
//! when the context needs `undefined` specifically (`() => undefined`).
//!
//! Binder names are varied across cases per the anti-hardcoding contract; every
//! case is pinned against `typescript@7.0.2` (`--strict false`).

use crate::test_utils::{check_source_non_strict_codes, check_source_strict_codes};

fn codes(src: &str) -> Vec<u32> {
    check_source_non_strict_codes(src)
}

fn only(src: &str, code: u32) -> Vec<u32> {
    codes(src).into_iter().filter(|&c| c == code).collect()
}

/// The core repro: a method-declared generic callback whose return target is
/// `U | number`, called with an empty-body lambda. No spurious `TS2345`.
#[test]
fn method_union_return_void_body_no_ts2345() {
    assert_eq!(
        only(
            "interface Box<Elem> { run<Res>(cb: (item: Elem) => Res | number): Res; }\n\
             declare const box: Box<unknown[]>;\n\
             box.run((rows) => {});",
            2345,
        ),
        Vec::<u32>::new(),
    );
}

/// The `T | PromiseLike<T>`-shaped union (the `.then` shape) is likewise clean.
#[test]
fn method_thenable_union_return_void_body_no_ts2345() {
    assert_eq!(
        only(
            "interface Chain<Seed> { step<Out>(cb: (seed: Seed) => Out | PromiseLike<Out>): Chain<Out>; }\n\
             declare const chain: Chain<unknown[]>;\n\
             chain.step((orders) => { });",
            2345,
        ),
        Vec::<u32>::new(),
    );
}

/// A `U | U[]` union arm containing the naked parameter structurally is also
/// clean — the empty body must not be widened away from `void`.
#[test]
fn method_array_union_return_void_body_no_ts2345() {
    assert_eq!(
        only(
            "interface Sink<In> { push<Val>(fn: (arg: In) => Val | Val[]): Val; }\n\
             declare const sink: Sink<unknown[]>;\n\
             sink.push((entries) => {});",
            2345,
        ),
        Vec::<u32>::new(),
    );
}

/// Not a method: a function-property callback slot behaves the same (the bug was
/// never about method bivariance — it is the empty-body return type).
#[test]
fn function_property_union_return_void_body_no_ts2345() {
    assert_eq!(
        only(
            "interface Holder<Cell> { apply: <Yield>(cb: (cell: Cell) => Yield | number) => Yield; }\n\
             declare const holder: Holder<unknown[]>;\n\
             holder.apply((cells) => {});",
            2345,
        ),
        Vec::<u32>::new(),
    );
}

/// The inferred result must be `void`, not `any`: consuming it where a `number`
/// is required is a real `TS2322`. Under the bug `U` inferred `any`, which would
/// silence this — so the presence of `TS2322` pins the correct `void` inference.
#[test]
fn empty_body_union_return_infers_void_not_any() {
    assert_eq!(
        only(
            "interface Maker<Src> { make<Made>(cb: (src: Src) => Made | number): Made; }\n\
             declare const maker: Maker<unknown[]>;\n\
             const produced = maker.make((parts) => {});\n\
             const asNumber: number = produced;",
            2322,
        ),
        vec![2322],
    );
    // And consuming the same result where `void` is expected is clean.
    assert_eq!(
        only(
            "interface Maker<Src> { make<Made>(cb: (src: Src) => Made | number): Made; }\n\
             declare const maker: Maker<unknown[]>;\n\
             const produced = maker.make((parts) => {});\n\
             const asVoid: void = produced;",
            2322,
        ),
        Vec::<u32>::new(),
    );
}

/// A concrete-value body still infers the value's type through the union — the
/// naked arm is not swallowed by the structured `PromiseLike` arm.
#[test]
fn concrete_body_union_return_infers_value_type() {
    // `Res` is `number`, so a `string` consumer is a real `TS2322`.
    assert_eq!(
        only(
            "interface Pipe<Feed> { via<Res>(cb: (feed: Feed) => Res | Res[]): Res; }\n\
             declare const pipe: Pipe<unknown[]>;\n\
             const out = pipe.via((feed) => 5);\n\
             const asString: string = out;",
            2322,
        ),
        vec![2322],
    );
}

/// Regression guard for the preserved behavior: an empty body contextually typed
/// by a return that needs `undefined` specifically (not `void`) is still
/// narrowed to `undefined`, so `() => undefined = () => {}` stays clean. This is
/// the `strictNullChecks` case, where `void` is genuinely not assignable to
/// `undefined` and the coercion is the only thing that keeps the assignment
/// clean — exactly the behavior the fix must not disturb.
#[test]
fn empty_body_undefined_only_context_still_coerces() {
    assert_eq!(
        check_source_strict_codes(
            "const cb: () => undefined = () => {};\n\
             const used: undefined = cb();",
        )
        .into_iter()
        .filter(|&c| c == 2322)
        .collect::<Vec<u32>>(),
        Vec::<u32>::new(),
    );
}
