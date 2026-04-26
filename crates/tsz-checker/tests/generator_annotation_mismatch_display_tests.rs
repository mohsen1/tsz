//! Type-display tests for TS2322 on generator functions whose declared
//! return type isn't Generator-like (or is an iterator-like interface whose
//! structure diverges from `Generator<Y, R, N>`).
//!
//! When a generator is annotated with a non-generator type (e.g., `number`),
//! the diagnostic synthesizes a "what the body would produce" type of the
//! form `Generator<TYield, TReturn, TNext>` and compares it against the
//! declared return type. tsc uses `unknown` for the synthesized `TNext` when
//! the declared return type exposes no `TYield` (non-generator-like), matching
//! the `TNext`-from-body-when-no-yield-receivers convention. When the declared
//! return type IS generator-like (and thus provides a `TYield`), tsc uses
//! `any` for `TNext` instead.
//!
//! Baselines these lock in (from the TypeScript compiler baselines):
//!   generatorTypeCheck6.ts:   `Generator<any, any, unknown>` vs `number`
//!   generatorTypeCheck8.ts:   `Generator<string, any, any>` vs `BadGenerator`
//!
//! The generatorTypeCheck8 case also locks in the Iterable-over-Iterator
//! priority when the declared return type extends both heritage families:
//! tsc derives the yield type from `[Symbol.iterator]()`, which is only
//! declared on Iterable-family bases, so `Iterable<string>` wins over
//! `Iterator<number>` regardless of source order in the `extends` list.

const GENERATOR_STUBS: &str = r#"
interface IteratorYieldResult<TYield> { done?: false; value: TYield; }
interface IteratorReturnResult<TReturn> { done: true; value: TReturn; }
type IteratorResult<T, TReturn = any> = IteratorYieldResult<T> | IteratorReturnResult<TReturn>;
interface Iterator<T, TReturn = any, TNext = undefined> {
    next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    return?(value?: TReturn): IteratorResult<T, TReturn>;
    throw?(e?: any): IteratorResult<T, TReturn>;
}
interface Iterable<T, TReturn = unknown, TNext = unknown> {
    [Symbol.iterator](): Iterator<T, TReturn, TNext>;
}
interface IterableIterator<T, TReturn = any, TNext = undefined> extends Iterator<T, TReturn, TNext> {
    [Symbol.iterator](): IterableIterator<T, TReturn, TNext>;
}
interface Generator<T = unknown, TReturn = any, TNext = any> extends IterableIterator<T, TReturn, TNext> {
    next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    return(value: TReturn): IteratorResult<T, TReturn>;
    throw(e: any): IteratorResult<T, TReturn>;
    [Symbol.iterator](): Generator<T, TReturn, TNext>;
}
"#;

fn get_diagnostics(user_source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(&format!(
        "{GENERATOR_STUBS}\n{user_source}"
    ))
}

#[test]
fn empty_generator_annotated_with_number_reports_tnext_unknown() {
    // `function* g1(): number { }` — no yields, no returns, no generator-shaped
    // context. Synthesized body type should be `Generator<any, any, unknown>`,
    // not `Generator<any, any, any>`.
    let diags = get_diagnostics("function* g1(): number { }");
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got {diags:#?}"
    );
    let (_, msg) = ts2322[0];
    assert!(
        msg.contains("Generator<any, any, unknown>"),
        "expected body type to display as Generator<any, any, unknown>, got: {msg}"
    );
    assert!(
        msg.contains("'number'"),
        "expected mismatch against 'number', got: {msg}"
    );
}

#[test]
fn empty_generator_annotated_with_generator_like_keeps_tnext_any() {
    // `function* g(): BadIter { }` where BadIter extends a single iterator-like
    // interface — the declared return type exposes a TYield, so the synthesized
    // body type keeps TNext = `any` (matching tsc's `Generator<T, any, any>`
    // pattern). This locks in that the TNext fallback-to-`unknown` is gated on
    // the absence of a yield type, not applied blanketly.
    let source = r#"
interface BadIter extends Iterator<number> {
    extra: string;
}
function* g(): BadIter { }
"#;
    let diags = get_diagnostics(source);
    // The mismatch can surface as TS2322 (not assignable) or TS2741 (property
    // missing) depending on which elaboration path wins. Both render the
    // synthesized body type.
    let relevant: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| (*code == 2322 || *code == 2741) && msg.contains("Generator<"))
        .collect();
    assert!(
        !relevant.is_empty(),
        "expected a TS2322/TS2741 rendering the synthesized Generator type, got {diags:#?}"
    );
    let msg = relevant[0].1.as_str();
    // TNext must be `any` (not `unknown`) when a TYield was extractable from
    // the declared annotation. TYield may print as `number` (from Iterator<number>)
    // or reflect extraction fallbacks — the invariant under test is the TNext form.
    assert!(
        msg.contains(", any, any>"),
        "expected TNext=any in synthesized Generator type, got: {msg}"
    );
    assert!(
        !msg.contains(", any, unknown>"),
        "TNext must remain `any` when declared annotation exposes a TYield, got: {msg}"
    );
}

#[test]
fn yield_type_prefers_iterable_over_iterator_in_mixed_heritage() {
    // Exact shape of the conformance test `generatorTypeCheck8.ts`.
    // `BadGenerator` extends both `Iterator<number>` AND `Iterable<string>`.
    // tsc derives the yield type from `[Symbol.iterator]()`, which only
    // exists on `Iterable<T>`, so the synthesized body must render as
    // `Generator<string, any, any>` — NOT `Generator<number, any, any>`.
    let source = r#"
interface BadGenerator extends Iterator<number>, Iterable<string> { }
function* g3(): BadGenerator { }
"#;
    let diags = get_diagnostics(source);
    let relevant: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 2322 && msg.contains("Generator<"))
        .collect();
    assert!(
        !relevant.is_empty(),
        "expected a TS2322 rendering the synthesized Generator type, got {diags:#?}"
    );
    let msg = relevant[0].1.as_str();
    assert!(
        msg.contains("Generator<string,"),
        "yield type must come from Iterable<string> (via [Symbol.iterator]), got: {msg}"
    );
    assert!(
        !msg.contains("Generator<number,"),
        "yield type must not come from Iterator<number> when Iterable<string> is also extended, got: {msg}"
    );
}

#[test]
fn yield_type_prefers_iterable_even_when_iterator_listed_first() {
    // Same as above but with an extra middle heritage to ensure ordering
    // truly doesn't influence the outcome; the only thing that matters is
    // which heritage family exposes `[Symbol.iterator]()`.
    let source = r#"
interface Mixed extends Iterator<number>, IterableIterator<boolean> { }
function* g(): Mixed { }
"#;
    let diags = get_diagnostics(source);
    let relevant: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| *code == 2322 && msg.contains("Generator<"))
        .collect();
    assert!(
        !relevant.is_empty(),
        "expected a TS2322 rendering the synthesized Generator type, got {diags:#?}"
    );
    let msg = relevant[0].1.as_str();
    assert!(
        msg.contains("Generator<boolean,"),
        "yield type must come from IterableIterator<boolean>, got: {msg}"
    );
    assert!(
        !msg.contains("Generator<number,"),
        "yield type must not come from Iterator<number> when an Iterable-family is also extended, got: {msg}"
    );
}
