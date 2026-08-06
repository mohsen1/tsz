//! Regression tests for the **async** half of #15632: `yield*` inside an
//! unannotated `async function*` contributes nothing to the inferred yield
//! type when the delegate resolves through `[Symbol.asyncIterator]` (or
//! `[Symbol.iterator]`) property access rather than the solver's array/tuple
//! fast path.
//!
//! This is the same blind spot #16030/#16038 fixed on the **sync** arm of
//! `dispatch/yield_::check_yield_expression`, left standing on the async arm:
//! `get_iterator_info` is a pure structural solver query whose property lookup
//! cannot evaluate through the `TypeData::Lazy(DefId)` alias body that every
//! non-array/tuple lib iterable (`AsyncGenerator<T>`, `AsyncIterable<T>`,
//! `Generator<T>`, `Set<T>`, `string`) exposes its iterator member behind. It
//! answers `None`, the async arm gated its `push_generator_yield_contribution`
//! on `async_info.is_some()`, and so the aggregated yield type collapsed to
//! `any` — silently accepting a mismatched `AsyncGenerator` instantiation.
//!
//! Every delegate below is fed to a parameter typed as a deliberately **wrong**
//! `AsyncGenerator` instantiation, so a missing `TS2345` means the delegate's
//! contribution collapsed to `any` instead of its real element type. Each row
//! was oracled against `tsc@7.0.2`
//! (`--noEmit --strict --pretty false --target es2018 --lib es2018,dom`),
//! which reports TS2345 on every negative row and nothing on the controls.
//!
//! The controls are the load-bearing part of this suite. Matching "tsc reports
//! TS2345" is reachable by any change that makes the inferred yield type
//! *narrower*, including a wrong one, so the suite also pins:
//!
//! * `correct_instantiation_stays_clean` — the same delegate against the
//!   *right* instantiation must report nothing, which fails for any fix that
//!   widens or mis-resolves the element type rather than resolving it exactly.
//! * `contextual_generic_delegate_*` — the witness that blocked the previous
//!   attempt at this arm. An **annotated** containing generator delegating to
//!   an uninstantiated generic (`yield* g()` for a bare
//!   `<T>() => AsyncGenerator<T>`) must stay clean: `tsc` threads the
//!   container's contextual yield type into the delegate call so `T` infers to
//!   the container's element type. tsz does not thread it, so resolving the
//!   delegate's element type structurally answers `unknown` — which is only
//!   harmless as long as the *annotated* container never reads the pushed
//!   contribution. That separation is exactly what these two rows pin.

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
fn async_generator_binding_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncGenerator<string>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "AsyncGenerator<string> delegate must contribute `string`, not widen to `any`: {codes:?}"
    );
}

#[test]
fn async_iterable_binding_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncIterable<string>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "AsyncIterable<string> delegate must contribute `string`: {codes:?}"
    );
}

#[test]
fn sync_generator_delegate_in_async_generator_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Generator<string>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a sync Generator<string> delegate inside an async generator must contribute `string`: {codes:?}"
    );
}

#[test]
fn set_delegate_in_async_generator_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare const src: Set<string>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "Set<string> delegate inside an async generator must contribute `string`: {codes:?}"
    );
}

#[test]
fn async_generator_call_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare function src(): AsyncGenerator<string>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* src();
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a call returning AsyncGenerator<string> must contribute `string`: {codes:?}"
    );
}

#[test]
fn string_delegate_in_async_generator_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* "ab";
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a string delegate inside an async generator must contribute `string`: {codes:?}"
    );
}

#[test]
fn async_generator_declaration_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncGenerator<string>;
declare function wants(g: AsyncGenerator<number>): void;
async function* d() {
    yield* src;
}
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the declaration form must contribute the delegate's element type too: {codes:?}"
    );
}

/// The plain `yield` here contributes the *same* type the target asks for, so
/// the delegated contribution is the only thing that can produce `TS2345`.
/// Written this way deliberately: the obvious spelling (`yield "a"` against an
/// `AsyncGenerator<number>` target) also reports on unfixed `main`, off the
/// plain yield alone, and would have graded a no-op as a fix.
#[test]
fn delegated_contribution_survives_alongside_a_plain_yield() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncGenerator<string>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield 1;
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a delegated contribution must survive alongside a plain `yield`: {codes:?}"
    );
}

/// The other side of the same witness: both contributions must land, so the
/// union target accepting *both* has to stay clean. A fix that let the delegate
/// *replace* the plain yield rather than join it would pass the row above and
/// fail this one.
#[test]
fn plain_and_delegated_yields_union_rather_than_replace() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncGenerator<string>;
declare function wants(g: AsyncGenerator<number | string>): void;
const d = async function* () {
    yield 1;
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "plain and delegated yields must union into `number | string`: {codes:?}"
    );
}

#[test]
fn array_literal_delegate_stays_correct() {
    let codes = strict_codes(
        r#"
export {};
declare function wants(g: AsyncGenerator<string>): void;
const d = async function* () {
    yield* [1, 2];
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the array/tuple fast path must keep contributing its element type: {codes:?}"
    );
}

#[test]
fn correct_instantiation_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncGenerator<string>;
declare function wants(g: AsyncGenerator<string>): void;
const d = async function* () {
    yield* src;
};
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "the matching instantiation must stay clean — a narrower-but-wrong yield type would also satisfy the TS2345 rows: {codes:?}"
    );
}

