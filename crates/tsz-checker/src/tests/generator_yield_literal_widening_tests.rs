//! Regression tests for freshness- and context-gated literal widening of an
//! unannotated generator's inferred yield type.
//!
//! `tsc` widens the aggregated yield type only when it collapses to a single
//! *fresh* literal that the contextual yield type does not pin
//! (`getWidenedLiteralLikeTypeForContextualIterationTypeIfNeeded`):
//! - `function* () { yield 1; }` infers `Generator<number, ...>`;
//! - `yield 1 as const` and references to annotated bindings stay literal;
//! - `yield 1; yield 2` keeps the `1 | 2` literal union;
//! - a contextual `Generator<1 | 2, ...>` pins `yield 1` to `1`;
//! - a non-literal contextual yield type (e.g. `T | undefined` from a generic
//!   callback signature) still widens — generatorTypeCheck63;
//! - a fresh enum-member access widens to its parent enum.
//!
//! `yield*` delegation in unannotated generator *declarations* still bails
//! in `infer_generator_declaration_yield_type`, collapsing the inferred
//! yield type to `any`: the bail guards a real circular-inference hazard
//! (an evolving `var`/`let` binding whose type depends on the very yield*
//! aggregate it delegates to — TypeScript's own `yieldExpressionInControlFlow.ts`
//! conformance fixture hits this in plain `.ts`, not just checked-JS) that
//! the pre-pass cannot yet distinguish from an ordinary, non-circular
//! delegate; that family is tracked separately under #15632. `yield*`'s own
//! expression result for an array/tuple delegate is the iterator's
//! `TReturn` — `BuiltinIteratorReturn` in lib.d.ts, an intrinsic that
//! resolves to `undefined` under the default `strictBuiltInIteratorReturn`,
//! not the bare `Iterator<T, TReturn = any, ...>` default (tsz does not
//! model `strictBuiltInIteratorReturn` separately, so `undefined` is the
//! only value implemented). Unannotated generator *expressions* (`const g =
//! function* () { ... }`) used to lose the entire `Generator<...>` shape in
//! declaration emit, independent of `yield*` — fixed at the emitter layer
//! (`crates/tsz-cli`'s `generator_expression_initializer_dts_emit_tests`),
//! since the checker's own inferred type was already correct.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_diagnostics(source: &str) -> Vec<(u32, String)> {
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
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
    .collect()
}

