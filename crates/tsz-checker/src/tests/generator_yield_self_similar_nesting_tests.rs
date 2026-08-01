//! Regression matrix for #16116 item 2, **re-scoped**: an unannotated
//! generator that yields a value whose type is *the same generator kind as the
//! container* loses that value's type argument.
//!
//! The issue files this as "the inner generator expression's inferred yield
//! type is lost through one nesting level", which points at the inner
//! generator's own inference. It is not: the same loss reproduces with **no
//! inference on the yielded side at all**, from a `declare const` of a fully
//! written-out `AsyncGenerator` type (`declared_async_generator_operand_*`
//! below). What actually decides the outcome is whether the operand's
//! constructor is the *same* alias as the containing generator's:
//!
//! | operand type                    | container      | result  |
//! | ---                             | ---            | ---     |
//! | `AsyncGenerator[ string ]`      | async `function*` | **lost** |
//! | `Generator[ string ]`           | sync `function*`  | **lost** |
//! | `Generator[ string ]`           | async `function*` | correct |
//! | `AsyncIterable`/`AsyncIterator` | async `function*` | correct |
//! | `Iterator`/`Set`/`Promise`/`T[]`/object | async `function*` | correct |
//!
//! The relation is not at fault either: spelling the expected container type
//! out by hand and assigning it (`written_out_*_relation_still_reports`)
//! reports the mismatch in exactly the shape the inferred container should
//! have had. So the loss is in the *container's inferred yield type*, and it is
//! specific to a self-similar (same-alias-nested-in-itself) application.
//!
//! Every row is oracled against `tsc@7.0.2`
//! (`--noEmit --strict --pretty false --target es2018 --lib es2018,dom`).
//! Each negative row feeds the container to a deliberately **wrong**
//! instantiation, so a missing `TS2345` means the operand's type argument was
//! degraded rather than contributed. The controls are what make that
//! inference sound: they pin the same shape one alias away, where tsz is
//! already correct, so a "fix" that widens or drops types wholesale fails them.

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

/// The minimal witness for #16116 item 2, with the issue's inner generator
/// expression replaced by a `declare const` — no inference on the yielded side
/// at all, and the defect is unchanged. `tsc` reports TS2345 here.
#[test]
#[ignore = "#16116 item 2: a self-similar `AsyncGenerator` operand loses its type argument"]
fn declared_async_generator_operand_contributes_its_type_argument() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a yielded `AsyncGenerator<string>` must contribute its own type argument: {codes:?}"
    );
}

/// The sync twin of the row above. Pins that this is a property of the
/// self-similar nesting rather than anything async-specific: no `await`, no
/// `[Symbol.asyncIterator]`, same loss.
#[test]
#[ignore = "#16116 item 2: a self-similar `Generator` operand loses its type argument"]
fn declared_sync_generator_operand_contributes_its_type_argument() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Generator<string>;
declare function wants(h: Generator<Generator<number>>): void;
const d = function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a yielded `Generator<string>` must contribute its own type argument: {codes:?}"
    );
}

/// The load-bearing control: a **sync** `Generator` operand inside an **async**
/// container is one alias away from the failing row and is already correct on
/// `main`. This is the row that localises the defect to same-alias nesting; a
/// diagnosis that blamed "yielding an iterable" or "yielding a generator"
/// would predict this fails too.
#[test]
fn sync_generator_operand_in_async_container_is_already_correct() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Generator<string>;
declare function wants(h: AsyncGenerator<Generator<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a `Generator` operand in an async container must keep its argument: {codes:?}"
    );
}

/// `AsyncIterable` is the same structural shape as `AsyncGenerator` minus the
/// generator members, and is correct today.
#[test]
fn async_iterable_operand_is_already_correct() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncIterable<string>;
declare function wants(h: AsyncGenerator<AsyncIterable<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "an `AsyncIterable` operand must keep its argument: {codes:?}"
    );
}

/// A non-iterable generic operand: rules out "any nested generic argument is
/// dropped".
#[test]
fn promise_operand_is_already_correct() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Promise<string>;
declare function wants(h: AsyncGenerator<Promise<number>>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a `Promise` operand must keep its argument: {codes:?}"
    );
}

/// The container's *own* yield argument is not degraded — only the operand's
/// nested one. Feeding the same container to a target whose yield type is not
/// a generator at all still reports, which is why the failing rows above
/// cannot be explained by "the whole inferred container collapsed to `any`".
#[test]
fn self_similar_container_still_rejects_a_non_generator_yield_type() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<{ x: number }>): void;
const d = async function* () {
    yield zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the container's own yield type must still be a generator: {codes:?}"
    );
}

/// The relation is exonerated: the type the failing row *should* have inferred,
/// written out by hand, is rejected against the same target. Whatever is lost
/// is lost while building the container's yield type, not while comparing it.
#[test]
fn written_out_self_similar_relation_still_reports() {
    let codes = strict_codes(
        r#"
export {};
declare const a: AsyncGenerator<AsyncGenerator<string>, void, unknown>;
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
wants(a);
"#,
    );
    assert!(
        codes.contains(&2345),
        "the written-out self-similar relation must report: {codes:?}"
    );
}

/// The sync half of the exoneration above.
#[test]
fn written_out_sync_self_similar_relation_still_reports() {
    let codes = strict_codes(
        r#"
export {};
declare const a: Generator<Generator<string>, void, unknown>;
declare function wants(h: Generator<Generator<number>>): void;
wants(a);
"#,
    );
    assert!(
        codes.contains(&2345),
        "the written-out sync self-similar relation must report: {codes:?}"
    );
}

/// A non-nested container is correct, so the defect needs the nesting — this is
/// the depth-1 baseline the failing rows are the depth-2 form of.
#[test]
fn non_nested_container_is_already_correct() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(h: AsyncGenerator<number>): void;
const d = async function* () {
    yield* zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the non-nested delegate row must stay correct: {codes:?}"
    );
}

/// An **annotated** operand-producing generator expression is unaffected, which
/// is why the issue's original framing (inner-generator inference) looked
/// plausible: annotating the inner makes the row pass. It passes because the
/// annotated inner's type reaches the container by a different construction
/// path, not because inference of the inner was ever the problem — the
/// `declare const` rows above have no inner inference and still fail.
#[test]
fn annotated_inner_generator_expression_is_already_correct() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(h: AsyncGenerator<AsyncGenerator<number>>): void;
const d = async function* () {
    yield (async function* (): AsyncGenerator<string> {
        yield "a";
    })();
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "an annotated inner generator expression must stay correct: {codes:?}"
    );
}