#[test]
fn alias_wrapped_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
type Feed = AsyncGenerator<string>;
declare const zzSource: Feed;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a delegate behind a user type alias must contribute `string`: {codes:?}"
    );
}

#[test]
fn user_defined_async_iterable_class_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
class Chunks {
    async *[Symbol.asyncIterator](): AsyncGenerator<string> {
        yield "a";
    }
}
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* new Chunks();
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a user-defined [Symbol.asyncIterator] class must contribute `string`: {codes:?}"
    );
}

#[test]
fn user_defined_async_iterable_class_delegate_correct_instantiation_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
class Chunks {
    async *[Symbol.asyncIterator](): AsyncGenerator<string> {
        yield "a";
    }
}
declare function wants(g: AsyncGenerator<string>): void;
const d = async function* () {
    yield* new Chunks();
};
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "the matching instantiation of the user-defined class delegate must stay clean: {codes:?}"
    );
}

#[test]
fn instantiated_generic_delegate_contributes_its_argument() {
    let codes = strict_codes(
        r#"
export {};
declare function pull<Elem>(seed: Elem): AsyncGenerator<Elem>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* pull("a");
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "an instantiated generic delegate must contribute its inferred argument: {codes:?}"
    );
}

/// Still red on `main` and still red with this fix — a **separate** defect,
/// kept here rather than deleted because the shape belongs in this matrix.
/// The inner generator expression's own inferred yield type is correct (the
/// non-nested spelling of this row passes), but it does not survive being
/// yielded from the outer generator: tsz reports nothing where `tsc` reports
/// TS2345. The contribution this PR fixes is already landing; what fails is
/// downstream of it.
#[test]
#[ignore = "pre-existing, unrelated to the yield* contribution: see the doc comment"]
fn nested_async_generator_expression_delegate_contributes_element_type() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
declare function wants(g: AsyncGenerator<AsyncGenerator<number>>): void;
const d = async function* () {
    yield (async function* () {
        yield* zzSource;
    })();
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "the inner generator's inferred yield type must survive one nesting level: {codes:?}"
    );
}

/// `tsc` awaits a sync delegate's element type inside an async generator
/// (`IterationUse.AsyncYieldStar`), so `Iterable<Promise<string>>` contributes
/// `string`, not `Promise<string>`. This pins the *awaited* half of the reused
/// `for await..of` query: a fallback that resolved the sync element type
/// without awaiting would report here and stay clean on the row below.
#[test]
fn sync_iterable_of_promises_contributes_the_awaited_element() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Iterable<Promise<string>>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a sync iterable of promises must contribute the awaited element: {codes:?}"
    );
}

#[test]
fn sync_iterable_of_promises_correct_instantiation_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Iterable<Promise<string>>;
declare function wants(g: AsyncGenerator<string>): void;
const d = async function* () {
    yield* zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "`Iterable<Promise<string>>` must contribute `string`, not `Promise<string>`: {codes:?}"
    );
}

/// The union axis of this family. Both rows were previously blocked on two
/// independent defects on the union arm: a spurious **TS1320** raised by
/// `async_iterator_has_invalid_thenable_next_result` (fixed by #16116, which
/// distributes that predicate over union members), and the contribution itself
/// collapsing to `any` because `get_iterator_info` never distributes over a
/// union delegate. Both are now fixed — the `yield*` element resolution routes
/// through the checker's env-aware, union-distributing chain — so these rows
/// are live regression guards.
#[test]
fn union_delegate_contributes_the_union_of_element_types() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string> | AsyncGenerator<boolean>;
declare function wants(g: AsyncGenerator<number>): void;
const d = async function* () {
    yield* zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.contains(&2345),
        "a union delegate must contribute the union of its element types: {codes:?}"
    );
}

#[test]
fn union_delegate_correct_instantiation_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string> | AsyncGenerator<boolean>;
declare function wants(g: AsyncGenerator<string | boolean>): void;
const d = async function* () {
    yield* zzSource;
};
wants(d());
"#,
    );
    assert!(
        codes.is_empty(),
        "the union delegate must contribute exactly `string | boolean`: {codes:?}"
    );
}

/// The fallback must not make a non-iterable delegate resolvable: `TS2504` is
/// reported before the contribution is computed and must be unmoved.
#[test]
fn non_async_iterable_delegate_still_reports_ts2504() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: number;
const d = async function* () {
    yield* zzSource;
};
"#,
    );
    assert!(
        codes.contains(&2504),
        "a non-async-iterable delegate must still report TS2504: {codes:?}"
    );
}

#[test]
fn contextual_generic_delegate_declaration_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare function g<T>(): AsyncGenerator<T>;
async function* outer(): AsyncGenerator<number> {
    yield* g();
}
"#,
    );
    assert!(
        codes.is_empty(),
        "an annotated async generator delegating to an uninstantiated generic must not report: {codes:?}"
    );
}

#[test]
fn contextual_generic_delegate_expression_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare function g<T>(): AsyncGenerator<T>;
const outer = async function* (): AsyncGenerator<number> {
    yield* g();
};
"#,
    );
    assert!(
        codes.is_empty(),
        "the expression form of the contextual-generic witness must not report either: {codes:?}"
    );
}