fn strict_codes(source: &str) -> Vec<u32> {
    strict_diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

#[test]
fn bare_literal_yield_widens_to_base_type() {
    // `Generator<number, ...>` is not assignable to `Generator<1, ...>`, so the
    // TS2345 proves the fresh literal was widened.
    let codes = strict_codes(
        r#"
export {};
declare function wantsOne(producer: Generator<1, void, unknown>): void;
function* counts() { yield 1; }
wantsOne(counts());
"#,
    );
    assert!(
        codes.contains(&2345),
        "fresh `yield 1` must widen to number, rejecting Generator<1, ...>; got: {codes:?}"
    );
}

#[test]
fn const_asserted_yield_keeps_literal() {
    let codes = strict_codes(
        r#"
export {};
declare function wantsOne(producer: Generator<1, void, unknown>): void;
function* pinned() { yield 1 as const; }
wantsOne(pinned());
"#,
    );
    assert!(
        codes.is_empty(),
        "`yield 1 as const` must keep the literal 1; got: {codes:?}"
    );
}

#[test]
fn annotated_const_reference_yield_keeps_literal() {
    let codes = strict_codes(
        r#"
export {};
declare function wantsTag(producer: Generator<"start", void, unknown>): void;
const marker: "start" = "start";
function* labels() { yield marker; }
wantsTag(labels());
"#,
    );
    assert!(
        codes.is_empty(),
        "a reference to an annotated literal binding is non-fresh and must not widen; got: {codes:?}"
    );
}

#[test]
fn unannotated_const_reference_yield_widens() {
    // `const flag = 1` is a widening literal binding in tsc; copying it through
    // `yield` still widens.
    let codes = strict_codes(
        r#"
export {};
declare function wantsOne(producer: Generator<1, void, unknown>): void;
const flag = 1;
function* flags() { yield flag; }
wantsOne(flags());
"#,
    );
    assert!(
        codes.contains(&2345),
        "fresh-by-reference const must widen like a bare literal; got: {codes:?}"
    );
}

#[test]
fn multi_literal_yields_keep_literal_union() {
    let codes = strict_codes(
        r#"
export {};
declare function wantsDigits(producer: Generator<1 | 2, void, unknown>): void;
function* digits() { yield 1; yield 2; }
wantsDigits(digits());
"#,
    );
    assert!(
        codes.is_empty(),
        "`yield 1; yield 2` must keep the 1 | 2 literal union; got: {codes:?}"
    );
}

#[test]
fn mixed_fresh_and_const_asserted_same_literal_keeps_literal() {
    // One contribution is pinned by `as const`, so the collapsed literal must
    // not widen even though the other contribution is fresh (tsc keeps `1`).
    let codes = strict_codes(
        r#"
export {};
declare function wantsOne(producer: Generator<1, void, unknown>): void;
function* ones() { yield 1; yield 1 as const; }
wantsOne(ones());
"#,
    );
    assert!(
        codes.is_empty(),
        "a const-asserted contribution must pin the collapsed literal; got: {codes:?}"
    );
}

#[test]
fn contextual_literal_yield_type_pins_literal() {
    // The contextual `Generator<1 | 2, ...>` admits the literal, so `yield 1`
    // stays `1` and the assignment is clean.
    let codes = strict_codes(
        r#"
export {};
const steps: () => Generator<1 | 2, void, unknown> = function* () { yield 1; };
"#,
    );
    assert!(
        codes.is_empty(),
        "a literal contextual yield type must pin the fresh literal; got: {codes:?}"
    );
}

#[test]
fn non_literal_contextual_yield_type_still_widens() {
    // generatorTypeCheck63 witness: the contextual yield type
    // `Cargo | undefined` is not literal-like, so the fresh `yield 1` widens and
    // the whole-argument TS2345 renders `Generator<number, Cargo, any>`.
    let diagnostics = strict_diagnostics(
        r#"
export {};
interface Cargo { weight: number; }
declare function pipeline<T extends Cargo>(
    step: (a: T) => IterableIterator<T | undefined, void>,
): (a: T) => IterableIterator<T | undefined, void>;
const move: (a: Cargo) => IterableIterator<Cargo | undefined, void> =
    pipeline(function* (state: Cargo) {
        yield 1;
        return state;
    });
"#,
    );
    let ts2345 = diagnostics
        .iter()
        .find(|(code, _)| *code == 2345)
        .map(|(_, message)| message.clone())
        .unwrap_or_default();
    assert!(
        ts2345.contains("Generator<number, Cargo, any>"),
        "fresh literal must widen under a non-literal contextual yield type; got: {diagnostics:?}"
    );
}

#[test]
fn enum_member_yield_widens_to_parent_enum() {
    // If the fresh enum-member access widened correctly, the inferred yield
    // type is the parent enum, which is not assignable to the single member.
    let codes = strict_codes(
        r#"
export {};
enum Phase { Start, End }
declare function wantsStart(producer: Generator<Phase.Start, void, unknown>): void;
function* phases() { yield Phase.Start; }
wantsStart(phases());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a fresh enum-member yield must widen to the parent enum; got: {codes:?}"
    );
}

#[test]
fn enum_member_yield_assignable_to_parent_enum() {
    let codes = strict_codes(
        r#"
export {};
enum Phase { Start, End }
declare function wantsPhase(producer: Generator<Phase, void, unknown>): void;
function* phases() { yield Phase.Start; }
wantsPhase(phases());
"#,
    );
    assert!(
        codes.is_empty(),
        "the widened enum yield type must remain assignable to the parent enum; got: {codes:?}"
    );
}

#[test]
fn async_generator_const_asserted_yield_keeps_literal() {
    let codes = strict_codes(
        r#"
export {};
declare function wantsOne(producer: AsyncGenerator<1, void, unknown>): void;
async function* pinned() { yield 1 as const; }
wantsOne(pinned());
"#,
    );
    assert!(
        codes.is_empty(),
        "async generator `yield 1 as const` must keep the literal; got: {codes:?}"
    );
}

#[test]
fn conditional_with_identical_literal_branches_widens() {
    // tsc 6.0.3: `yield flip ? 1 : 1` collapses to the single fresh literal 1
    // and widens to number (unlike the return path's conditional carve-out).
    let codes = strict_codes(
        r#"
export {};
declare const flip: boolean;
declare function wantsOne(producer: Generator<1, void, unknown>): void;
function* coins() { yield flip ? 1 : 1; }
wantsOne(coins());
"#,
    );
    assert!(
        codes.contains(&2345),
        "identical-branch conditional yield must widen to number; got: {codes:?}"
    );
}

#[test]
fn conditional_with_distinct_literal_branches_keeps_union() {
    let codes = strict_codes(
        r#"
export {};
declare const flip: boolean;
declare function wantsDigits(producer: Generator<1 | 2, void, unknown>): void;
function* coins() { yield flip ? 1 : 2; }
wantsDigits(coins());
"#,
    );
    assert!(
        codes.is_empty(),
        "distinct-branch conditional yield must keep the 1 | 2 union; got: {codes:?}"
    );
}

#[test]
fn boolean_literal_yield_widens_to_boolean() {
    let codes = strict_codes(
        r#"
export {};
declare function wantsTrue(producer: Generator<true, void, unknown>): void;
function* toggles() { yield true; }
wantsTrue(toggles());
"#,
    );
    assert!(
        codes.contains(&2345),
        "fresh `yield true` must widen to boolean; got: {codes:?}"
    );
}
