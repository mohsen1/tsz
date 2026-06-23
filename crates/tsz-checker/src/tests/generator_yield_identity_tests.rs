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
