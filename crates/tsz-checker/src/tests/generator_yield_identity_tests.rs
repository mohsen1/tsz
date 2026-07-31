//! Regression tests for generator-yield contextual typing over stable lib identity.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes_with_libs(source: &str) -> Vec<u32> {
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

fn checked_js_codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

#[test]
fn local_generator_alias_does_not_contextually_type_yield_operand() {
    let codes = strict_codes_with_libs(
        r#"
export {};

type Generator<Y, R, N> = { fake: Y };

function* gen(): Generator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );

    assert!(
        codes.contains(&7006),
        "module-local Generator alias must not provide the yield contextual type; got: {codes:?}"
    );
}

#[test]
fn local_iterator_alias_does_not_contextually_type_yield_operand() {
    let codes = strict_codes_with_libs(
        r#"
export {};

type Iterator<Y, R, N> = { fake: Y };

function* gen(): Iterator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );

    assert!(
        codes.contains(&7006),
        "module-local Iterator alias must not provide the yield contextual type; got: {codes:?}"
    );
}

#[test]
fn lib_generator_identity_still_contextually_types_yield_operand() {
    let codes = strict_codes_with_libs(
        r#"
function* gen(): Generator<(x: string) => void, void, unknown> {
    yield x => x.toUpperCase();
}
"#,
    );

    assert!(
        !codes.contains(&7006),
        "lib Generator identity should contextually type the yield operand; got: {codes:?}"
    );
}

#[test]
fn generator_declaration_infers_yield_type_for_consumers() {
    // An unannotated `function*` *declaration* infers its yield type from the
    // body just like a generator method/expression. Consuming `.next().value`
    // as an incompatible type must surface TS2322 — previously the declaration's
    // yield collapsed to `any`, hiding the error (and emitting `Generator<any>`
    // in `.d.ts`). Binder names vary across the cases below so the guard tracks
    // the structural rule, not a spelling.
    let codes = strict_codes_with_libs(
        r#"
function* produce() {
    yield "alpha";
}
const wrong: number = produce().next().value;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "generator declaration's inferred string yield must surface TS2322 at a number consumer; got: {codes:?}"
    );
}

#[test]
fn async_generator_declaration_infers_yield_type_for_consumers() {
    let codes = strict_codes_with_libs(
        r#"
async function* stream() {
    yield 42;
}
async function drain() {
    const wrong: string = (await stream().next()).value;
}
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "async generator declaration's inferred number yield must surface TS2322 at a string consumer; got: {codes:?}"
    );
}

#[test]
fn generator_declaration_inferred_yield_accepts_matching_consumer() {
    // A correctly-typed consumer of the inferred yield must stay clean.
    let codes = strict_codes_with_libs(
        r#"
function* emit() {
    yield "beta";
}
const ok: string | void = emit().next().value;
export {};
"#,
    );
    assert!(
        !codes.contains(&2322),
        "a consumer matching the inferred yield type must not error; got: {codes:?}"
    );
}

#[test]
fn empty_generator_declaration_infers_never_yield() {
    // A `function*` declaration with no `yield` infers a non-`any` yield
    // (`Generator<never, void, unknown>`, matching tsc), so `.next().value` is
    // `void`-typed and a number consumer reports TS2322.
    let codes = strict_codes_with_libs(
        r#"
function* drained() {
}
const wrong: number = drained().next().value;
export {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "empty generator declaration must infer a non-any (never) yield; got: {codes:?}"
    );
}

#[test]
fn yield_star_generator_declaration_preserves_checked_js_implicit_any_diagnostics() {
    // `yield*` delegation has its own unresolved inference gap in declaration
    // signatures. The signature recovery pass must not pre-check the body and
    // consume checked-JS implicit-any diagnostics that the real declaration
    // body pass owns.
    let codes = checked_js_codes_with_libs(
        r#"
function* stream() {
    var bucket = []
    while (true) {
        bucket = yield* bucket
    }
}
"#,
    );

    assert!(
        codes.contains(&7005) && codes.contains(&7034),
        "yield* declaration recovery must preserve checked-JS implicit-any diagnostics; got: {codes:?}"
    );
}

#[test]
fn yield_star_array_declaration_infers_the_delegated_element_type() {
    // The signature recovery pass used to bail on *any* `yield*`, leaving an
    // unannotated generator declaration's yield type at `any` and silently
    // dropping every downstream diagnostic. tsc infers `number` here.
    let codes = strict_codes_with_libs(
        r#"
declare function want(x: string): void;
function* fromArray() { yield* [1, 2]; }
for (const v of fromArray()) { want(v); }
export {};
"#,
    );

    assert!(
        codes.contains(&2345),
        "`yield* [1, 2]` in a declaration must infer number, not any; got: {codes:?}"
    );
}

#[test]
fn yield_star_declaration_yield_type_is_not_binder_name_specific() {
    // Same shape, different binder names — the recovery gate is structural,
    // not keyed to any particular identifier.
    let codes = strict_codes_with_libs(
        r#"
declare function accept(x: string): void;
function* zzz() { yield* ["a"]; }
for (const q of zzz()) { accept(q); }
export {};
"#,
    );

    assert!(
        !codes.contains(&2345),
        "string elements must satisfy a string consumer; got: {codes:?}"
    );
}

#[test]
fn mixed_yield_and_yield_star_declaration_unions_both_contributions() {
    let codes = strict_codes_with_libs(
        r#"
declare function want(x: string): void;
function* mixed() { yield 5; yield* [1, 2]; }
for (const v of mixed()) { want(v); }
export {};
"#,
    );

    assert!(
        codes.contains(&2345),
        "a plain yield beside a yield* must still reach the yield union; got: {codes:?}"
    );
}

#[test]
fn self_referential_yield_star_still_defers_to_the_declaration_body_pass() {
    // Negative control for the structural gate: an evolving `var` whose type
    // feeds back through the very `yield*` it delegates to must still skip the
    // recovery pass, so the real body pass owns TS7005/TS7034. Renamed binder
    // relative to the checked-JS guard test above.
    let codes = checked_js_codes_with_libs(
        r#"
function* pump() {
    var reservoir = []
    while (true) {
        reservoir = yield* reservoir
    }
}
"#,
    );

    assert!(
        codes.contains(&7005) && codes.contains(&7034),
        "self-referential yield* must preserve implicit-any diagnostics; got: {codes:?}"
    );
}

#[test]
fn annotated_evolving_binder_does_not_trip_the_self_reference_gate() {
    // An *annotated* local is not an evolving binding, so a `yield*` over it
    // must not disable yield-type recovery.
    let codes = strict_codes_with_libs(
        r#"
declare function want(x: string): void;
function* annotated() {
    const source: number[] = [1, 2];
    yield* source;
}
for (const v of annotated()) { want(v); }
export {};
"#,
    );

    assert!(
        codes.contains(&2345),
        "an annotated local operand must still allow yield recovery; got: {codes:?}"
    );
}
