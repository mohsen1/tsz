//! Regression tests for #16030: `yield*` contributes nothing to an
//! unannotated generator's inferred yield type when the delegate resolves
//! through `[Symbol.iterator]` property access rather than the solver's
//! array/tuple fast path.
//!
//! `get_iterator_info` (`tsz-solver`) is a pure structural query: its
//! `[Symbol.iterator]` lookup cannot evaluate through the `TypeData::Lazy(DefId)`
//! alias body every non-array/tuple lib iterable (`Set<T>`, `Iterable<T>`,
//! `Generator<T>`, a delegate through another generator declaration) exposes
//! that member behind. It resolves the property to `ANY`, which the
//! `ThisType`-substitution fallback then reinterprets as "the delegate IS the
//! iterator", so `next()` is never found and the whole query returns `None` —
//! silently collapsing the aggregated yield type to `any` and accepting a
//! mismatched `Generator` instantiation. `for..of` already solves this exact
//! gap via the checker's env-aware property-access chain
//! (`resolve_iterator_element_type`); the fix reuses that same query as a
//! fallback for `yield*`, but on the **sync path only**.
//!
//! The async path (`for await` / `yield*` inside an `async function*`) has
//! the identical solver-only blind spot — `get_async_iterable_element_type`
//! is just `get_iterator_info` retried sync-then-async — but widening it the
//! same way regressed `asyncYieldStarContextualType.ts`: an uninstantiated
//! generic delegate (`yield* g()` for a bare `<T>() => AsyncGenerator<T>`)
//! resolved to its structural `T` (defaulting to `unknown`) instead of the
//! contextual yield type the containing annotated generator provides, which
//! `tsc` threads into the delegate call and this fallback does not. Left
//! solver-only pending a fix that also threads that contextual typing in;
//! see the module doc on the async branch in `dispatch/yield_.rs`.
//!
//! Every test uses a generator *expression* (`const d = function* () {...}`),
//! not a declaration: unannotated generator *declarations* have a separate,
//! pre-existing, and much wider bail in `infer_generator_declaration_yield_type`
//! that collapses the inferred signature to `any` for *any* `yield*` in the
//! body regardless of delegate shape (tracked and narrowed separately by
//! #16029, not yet merged) — using a declaration here would test that bail,
//! not this fix. The generator-expression path has never had that bail (see
//! `generator_expression_set_delegate_contributes_element_type` below, which
//! exists specifically to prove this fix is not the declaration pre-pass).
//!
//! Each delegate is passed to a parameter typed as a deliberately wrong
//! `Generator` instantiation, so a missing `TS2345` means
//! the delegate's contribution collapsed to `any` instead of its real
//! element type.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn set_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<number>): void;
const d = function* (src: Set<string>) {
    yield* src;
};
wants(d(new Set<string>()));
"#,
    );
    assert!(
        codes.contains(&2345),
        "Set<string> delegate must contribute `string`, not widen to `any`: {codes:?}"
    );
}

#[test]
fn iterable_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<number>): void;
const d = function* (src: Iterable<string>) {
    yield* src;
};
wants(d(["a"]));
"#,
    );
    assert!(
        codes.contains(&2345),
        "Iterable<string> delegate must contribute `string`, not widen to `any`: {codes:?}"
    );
}

#[test]
fn generator_binding_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<number>): void;
declare const src: Generator<string>;
const d = function* () {
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "annotated Generator<string> binding delegate must contribute `string`: {codes:?}"
    );
}

#[test]
fn generator_declaration_call_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<number>): void;
function* src(): Generator<string> {
    yield "a";
}
const d = function* () {
    yield* src();
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "delegate to a call of another generator declaration must contribute its yield type: {codes:?}"
    );
}

#[test]
fn string_literal_delegate_contributes_string() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<number>): void;
const d = function* () {
    yield* "ab";
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a string delegate must contribute `string`, not widen to `any`: {codes:?}"
    );
}

#[test]
fn generator_expression_set_delegate_contributes_element_type() {
    // The declaration-signature bail (#16029, unmerged) does not exist for
    // generator expressions, so this is the shape that directly proves the
    // fix and is not confounded by that separate, wider bail.
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<number>): void;
const d = function* (src: Set<string>) {
    yield* src;
};
wants(d(new Set<string>()));
"#,
    );
    assert!(
        codes.contains(&2345),
        "the same gap on the generator *expression* path (no bail ever existed there): {codes:?}"
    );
}

#[test]
fn array_delegate_still_works_unaffected() {
    // Adjacent control: the pre-existing array/tuple fast path must be untouched.
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<string>): void;
const d = function* () {
    yield* [1, 2];
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "array delegate (fast path) must keep contributing `number`: {codes:?}"
    );
}

#[test]
fn renamed_binder_set_delegate_is_structural() {
    // Anti-hardcoding control: same shape, every identifier renamed.
    let codes = strict_codes(
        r#"
export {};
declare function expects(gen: Generator<string>): void;
const generatorFn = function* (itemSource: Set<boolean>) {
    yield* itemSource;
};
expects(generatorFn(new Set<boolean>()));
"#,
    );
    assert!(
        codes.contains(&2345),
        "the fix must be structural, not name-keyed: {codes:?}"
    );
}

#[test]
fn correctly_typed_set_delegate_stays_clean() {
    // Negative control: a correctly typed instantiation must not regress to
    // a spurious diagnostic once the Set delegate is no longer `any`.
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: Generator<string>): void;
const d = function* (src: Set<string>) {
    yield* src;
};
wants(d(new Set<string>()));
"#,
    );
    assert!(
        !codes.contains(&2345) && !codes.contains(&2322),
        "a correctly typed Set<string> delegate must stay clean: {codes:?}"
    );
}

#[test]
fn evolving_delegate_bail_is_unaffected() {
    // Adjacent control: the array/tuple fast path (which an evolving `var`
    // delegate hits) must still win over the new checker-level fallback, so
    // #16029's declaration-signature hazard gate (which relies on that fast
    // path never resolving through property access) is untouched by this fix.
    let codes = strict_codes(
        r#"
export {};
function* d() {
    var o: any = [];
    while (true) {
        o = yield* o;
    }
}
"#,
    );
    assert!(
        !codes.contains(&2345),
        "the evolving-binding circular shape must not spuriously report TS2345: {codes:?}"
    );
}
