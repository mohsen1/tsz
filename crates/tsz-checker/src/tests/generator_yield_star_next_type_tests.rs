//! Regression tests for the `TNext` slot of an unannotated generator whose
//! body delegates with `yield*` (#15632, `TNext` half).
//!
//! Structural rule: when an unannotated generator's body contains `yield* e`,
//! `tsc` contributes `e`'s own `TNext` — the type its `next()` accepts — to the
//! enclosing generator's `TNext`, intersecting one contribution per delegation
//! (`checkAndAggregateYieldOperandTypes` collects
//! `getIterationTypeOfIterable(IterationTypeKind.Next, ...)`, then
//! `getIntersectionType`). tsz never collected it: the slot was hardcoded to
//! `unknown` unless a *contextual* `Generator<Y, R, N>` supplied one, so every
//! delegating generator's `.next()` accepted any argument at all.
//!
//! `unknown` is the correct answer only when nothing contributes, which is why
//! the delegate-has-no-`TNext` rows below (array, tuple, `string`) are load-
//! bearing negative controls rather than filler: the fix must stay silent on
//! them. That is also why the contribution reads the delegate's *declared*
//! `TNext` type argument instead of `get_iterator_info(...).next_type` — the
//! latter reports `undefined` for the `Array`/`Tuple` fast paths, which would
//! replace a correct `unknown` with a wrong `undefined`.
//!
//! The oracle is `.next(arg)` argument checking, not rendered type text: with
//! `TNext = unknown` every argument is accepted, so a missing contribution is
//! observable as a *missing* TS2345. Every expectation below was pinned against
//! `tsc` 6.0.2 (`--strict --target es2022 --lib es2022`); the `[] | [string]`
//! parameter shape in those diagnostics is `Generator.next`'s own overload
//! tuple, which is what makes the slot observable at a call site.

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

/// Delegate sources. Binder names are varied per test (see
/// `renamed_binders_*`) so no identifier here is load-bearing.
const PREAMBLE: &str = r#"
export {};
declare function srcStr(): Generator<number, void, string>;
declare function srcNum(): Generator<number, void, number>;
declare function srcStrOrUndef(): Generator<number, void, string | undefined>;
declare function asrcStr(): AsyncGenerator<number, void, string>;
"#;

const TS2345: u32 = 2345;

fn check(body: &str) -> Vec<u32> {
    strict_codes(&format!("{PREAMBLE}{body}"))
}

// ── Core witness: a plain declaration delegating to a generator ──

#[test]
fn delegated_next_type_rejects_incompatible_next_argument() {
    let codes = check(
        r#"
function* relay() { yield* srcStr(); }
relay().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "`yield* srcStr()` must give `relay` the delegate's `TNext` (`string`), \
         so `.next(42)` is TS2345. Before the fix `TNext` stayed `unknown` and \
         this call was silently accepted."
    );
}

#[test]
fn delegated_next_type_accepts_compatible_next_argument() {
    let codes = check(
        r#"
function* relay() { yield* srcStr(); }
relay().next("ok");
"#,
    );
    assert!(
        codes.is_empty(),
        "a `string` argument satisfies the delegated `TNext`; got {codes:?}"
    );
}

// ── Negative controls: shapes that must keep the `unknown` default ──

#[test]
fn plain_yield_only_generator_keeps_unknown_next_type() {
    let codes = check(
        r#"
function* plain() { yield 1; }
plain().next(42);
"#,
    );
    assert!(
        codes.is_empty(),
        "no delegation contributes nothing, so `TNext` stays `unknown` and \
         every argument is accepted; got {codes:?}"
    );
}

#[test]
fn array_delegate_keeps_unknown_next_type() {
    let codes = check(
        r#"
function* fromArray() { yield* [1, 2]; }
fromArray().next(42);
"#,
    );
    assert!(
        codes.is_empty(),
        "an array delegate declares no `TNext` of its own, so the slot stays \
         `unknown` — matching tsc. Reading the structural iterator info here \
         would wrongly pin it to `undefined`; got {codes:?}"
    );
}

#[test]
fn tuple_delegate_keeps_unknown_next_type() {
    let codes = check(
        r#"
function* fromTuple() { yield* [1, "a"] as [number, string]; }
fromTuple().next(42);
"#,
    );
    assert!(
        codes.is_empty(),
        "tuple delegate: same negative control as the array row; got {codes:?}"
    );
}

#[test]
fn string_delegate_keeps_unknown_next_type() {
    let codes = check(
        r#"
function* fromString() { yield* "ab"; }
fromString().next(42);
"#,
    );
    assert!(
        codes.is_empty(),
        "`string`'s iterator declares no `TNext`; got {codes:?}"
    );
}

// ── Aggregation across several delegations: intersection, not union ──

#[test]
fn two_delegates_intersect_their_next_types() {
    let codes = check(
        r#"
function* two() { yield* srcStr(); yield* srcStrOrUndef(); }
two().next("ok");
"#,
    );
    assert!(
        codes.is_empty(),
        "`string & (string | undefined)` still admits a string; got {codes:?}"
    );
}

#[test]
fn two_delegates_reject_argument_only_one_admits() {
    let codes = check(
        r#"
function* two() { yield* srcStr(); yield* srcStrOrUndef(); }
two().next(undefined);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "intersection, not union: a value sent into `two` can reach either \
         delegate, so it must satisfy both. `undefined` satisfies only the \
         second."
    );
}

#[test]
fn disjoint_delegate_next_types_intersect_to_never() {
    let codes = check(
        r#"
function* conflict() { yield* srcStr(); yield* srcNum(); }
conflict().next("ok");
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "`string & number` is `never`: no argument can be forwarded to both \
         delegates, which is exactly what tsc reports here."
    );
}

// ── Mixed plain + delegated bodies ──

#[test]
fn plain_yield_does_not_erase_the_delegated_next_type() {
    let codes = check(
        r#"
function* mixed() { yield 1; yield* srcStr(); }
mixed().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "a plain `yield` alongside the delegation contributes nothing to \
         `TNext` and must not wash the delegated contribution out"
    );
}

#[test]
fn delegation_after_plain_yield_order_independent() {
    let codes = check(
        r#"
function* mixed() { yield* srcStr(); yield 1; }
mixed().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "source order of the plain and delegated yields must not matter"
    );
}

// ── Adjacent forms: expression, async, method, custom iterable ──

#[test]
fn generator_function_expression_gets_the_delegated_next_type() {
    let codes = check(
        r#"
const relayExpr = function* () { yield* srcStr(); };
relayExpr().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "the expression path builds its signature through the same body-check \
         aggregation as a declaration"
    );
}

#[test]
fn async_generator_gets_the_delegated_next_type() {
    let codes = check(
        r#"
async function* arelay() { yield* asrcStr(); }
arelay().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "the async arm collects the delegated `TNext` too, into `AsyncGenerator`"
    );
}

#[test]
fn generator_method_gets_the_delegated_next_type() {
    let codes = check(
        r#"
const host = { *relay() { yield* srcStr(); } };
host.relay().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "a generator *method* shares the same aggregation path"
    );
}

#[test]
fn custom_iterable_delegate_contributes_its_declared_next_type() {
    let codes = check(
        r#"
interface Custom { [Symbol.iterator](): Iterator<number, void, string>; }
declare const custom: Custom;
function* fromCustom() { yield* custom; }
fromCustom().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "not just lib `Generator`: a user iterable whose iterator declares \
         `TNext` contributes it as well"
    );
}

#[test]
fn nested_delegation_through_a_sibling_generator_propagates_next_type() {
    let codes = check(
        r#"
function* inner() { yield* srcStr(); }
function* outer() { yield* inner(); }
outer().next(42);
"#,
    );
    assert_eq!(
        codes,
        vec![TS2345],
        "`inner`'s own inferred `TNext` must be visible to `outer`'s \
         delegation — the fix has to compose with itself"
    );
}

// ── Binder-name independence ──

#[test]
fn renamed_binders_behave_identically() {
    let codes = strict_codes(
        r#"
export {};
declare function zzz(): Generator<number, void, string>;
function* qqq() { yield* zzz(); }
qqq().next(42);
"#,
    );
    assert_eq!(codes, vec![TS2345], "no identifier drives this decision");
}

#[test]
fn renamed_binders_negative_case_behaves_identically() {
    let codes = strict_codes(
        r#"
export {};
function* qqq() { yield* [1, 2]; }
qqq().next(42);
"#,
    );
    assert!(
        codes.is_empty(),
        "renamed negative control stays silent; got {codes:?}"
    );
}

// ── An explicit annotation still wins ──

#[test]
fn explicit_return_annotation_overrides_the_delegated_next_type() {
    let codes = check(
        r#"
function* annotated(): Generator<number, void, unknown> { yield* srcStr(); }
annotated().next(42);
"#,
    );
    // What this fix owns: an *annotated* generator's `TNext` comes from its
    // annotation, never from the delegation, so the delegated `string` must not
    // leak into the signature and make `.next(42)` an error.
    assert!(
        !codes.contains(&TS2345),
        "the declared `TNext` is `unknown`, so `.next(42)` is accepted; the \
         delegated contribution is only ever consulted for an *unannotated* \
         generator's inferred signature. Got {codes:?}"
    );
    // Deliberately NOT pinned either way: tsc additionally reports TS2766 at
    // the delegation itself here (the container will always send `unknown`,
    // which the delegate's `next(value: string)` does not accept). tsz misses
    // it — a pre-existing false negative on the *annotated*-container arm of
    // `check_iterator_next_type_assignability`, unrelated to and unchanged by
    // this fix, which never runs for an annotated generator. Tracked separately
    // so this suite does not bake the gap in as expected behaviour.
}
